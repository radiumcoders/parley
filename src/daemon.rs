//! The background process: push-to-talk turns, pasting, and reacting to Claude Code hooks.
//!
//! `parley start`/`stop`/`toggle` and `parley hook` talk to it over a Unix socket.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot};

use crate::audio::{self, Chime, Mic, chime};
use crate::cleanup::Cleaner;
use crate::commands::{self, Command};
use crate::config::{self, Cleanup, Settings, Submit};
use crate::desktop::{self, Window, notify};
use crate::listen::{self, Mode};
use crate::overlay::Ui;
use crate::{history, project, speak, words};

/// A prompt submitted this soon after a paste into the same window was ours: that's how a
/// Claude session becomes one you talk to.
const OWN_PROMPT_WINDOW: Duration = Duration::from_secs(5);
/// A prompt sent this soon after a paste into its window may be the paste, fixed.
const LEARN_WINDOW: Duration = Duration::from_secs(600);

enum Event {
    Start,
    Stop,
    Toggle,
    Cancel,
    Hook(Value),
    Finished {
        id: u64,
        result: Result<listen::Turn>,
        target: Option<Window>,
    },
    /// A spoken answer ended (or failed); `asks` if it ended with a question.
    Spoke {
        window: String,
        asks: bool,
    },
    /// Time to close the microphone, unless a turn has used it since.
    CoolMic(u64),
    /// A spoken command's keys were pressed (or not).
    Ran {
        command: Command,
        ok: bool,
    },
    /// A turn was pasted into `window` (or not, if `!ok`).
    Delivered {
        text: String,
        window: Option<String>,
        voice: bool,
        ok: bool,
    },
}

struct Recording {
    id: u64,
    mode: Mode,
    /// Still held down (`Hold` turns only).
    release: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

/// A Claude session you've talked to.
struct Session {
    window: String,
    cwd: Option<PathBuf>,
}

/// The last text parley pasted, to learn from if you fix it before sending.
struct Pasted {
    text: String,
    window: String,
    at: Instant,
}

struct Daemon {
    events: mpsc::UnboundedSender<Event>,
    recording: Option<Recording>,
    next_id: u64,
    /// Claude sessions you've talked to. Sessions you've never spoken to don't chime,
    /// notify, speak or open the microphone.
    sessions: HashMap<String, Session>,
    last_paste: Option<Pasted>,
    /// Windows whose next answer you asked to hear.
    voice_replies: HashSet<String>,
    speaking: Option<tokio::task::JoinHandle<()>>,
    /// Open between turns for `PARLEY_WARM_MIC`, so a turn starts with what came just
    /// before the key press.
    mic: Option<Mic>,
    /// Bumped by every turn, so only the last one's cool-down closes the microphone.
    mic_turns: u64,
    ui: Ui,
    deliveries: mpsc::UnboundedSender<Delivery>,
}

pub async fn run() -> Result<()> {
    let path = config::socket_path();
    if UnixStream::connect(&path).await.is_ok() {
        log("already running");
        return Ok(());
    }
    let _ = std::fs::remove_file(&path);
    let listener =
        UnixListener::bind(&path).with_context(|| format!("binding {}", path.display()))?;
    log(&format!("listening on {}", path.display()));

    let (tx, mut rx) = mpsc::unbounded_channel();
    tokio::spawn(serve(listener, tx.clone()));
    let (deliveries, to_deliver) = mpsc::unbounded_channel();
    tokio::spawn(deliver(to_deliver, tx.clone()));
    let mut d = Daemon {
        events: tx,
        recording: None,
        next_id: 0,
        sessions: HashMap::new(),
        last_paste: None,
        voice_replies: HashSet::new(),
        speaking: None,
        mic: None,
        mic_turns: 0,
        ui: Ui::new(),
        deliveries,
    };
    while let Some(event) = rx.recv().await {
        match event {
            Event::Start => d.start(Mode::Hold, None).await,
            Event::Stop => d.stop(),
            Event::Toggle => {
                if d.recording.as_ref().is_some_and(|r| r.release.is_some()) {
                    d.stop();
                } else {
                    d.start(Mode::Hold, None).await;
                }
            }
            Event::Cancel => d.cancel(),
            Event::Hook(v) => d.hook(v).await,
            Event::Finished { id, result, target } => d.finished(id, result, target).await,
            Event::Spoke { window, asks } => {
                d.speaking = None;
                d.ui.hide();
                if asks && Settings::load().auto_answer && d.recording.is_none() {
                    d.start(Mode::Answer, Some(window)).await;
                }
            }
            Event::Delivered {
                text,
                window,
                voice,
                ok,
            } => d.delivered(text, window, voice, ok),
            Event::Ran { command, ok } => {
                if d.recording.is_none() {
                    match ok {
                        true => d.ui.info(command.describe()),
                        false => d.ui.error("Couldn't press the keys"),
                    }
                }
            }
            Event::CoolMic(turn) => {
                if turn == d.mic_turns && d.recording.is_none() && d.mic.take().is_some() {
                    log("microphone closed");
                }
            }
        }
    }
    Ok(())
}

impl Daemon {
    async fn start(&mut self, mode: Mode, target: Option<String>) {
        if let Some(r) = self.recording.take() {
            r.task.abort(); // a key press wins over an answer turn that opened by itself
        }
        if let Some(s) = self.speaking.take() {
            s.abort(); // talking over an answer stops it
        }
        let pressed = tokio::time::Instant::now();
        let settings = Settings::load();
        let Some(key) = settings.api_key.clone() else {
            log("no GRADIUM_API_KEY in ~/.config/parley/env");
            notify("parley", "Add GRADIUM_API_KEY=... to ~/.config/parley/env");
            self.ui.error("Add GRADIUM_API_KEY to ~/.config/parley/env");
            return;
        };
        // The microphone opens before anything else, so the first word isn't lost while
        // parley finds the window, its project and the words to boost. A warm one also
        // has the moment before the key press.
        self.mic_turns += 1;
        let warm = self.mic.as_ref().is_some_and(Mic::alive);
        if !warm {
            self.mic = match Mic::open(settings.mic.as_deref()) {
                Ok(mic) => Some(mic),
                Err(e) => {
                    log(&format!("{e:#}"));
                    notify("parley", "Couldn't open the microphone");
                    self.ui.error("Couldn't open the microphone");
                    return;
                }
            };
        }
        // Not for an answer turn: just before it, Claude's answer was being read aloud.
        let mic = self.mic.as_ref().unwrap().listen(mode == Mode::Hold);
        self.ui.recording(mode == Mode::Answer);
        let target = match target {
            Some(address) => desktop::window(&address).await,
            None => desktop::active_window().await,
        };
        let known_cwd = target.as_ref().and_then(|w| self.known_cwd(w));
        let window = target.clone();
        self.next_id += 1;
        let id = self.next_id;
        let (release_tx, release_rx) = oneshot::channel();
        let events = self.events.clone();
        let ui = self.ui.clone();
        let task = tokio::spawn(async move {
            let client = gradium::Client::new(&key);
            let config = tokio::task::spawn_blocking(move || {
                let words = window
                    .and_then(|w| project::window_project(known_cwd, &w))
                    .map(|(_, root)| project::file_words(&project::project_files(&root)))
                    .unwrap_or_default();
                settings.listen_config(&words)
            })
            .await
            .expect("listen config");
            let result = listen::turn(&client, &config, mode, mic, release_rx, pressed, &ui).await;
            let _ = events.send(Event::Finished { id, result, target });
        });
        chime(Chime::Listen);
        log(&format!(
            "listening ({mode:?}, {} microphone)",
            if warm { "warm" } else { "cold" }
        ));
        let release = (mode == Mode::Hold).then_some(release_tx);
        self.recording = Some(Recording {
            id,
            mode,
            release,
            task,
        });
    }

    fn stop(&mut self) {
        if let Some(release) = self.recording.as_mut().and_then(|r| r.release.take()) {
            let _ = release.send(());
            chime(Chime::Done);
        }
    }

    /// Throws the turn away, sending nothing.
    fn cancel(&mut self) {
        if let Some(r) = self.recording.take() {
            r.task.abort();
            chime(Chime::Done);
            self.ui.info("Cancelled");
            log("cancelled");
            self.cool_mic();
        } else if let Some(s) = self.speaking.take() {
            s.abort();
            self.ui.hide();
        }
    }

    /// Keeps the microphone open for `PARLEY_WARM_MIC` after a turn, then closes it.
    fn cool_mic(&mut self) {
        let warm = Settings::load().warm_mic;
        if warm.is_zero() {
            self.mic = None;
        } else if warm != Duration::MAX {
            let (turn, events) = (self.mic_turns, self.events.clone());
            tokio::spawn(async move {
                tokio::time::sleep(warm).await;
                let _ = events.send(Event::CoolMic(turn));
            });
        }
    }

    async fn finished(&mut self, id: u64, result: Result<listen::Turn>, target: Option<Window>) {
        let Some(recording) = self.recording.take_if(|r| r.id == id) else {
            return;
        };
        self.cool_mic();
        if recording.mode == Mode::Answer {
            chime(Chime::Done);
        }
        let said = match result {
            Ok(turn) => {
                log(&turn.timings);
                save_turn(&turn.audio);
                turn.text
            }
            Err(e) => {
                let e = format!("{e:#}");
                log(&format!("error: {e}"));
                let first = e.lines().next().unwrap_or_default();
                notify("parley", first);
                self.ui.error(first);
                return;
            }
        };
        if said.is_empty() {
            log("heard nothing");
            if recording.mode == Mode::Hold {
                self.ui.info("Heard nothing");
            } else {
                self.ui.hide();
            }
            return;
        }
        log(&format!("heard: {said}"));
        let settings = Settings::load();
        // Prompts go to coding agents; anywhere else parley is plain dictation.
        let agent = match &target {
            Some(w) => self.is_agent(w).await,
            None => false,
        };

        let command = agent.then(|| commands::command(&said)).flatten();
        let files = match &target {
            Some(w) if command.is_none() => self.files_named(&said, w).await,
            Some(w) => self.files_named(&said, w).await,
            _ => Vec::new(),
        };
        let cleanup_key = settings.anthropic_key.clone();
        if command.is_none() && settings.cleanup != Cleanup::Off && cleanup_key.is_none() {
            log("no ANTHROPIC_API_KEY in ~/.config/parley/env: sending it as said");
        }
        let clean = command.is_none()
            && cleanup_key.is_some()
            && match settings.cleanup {
                Cleanup::Always => true,
                Cleanup::Auto => needs_cleanup(&said, &files),
                Cleanup::Off => false,
            };
        if clean {
            self.ui.working("Cleaning up");
        }
        // The cleanup and paste happen elsewhere, so the next key press is heard at once.
        let _ = self.deliveries.send(Delivery {
            cleanup_key,
            command,
            said,
            files,
            clean,
            agent,
            submit: match settings.submit {
                Submit::Always => true,
                Submit::Agents => agent,
                Submit::Never => false,
            },
            dry_run: settings.dry_run,
            language: settings.language_name(),
            target,
        });
    }

    /// A turn was pasted (or not).
    fn delivered(&mut self, text: String, window: Option<String>, voice: bool, ok: bool) {
        let showing = self.recording.is_none(); // not over the next turn
        if !ok {
            if showing {
                self.ui
                    .error("Couldn't paste: it's in the history (parley history)");
            }
            return;
        }
        if showing {
            self.ui.done(&text);
        }
        if let Some(window) = window {
            if voice {
                log("will speak the answer");
                self.voice_replies.insert(window.clone());
            }
            self.last_paste = Some(Pasted {
                text,
                window,
                at: Instant::now(),
            });
        }
    }

    /// A Claude session you've talked to lives in `window`, or a coding agent runs in it
    /// (by its title, when the window's process has other windows too).
    async fn is_agent(&self, window: &Window) -> bool {
        if self.sessions.values().any(|s| s.window == window.address) {
            return true;
        }
        if desktop::shares_process(window).await {
            return desktop::agent_title(&window.title);
        }
        let Some(pid) = window.pid else { return false };
        tokio::task::spawn_blocking(move || project::agent_in(pid))
            .await
            .unwrap_or(false)
    }

    /// The project files `said` names, as Claude Code in that window refers to them.
    async fn files_named(&self, said: &str, window: &Window) -> Vec<String> {
        let known = self.known_cwd(window);
        let (said, window) = (said.to_string(), window.clone());
        tokio::task::spawn_blocking(move || {
            let (cwd, root) = project::window_project(known, &window)?;
            let files = project::project_files(&root);
            let named: Vec<String> = project::file_candidates(&said, &files)
                .iter()
                .map(|f| project::relative_to_cwd(&root, f, &cwd))
                .collect();
            if !named.is_empty() {
                log(&format!("files it may name: {}", named.join(", ")));
            }
            Some(named)
        })
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
    }

    /// The working directory of the Claude session you talked to in `window`, if any.
    fn known_cwd(&self, window: &Window) -> Option<PathBuf> {
        self.sessions
            .values()
            .find(|s| s.window == window.address)
            .and_then(|s| s.cwd.clone())
    }

    async fn hook(&mut self, v: Value) {
        let event = v["hook_event_name"].as_str().unwrap_or_default();
        let Some(session) = v["session_id"].as_str() else {
            return;
        };
        let active = desktop::active_window().await.map(|w| w.address);

        if event == "UserPromptSubmit" {
            let Some(active) = active else { return };
            let ours = self
                .last_paste
                .as_ref()
                .is_some_and(|p| p.at.elapsed() < OWN_PROMPT_WINDOW && p.window == active);
            // The first prompt sent from the window parley pasted into: if you fixed the
            // paste before sending it, learn the misheard words.
            if let Some(p) = self
                .last_paste
                .take_if(|p| p.window == active && p.at.elapsed() < LEARN_WINDOW)
                && let Some(prompt) = v["prompt"].as_str()
                && p.text.trim() != prompt.trim()
            {
                learn(p.text, prompt.to_string(), self.ui.clone());
            }
            if ours || self.sessions.contains_key(session) {
                // Typing in the session later keeps its window current.
                let cwd = v["cwd"].as_str().map(PathBuf::from);
                self.sessions.insert(
                    session.to_string(),
                    Session {
                        window: active,
                        cwd,
                    },
                );
            }
            return;
        }

        let Some(window) = self.sessions.get(session).map(|s| s.window.clone()) else {
            return;
        };
        let away = active.as_deref() != Some(window.as_str());
        match event {
            "Stop" if v["stop_hook_active"] != true => {
                let message = v["last_assistant_message"].as_str().unwrap_or_default();
                if self.voice_replies.remove(&window) {
                    self.say(message, window);
                } else if asks(message) {
                    log("agent asked");
                    chime(Chime::Ask);
                    if away {
                        notify("Claude is asking", last_paragraph(message));
                    }
                    if Settings::load().auto_answer && self.recording.is_none() {
                        self.start(Mode::Answer, Some(window)).await;
                    }
                } else {
                    log("agent finished");
                    chime(Chime::Finished);
                    if away {
                        notify("Claude is done", &first_line(message));
                    }
                }
            }
            "Notification" => {
                let message = v["message"].as_str().unwrap_or_default();
                // Idle reminders repeat what the Stop hook already said.
                if v["notification_type"] == "idle_prompt"
                    || message.contains("waiting for your input")
                {
                    return;
                }
                chime(Chime::Ask);
                if away {
                    notify("Claude needs you", message);
                }
            }
            "PreToolUse" if v["tool_name"] == "AskUserQuestion" => {
                chime(Chime::Ask);
                if away {
                    let q = v["tool_input"]["questions"][0]["question"]
                        .as_str()
                        .unwrap_or("A question for you");
                    notify("Claude is asking", q);
                }
            }
            _ => {}
        }
    }

    /// Reads Claude's answer aloud; if it ends with a question, the microphone opens
    /// after it (not during, or it would hear itself).
    fn say(&mut self, message: &str, window: String) {
        let settings = Settings::load();
        let Some(key) = settings.api_key else { return };
        let (message, asks) = (message.to_string(), asks(message));
        let events = self.events.clone();
        log("speaking the answer");
        self.ui.speaking();
        self.speaking = Some(tokio::spawn(async move {
            let client = gradium::Client::new(&key);
            if let Err(e) = speak::speak(&client, &settings.voice, &message).await {
                log(&format!("speaking: {e:#}"));
                notify("Claude answered", &first_line(&message));
            }
            let _ = events.send(Event::Spoke { window, asks });
        }));
    }
}

/// What a turn sends, handed from the event loop to `deliver`.
struct Delivery {
    cleanup_key: Option<String>,
    /// Keys to press instead of pasting.
    command: Option<Command>,
    said: String,
    files: Vec<String>,
    clean: bool,
    agent: bool,
    submit: bool,
    dry_run: bool,
    language: &'static str,
    target: Option<Window>,
}

/// Cleans up and pastes turns, one at a time and in order.
async fn deliver(mut rx: mpsc::UnboundedReceiver<Delivery>, events: mpsc::UnboundedSender<Event>) {
    while let Some(d) = rx.recv().await {
        if let Some(command) = d.command {
            let ok = if d.dry_run {
                log(&format!("dry run: would run {command:?}"));
                true
            } else {
                focus(d.target.as_ref()).await;
                match run_command(&command).await {
                    Ok(()) => {
                        log(&format!("ran {command:?}"));
                        true
                    }
                    Err(e) => {
                        log(&format!("{command:?}: {e:#}"));
                        false
                    }
                }
            };
            let _ = events.send(Event::Ran { command, ok });
            continue;
        }
        let mut text = if let Some(key) = d.cleanup_key.as_deref().filter(|_| d.clean) {
            clean_up(key, d.said.clone(), d.files).await
        } else {
            d.said.clone()
        };
        let voice = d.agent && (commands::wants_voice(&d.said) || commands::wants_voice(&text));
        if voice {
            text += &commands::voice_note(d.language);
        }
        let pasted = if d.dry_run {
            let send = if d.submit { " and send" } else { "" };
            log(&format!("dry run: would paste{send}: {text}"));
            Ok(())
        } else {
            focus(d.target.as_ref()).await;
            desktop::paste(&text, d.submit).await
        };
        history::add(&d.said, &text, d.target.as_ref(), pasted.is_ok());
        if let Err(e) = &pasted {
            log(&format!("paste: {e:#}"));
            notify("parley couldn't paste", &text);
        }
        let _ = events.send(Event::Delivered {
            text,
            window: d.target.map(|w| w.address),
            voice,
            ok: pasted.is_ok(),
        });
    }
}

/// Focuses `target` if you've moved away from it.
async fn focus(target: Option<&Window>) {
    let Some(target) = target else { return };
    let focused = desktop::active_window().await.map(|w| w.address);
    if focused.as_ref() != Some(&target.address)
        && let Err(e) = desktop::focus(&target.address).await
    {
        log(&format!("{e:#}"));
    }
}

async fn run_command(command: &Command) -> Result<()> {
    match command {
        Command::Stop => desktop::wtype(&["-k", "Escape"]).await,
        Command::Rewind => {
            desktop::wtype(&["-k", "Escape"]).await?;
            tokio::time::sleep(Duration::from_millis(80)).await;
            desktop::wtype(&["-k", "Escape"]).await
        }
        Command::Slash(slash) => {
            desktop::wtype(&[slash]).await?;
            // Let Claude Code's command menu catch up before Enter.
            tokio::time::sleep(Duration::from_millis(150)).await;
            desktop::wtype(&["-k", "Return"]).await
        }
    }
}

/// Short, plain turns ("Run the tests.") are pasted as Gradium wrote them, which already
/// has punctuation and capitals: the cleanup would take a second to change nothing. It
/// runs for anything longer, anything with a self-correction, filler or list, and
/// anything that names a file.
fn needs_cleanup(said: &str, files: &[String]) -> bool {
    static CUES: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(
            r"(?i)\b(um+|uh+|er+m?|hmm+|wait|actually|scratch that|i mean|sorry|no no|rather|instead|first|second|third|bullet|new line|new paragraph|heading|period|comma)\b",
        )
        .unwrap()
    });
    !files.is_empty() || said.split_whitespace().count() > 12 || CUES.is_match(said)
}

/// Keeps the audio of the last few turns in ~/.local/state/parley/turns, to replay when a
/// transcript comes out wrong.
fn save_turn(pcm: &[u8]) {
    const KEEP: usize = 10;
    let dir = config::turns_dir();
    if pcm.is_empty() || std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    if let Err(e) = audio::save_wav(&dir.join(format!("{stamp}.wav")), pcm) {
        log(&format!("saving the turn's audio: {e}"));
    }
    let mut saved: Vec<_> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "wav"))
        .collect();
    saved.sort();
    for old in saved.iter().rev().skip(KEEP) {
        let _ = std::fs::remove_file(old);
    }
}

/// The cleanup (src/cleanup.rs): self-corrections applied, filler dropped,
/// punctuation, lists, misheard terms fixed. `files` are project paths the speech may
/// name: the cleanup writes them as `@path`, which is how Claude Code takes a file. If
/// it fails, what was said goes out as it was.
async fn clean_up(key: &str, text: String, files: Vec<String>) -> String {
    match Cleaner::new(key).clean(&text, &files).await {
        Ok(c) => {
            log(&format!(
                "cleaned in {} ms ({}): {}",
                c.ms, c.verdict, c.text
            ));
            c.text
        }
        Err(e) => {
            log(&format!("cleanup failed, sending it as said: {e:#}"));
            text
        }
    }
}

/// Learns from a pasted text you fixed before sending: the words you changed that were
/// misheard (not changes of mind, which the cleanup model tells apart) go to your
/// words.txt, so they're recognised and spelled right from then on.
fn learn(pasted: String, sent: String, ui: Ui) {
    let Some(key) = Settings::load().anthropic_key else {
        return;
    };
    tokio::spawn(async move {
        let Some(edited) = words::edited_region(&pasted, &sent) else {
            return;
        };
        let pairs = match words::corrections(&pasted, &edited) {
            Some(pairs) if !pairs.is_empty() => pairs,
            _ => return,
        };
        let misheard = match Cleaner::new(&key)
            .mishearings(&pasted, &edited, &pairs)
            .await
        {
            Ok(m) => m,
            Err(e) => return log(&format!("learning: {e:#}")),
        };
        match words::learn(&misheard) {
            Ok(added) if !added.is_empty() => {
                let words: Vec<String> = added.iter().map(|(h, m)| format!("{h} → {m}")).collect();
                log(&format!("learned {}", words.join(", ")));
                ui.info(&format!("Learned {}", words.join(", ")));
            }
            Ok(_) => {}
            Err(e) => log(&format!("learning: {e}")),
        }
    });
}

/// Whether a reply ends by asking you something: its last paragraph has a question mark.
fn asks(message: &str) -> bool {
    last_paragraph(message).contains('?')
}

fn last_paragraph(message: &str) -> &str {
    message
        .trim()
        .rsplit("\n\n")
        .next()
        .unwrap_or_default()
        .trim()
}

fn first_line(message: &str) -> String {
    let line = message.trim().lines().next().unwrap_or_default();
    match line.char_indices().nth(140) {
        Some((i, _)) => format!("{}…", &line[..i]),
        None => line.to_string(),
    }
}

/// One request per connection: a command line, then (for `hook`) the hook's JSON.
async fn serve(listener: UnixListener, events: mpsc::UnboundedSender<Event>) {
    while let Ok((stream, _)) = listener.accept().await {
        let events = events.clone();
        tokio::spawn(async move {
            let mut buf = Vec::new();
            if stream.take(4 << 20).read_to_end(&mut buf).await.is_err() {
                return;
            }
            let text = String::from_utf8_lossy(&buf);
            let (command, payload) = text.split_once('\n').unwrap_or((&text, ""));
            let event = match command.trim() {
                "start" => Event::Start,
                "stop" => Event::Stop,
                "toggle" => Event::Toggle,
                "cancel" => Event::Cancel,
                "hook" => match serde_json::from_str(payload) {
                    Ok(v) => Event::Hook(v),
                    Err(_) => return,
                },
                _ => return,
            };
            let _ = events.send(event);
        });
    }
}

/// Sends a command to the daemon.
pub async fn send(command: &str, payload: &str) -> Result<()> {
    let mut stream = UnixStream::connect(config::socket_path()).await?;
    stream
        .write_all(format!("{command}\n{payload}").as_bytes())
        .await?;
    stream.shutdown().await?;
    Ok(())
}

pub fn log(message: &str) {
    eprintln!("parley: {message}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_plain_turns_skip_cleanup() {
        assert!(!needs_cleanup("Run the tests.", &[]));
        assert!(!needs_cleanup("What does this function return?", &[]));
        assert!(needs_cleanup(
            "Add a toggle to settings, wait, no, to the header.",
            &[]
        ));
        assert!(needs_cleanup("Um, run the tests.", &[]));
        assert!(needs_cleanup(
            "Open the main file.",
            &["src/main.rs".into()]
        ));
    }

    #[test]
    fn question_at_the_end_asks() {
        assert!(asks(
            "I found two options.\n\nShould I keep the old API or remove it?"
        ));
        assert!(!asks("Does it work? Yes.\n\nDone: the tests pass."));
        assert!(!asks(""));
    }
}
