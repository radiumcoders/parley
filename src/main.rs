//! parley: push-to-talk for Claude Code.
//!
//! Hold a key and talk: Gradium transcribes it (translating it into English if you speak
//! another language) and parley pastes it into Claude Code and submits it. Claude Code's
//! hooks tell parley when the agent asks you something: it chimes, notifies you if you're
//! in another window, and opens the microphone so you can just answer.

mod audio;
mod cleanup;
mod commands;
mod config;
mod daemon;
mod desktop;
#[cfg(test)]
mod eval;
mod history;
mod ipc;
mod listen;
mod overlay;
mod project;
mod setup;
mod speak;
mod words;

use std::io::Read;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(about = "Push-to-talk for Claude Code, by Gradium.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the background process (started by `start`/`toggle` when it isn't running)
    Daemon,
    /// Start listening (bind to a key press)
    Start,
    /// Stop listening and send what was said (bind to the key release)
    Stop,
    /// Start or stop listening (for a single key press)
    Toggle,
    /// Stop listening and throw away what was said
    Cancel,
    /// Show what you said lately and what went out
    History {
        /// How many turns
        #[arg(default_value_t = 10)]
        count: usize,
        /// Print only the last text sent, e.g. for `parley history --last | wl-copy`
        #[arg(long)]
        last: bool,
    },
    /// Transcribe a saved turn again with the current settings, to see what went wrong
    Replay {
        /// 1 for the last turn, 2 for the one before, ...
        #[arg(default_value_t = 1)]
        turn: usize,
    },
    /// Forward a Claude Code hook event, read from stdin, to the daemon
    Hook,
    /// Add parley's hooks to ~/.claude/settings.json
    Setup,
    /// The on-screen pill (started by the daemon)
    #[command(hide = true)]
    Overlay,
}

fn main() -> Result<()> {
    tls();
    let cli = Cli::parse();
    let rt = tokio::runtime::Runtime::new()?;
    match cli.command {
        Command::Daemon => rt.block_on(daemon::run()),
        Command::Start => rt.block_on(send_starting_daemon("start")),
        Command::Stop => {
            let _ = rt.block_on(daemon::send("stop", "")); // nothing running: nothing to stop
            Ok(())
        }
        Command::Toggle => rt.block_on(send_starting_daemon("toggle")),
        Command::Cancel => {
            let _ = rt.block_on(daemon::send("cancel", ""));
            Ok(())
        }
        Command::History { count, last } => {
            if last {
                if let Some(text) = history::last() {
                    println!("{text}");
                }
            } else {
                history::show(count);
            }
            Ok(())
        }
        Command::Replay { turn } => rt.block_on(replay(turn)),
        Command::Hook => {
            // Never fail or print: Claude Code shows hook output, and parley may not be running.
            let mut input = String::new();
            if std::io::stdin().read_to_string(&mut input).is_ok() {
                let _ = rt.block_on(daemon::send("hook", &input));
            }
            Ok(())
        }
        Command::Setup => setup::run(),
        Command::Overlay => overlay::run(),
    }
}

/// Gradium's client and the cleanup's both use rustls; installing its crypto provider
/// up front means it never has to pick one (with two in the build it panics instead).
pub fn tls() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

/// Runs a saved turn's audio through Gradium and the cleanup again, printing both.
async fn replay(turn: usize) -> Result<()> {
    let mut saved: Vec<_> = std::fs::read_dir(config::turns_dir())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "wav"))
        .collect();
    saved.sort();
    let path = saved
        .iter()
        .rev()
        .nth(turn.saturating_sub(1))
        .context("no saved turn: they're kept in ~/.local/state/parley/turns")?;
    let wav = std::fs::read(path)?;
    let pcm = wav.get(44..).unwrap_or_default();
    let settings = config::Settings::load();
    let key = settings
        .api_key
        .clone()
        .context("no GRADIUM_API_KEY in ~/.config/parley/env")?;
    println!(
        "{} ({:.1} s)",
        path.display(),
        pcm.len() as f64 / (audio::IN_RATE as f64 * 2.0)
    );
    let client = gradium::Client::new(&key);
    let heard = listen::transcribe(&client, &settings.listen_config(&[]), pcm).await?;
    println!("heard:   {heard}");
    if let Some(key) = settings.anthropic_key.as_deref()
        && settings.cleanup != config::Cleanup::Off
        && !heard.is_empty()
    {
        match cleanup::Cleaner::new(key).clean(&heard, &[]).await {
            Ok(c) => println!("cleaned: {}", c.text),
            Err(e) => println!("cleanup failed: {e:#}"),
        }
    }
    Ok(())
}

async fn send_starting_daemon(command: &str) -> Result<()> {
    if daemon::send(command, "").await.is_ok() {
        return Ok(());
    }
    spawn_daemon()?;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if daemon::send(command, "").await.is_ok() {
            return Ok(());
        }
    }
    anyhow::bail!(
        "parley daemon didn't start; see {}",
        config::log_path().display()
    )
}

fn spawn_daemon() -> Result<()> {
    let log = config::log_path();
    std::fs::create_dir_all(log.parent().unwrap())?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)?;
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command
        .arg("daemon")
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    // Its own process group, so it outlives the key binding that started it.
    #[cfg(unix)]
    command.process_group(0);
    // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP: no console window, outlives its parent.
    #[cfg(windows)]
    command.creation_flags(0x0000_0008 | 0x0000_0200);
    command.spawn().context("starting the parley daemon")?;
    Ok(())
}
