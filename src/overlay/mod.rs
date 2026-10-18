//! The pill at the bottom of the screen: the microphone's level and the words so far
//! while you talk, then "Transcribing" and "Cleaning up", then what was sent.
//!
//! It's its own process (`parley overlay`, GTK 4 on a layer-shell surface that never
//! takes keyboard focus or clicks), started by the daemon on first use and fed one JSON
//! message per line on stdin. If it can't start, parley works the same without it.
//! It needs Wayland's layer shell, so it's Linux only: elsewhere `Ui` does nothing.

#[cfg(target_os = "linux")]
mod gtk;

#[cfg(target_os = "linux")]
pub use gtk::run;

/// The pill runs on Linux only.
const SUPPORTED: bool = cfg!(target_os = "linux");

#[cfg(not(target_os = "linux"))]
pub fn run() -> Result<()> {
    anyhow::bail!("the on-screen pill needs Linux (GTK 4 and Wayland's layer shell)")
}

use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Result;
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use crate::config::Settings;
use crate::daemon::log;

/// The daemon's handle on the overlay. Cheap to clone; does nothing when the overlay is
/// off (`PARLEY_OVERLAY=0`) or can't start.
#[derive(Clone)]
pub struct Ui {
    tx: mpsc::UnboundedSender<Value>,
    enabled: Arc<AtomicBool>,
}

impl Ui {
    pub fn new() -> Ui {
        let (tx, mut rx) = mpsc::unbounded_channel::<Value>();
        tokio::spawn(async move {
            let mut child: Option<(tokio::process::Child, tokio::process::ChildStdin)> = None;
            let mut retry_at = tokio::time::Instant::now();
            while let Some(msg) = rx.recv().await {
                if child
                    .as_mut()
                    .is_some_and(|(c, _)| c.try_wait().ok().flatten().is_some())
                {
                    child = None;
                }
                if child.is_none() {
                    // Only a message that shows something is worth starting it for.
                    if msg["s"] == "level"
                        || msg["s"] == "hide"
                        || tokio::time::Instant::now() < retry_at
                    {
                        continue;
                    }
                    match spawn() {
                        Ok(c) => child = Some(c),
                        Err(e) => {
                            log(&format!("overlay: {e:#}"));
                            retry_at = tokio::time::Instant::now() + Duration::from_secs(60);
                            continue;
                        }
                    }
                }
                let (_, stdin) = child.as_mut().unwrap();
                let line = msg.to_string() + "\n";
                if stdin.write_all(line.as_bytes()).await.is_err() {
                    child = None;
                }
            }
        });
        Ui {
            tx,
            enabled: Arc::new(AtomicBool::new(SUPPORTED)),
        }
    }

    fn send(&self, msg: Value) {
        if self.enabled.load(Ordering::Relaxed) {
            let _ = self.tx.send(msg);
        }
    }

    /// The microphone opened: a hold turn, or an answer turn that opened by itself.
    pub fn recording(&self, answer: bool) {
        self.enabled
            .store(SUPPORTED && Settings::load().overlay, Ordering::Relaxed);
        self.send(json!({ "s": "rec", "answer": answer }));
    }

    /// The microphone's loudness, 0..1.
    pub fn level(&self, level: f32) {
        self.send(json!({ "s": "level", "v": level }));
    }

    /// Everything heard so far.
    pub fn heard(&self, text: &str) {
        self.send(json!({ "s": "heard", "t": text }));
    }

    /// A step after the key comes up ("Transcribing", "Cleaning up").
    pub fn working(&self, step: &str) {
        self.send(json!({ "s": "work", "t": step }));
    }

    /// What went out, shown for a moment.
    pub fn done(&self, text: &str) {
        self.send(json!({ "s": "done", "t": text }));
    }

    pub fn error(&self, text: &str) {
        self.send(json!({ "s": "error", "t": text }));
    }

    /// A short note ("Esc", "Learned Svelte"), shown for a moment.
    pub fn info(&self, text: &str) {
        self.send(json!({ "s": "info", "t": text }));
    }

    /// Claude's answer is being read aloud.
    pub fn speaking(&self) {
        self.enabled
            .store(SUPPORTED && Settings::load().overlay, Ordering::Relaxed);
        self.send(json!({ "s": "speak" }));
    }

    pub fn hide(&self) {
        self.send(json!({ "s": "hide" }));
    }
}

fn spawn() -> Result<(tokio::process::Child, tokio::process::ChildStdin)> {
    let mut child = tokio::process::Command::new(std::env::current_exe()?)
        .arg("overlay")
        // A layer surface needs Wayland; under X11 it would be an ordinary window.
        .env("GDK_BACKEND", "wayland")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let stdin = child.stdin.take().unwrap();
    Ok((child, stdin))
}
