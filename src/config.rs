//! Settings, read from ~/.config/parley/env on every turn so edits apply without a restart.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::listen;

/// The languages Gradium understands: code, name, and a flagship voice that speaks it.
pub const LANGUAGES: &[(&str, &str, &str)] = &[
    ("en", "English", "NbpkqMVS3CJeq2j8"),    // Zoey
    ("fr", "French", "YhIHaAfQ0cQPDV9R"),     // Solène
    ("de", "German", "aBNlTApBeOlVKa23"),     // Lorena
    ("es", "Spanish", "iTQW2xFICXk8riV4"),    // Vera
    ("pt", "Portuguese", "6k6cRt7QJfm5ChrH"), // Rafaela
];

pub struct Settings {
    pub api_key: Option<String>,
    /// For the cleanup (Claude Haiku).
    pub anthropic_key: Option<String>,
    /// The language you speak.
    pub lang: String,
    /// Speech in another language reaches the agent in English.
    pub translate: bool,
    /// When to press Enter after pasting.
    pub submit: Submit,
    /// When to clean up what was said before pasting it.
    pub cleanup: Cleanup,
    /// How long the microphone stays open after a turn, so the next one can start with
    /// the moment before its key press.
    pub warm_mic: Duration,
    /// Show the pill at the bottom of the screen.
    pub overlay: bool,
    /// The PipeWire input to record from (`pw-record --target`); the default if `None`.
    pub mic: Option<String>,
    /// Log what would be typed instead of typing it.
    pub dry_run: bool,
    /// Gradium's lookahead, in 80 ms frames: more is more accurate and slower to finish.
    pub stt_delay: u32,
    /// Open the microphone by itself when the agent ends its turn with a question.
    pub auto_answer: bool,
    /// Gradium voice for answers you ask to hear.
    pub voice: String,
}

impl Settings {
    pub fn load() -> Self {
        let file = read_env_file();
        let get = |key: &str| file.get(key).cloned().or_else(|| std::env::var(key).ok());
        let flag = |key: &str, default: bool| {
            get(key).map_or(default, |v| {
                !matches!(v.to_lowercase().as_str(), "0" | "false" | "no" | "off")
            })
        };
        let lang = get("PARLEY_LANG")
            .filter(|l| LANGUAGES.iter().any(|(code, ..)| code == l))
            .unwrap_or("en".into());
        let voice = get("PARLEY_VOICE")
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| language(&lang).2.into());
        Settings {
            api_key: get("GRADIUM_API_KEY").filter(|k| !k.is_empty()),
            anthropic_key: get("ANTHROPIC_API_KEY").filter(|k| !k.is_empty()),
            lang,
            translate: flag("PARLEY_TRANSLATE", true),
            submit: match get("PARLEY_SUBMIT")
                .unwrap_or_default()
                .to_lowercase()
                .as_str()
            {
                "always" => Submit::Always,
                "0" | "false" | "no" | "off" | "never" => Submit::Never,
                _ => Submit::Agents,
            },
            cleanup: match get("PARLEY_CLEANUP")
                .unwrap_or_default()
                .to_lowercase()
                .as_str()
            {
                "auto" => Cleanup::Auto,
                "0" | "false" | "no" | "off" | "never" => Cleanup::Off,
                _ => Cleanup::Always,
            },
            warm_mic: match get("PARLEY_WARM_MIC")
                .unwrap_or_default()
                .to_lowercase()
                .as_str()
            {
                "always" | "on" => Duration::MAX,
                "" => Duration::from_secs(300),
                v => Duration::from_secs(v.parse().unwrap_or(0)),
            },
            overlay: flag("PARLEY_OVERLAY", true),
            mic: get("PARLEY_MIC").filter(|m| !m.is_empty()),
            dry_run: flag("PARLEY_DRY_RUN", false),
            stt_delay: get("PARLEY_STT_DELAY")
                .and_then(|d| d.parse().ok())
                .filter(|d| *d <= 80)
                .unwrap_or(16),
            auto_answer: flag("PARLEY_AUTO_ANSWER", true),
            voice,
        }
    }

    pub fn language_name(&self) -> &'static str {
        language(&self.lang).1
    }

    /// `project_words` are boosted after parley's vocabulary: the project's file names.
    pub fn listen_config(&self, project_words: &[String]) -> listen::Config {
        let translate = self.translate && self.lang != "en";
        listen::Config {
            // A translating model writes its transcript in `language`, whatever was spoken.
            model: if translate {
                "stt-translate"
            } else {
                "default"
            }
            .into(),
            json_config: {
                let mut c = serde_json::json!({
                    "language": if translate { "en" } else { self.lang.as_str() },
                    // 10 (Gradium's default) had 9.5% of words wrong in src/eval.rs, 16
                    // and 24 7.8%; 16 finishes ~200 ms later than 10, 24 ~500 ms.
                    "delay_in_frames": self.stt_delay,
                });
                if translate {
                    c["target_language"] = "en".into();
                }
                let words = keywords(project_words);
                if !words.is_empty() {
                    c["keywords"] = serde_json::json!({ "words": words, "boost": 3 });
                }
                c
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Submit {
    Never,
    /// In windows running a coding agent (Claude Code, Codex, ...), where a prompt is
    /// meant to be sent. Elsewhere parley only types, so a text box or document isn't
    /// sent before you've read it.
    Agents,
    Always,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cleanup {
    Off,
    /// Skip short plain turns: faster, but misheard words in them stay.
    Auto,
    Always,
}

fn language(code: &str) -> &'static (&'static str, &'static str, &'static str) {
    LANGUAGES
        .iter()
        .find(|(c, ..)| *c == code)
        .unwrap_or(&LANGUAGES[0])
}

/// parley's vocabulary (its programming terms and your words.txt), then `extra`, for
/// Gradium's keyword boosting: names come out right before the cleanup sees them.
/// Gradium matches single tokens, so "Claude Code" boosts "Claude" and "Code".
fn keywords(extra: &[String]) -> Vec<String> {
    const MAX: usize = 500; // Gradium's limit
    let terms = crate::words::terms();
    let mut seen = std::collections::HashSet::new();
    terms
        .iter()
        .flat_map(|term| {
            let built_in = crate::words::is_built_in(term);
            term.split_whitespace().map(move |w| (w, built_in))
        })
        .chain(extra.iter().map(|w| (w.as_str(), true)))
        .map(|(w, known)| {
            (
                w.trim_matches(|c: char| !c.is_alphanumeric()).to_string(),
                known,
            )
        })
        // Not short everyday words ("you: often heard as U"): boosting those skews
        // ordinary speech. Short names are fine when they look like names (CSS, Svelte),
        // are programming terms (enum) or come from a file name (auth).
        .filter(|(w, known)| {
            let len = w.chars().count();
            let named = *known || w.chars().any(|c| !c.is_lowercase());
            len >= 5 || (len >= 3 && named)
        })
        .map(|(w, _)| w)
        .filter(|w| seen.insert(w.clone()))
        .take(MAX)
        .collect()
}

pub fn dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"))
        .join("parley")
}

pub fn home() -> PathBuf {
    // Windows sets USERPROFILE; HOME there depends on which shell started parley.
    #[cfg(windows)]
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
    #[cfg(not(windows))]
    let home = std::env::var_os("HOME");
    PathBuf::from(home.unwrap_or_default())
}

#[cfg(unix)]
pub fn socket_path() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(dir) => PathBuf::from(dir).join("parley.sock"),
        None => std::env::temp_dir().join(format!(
            "parley-{}.sock",
            std::env::var("USER").unwrap_or_default()
        )),
    }
}

pub fn log_path() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/state"))
        .join("parley/daemon.log")
}

/// The audio of the last few turns, to replay when a transcript comes out wrong.
pub fn turns_dir() -> PathBuf {
    log_path().with_file_name("turns")
}

/// KEY=value lines; `#` starts a comment line.
fn read_env_file() -> HashMap<String, String> {
    let Ok(text) = std::fs::read_to_string(dir().join("env")) else {
        return HashMap::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| {
            (
                k.trim().to_string(),
                v.trim().trim_matches(['"', '\'']).to_string(),
            )
        })
        .collect()
}
