//! What you said and what went out, newest last, in ~/.local/state/parley/history.jsonl:
//! `parley history` shows it, and a turn that couldn't be pasted isn't lost.

use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::config;
use crate::desktop::Window;

const KEEP: usize = 500;

fn path() -> std::path::PathBuf {
    config::log_path().with_file_name("history.jsonl")
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub fn add(heard: &str, text: &str, window: Option<&Window>, pasted: bool) {
    let entry = json!({
        "at": now(),
        "app": window.map_or("", |w| w.class.as_str()),
        "heard": heard,
        "text": text,
        "pasted": pasted,
    });
    let path = path();
    let mut lines = read();
    if lines.len() >= KEEP * 2 {
        // Now and then, keep only the newest.
        lines.drain(..lines.len() - KEEP);
        let text: String = lines.iter().map(|l| l.to_string() + "\n").collect();
        let _ = std::fs::write(&path, text);
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path);
    if let Ok(mut f) = file {
        let _ = writeln!(f, "{entry}");
    }
}

fn read() -> Vec<Value> {
    std::fs::read_to_string(path())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// Prints the last `count` turns, oldest first.
pub fn show(count: usize) {
    let entries = read();
    if entries.is_empty() {
        println!("Nothing yet: hold the key and talk.");
        return;
    }
    let now = now();
    for e in entries.iter().skip(entries.len().saturating_sub(count)) {
        let ago = now.saturating_sub(e["at"].as_u64().unwrap_or(now));
        let ago = match ago {
            0..60 => format!("{ago}s"),
            60..3600 => format!("{}m", ago / 60),
            3600..86400 => format!("{}h", ago / 3600),
            _ => format!("{}d", ago / 86400),
        };
        let app = e["app"].as_str().filter(|a| !a.is_empty()).unwrap_or("?");
        let failed = if e["pasted"] == false {
            " (not pasted)"
        } else {
            ""
        };
        println!("{ago:>4} ago in {app}{failed}");
        let (heard, text) = (
            e["heard"].as_str().unwrap_or_default(),
            e["text"].as_str().unwrap_or_default(),
        );
        if heard != text {
            println!("  heard: {heard}");
        }
        for line in text.lines() {
            println!("  {line}");
        }
    }
}

/// The text of the last turn.
pub fn last() -> Option<String> {
    read().last()?["text"].as_str().map(String::from)
}
