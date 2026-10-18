//! `parley setup`: adds parley's hooks to Claude Code's user settings, next to any
//! hooks already there.

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde_json::{Value, json};

use crate::config;

/// Hook events parley listens to, with their matchers.
const HOOKS: &[(&str, Option<&str>)] = &[
    ("UserPromptSubmit", None),
    ("Stop", None),
    ("Notification", None),
    ("PreToolUse", Some("AskUserQuestion")),
];

pub fn run() -> Result<()> {
    let exe = std::env::current_exe()?;
    let command = format!("'{}' hook", exe.display());
    let path = config::home().join(".claude/settings.json");
    let original = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into());
    let mut settings: Value =
        serde_json::from_str(&original).context("reading ~/.claude/settings.json")?;

    let hooks = settings
        .as_object_mut()
        .context("~/.claude/settings.json isn't a JSON object")?
        .entry("hooks")
        .or_insert_with(|| json!({}));
    let mut added = Vec::new();
    for &(event, matcher) in HOOKS {
        let list = hooks
            .as_object_mut()
            .context("\"hooks\" isn't an object")?
            .entry(event)
            .or_insert_with(|| json!([]));
        let present = list.as_array().into_iter().flatten().any(|group| {
            group["hooks"].as_array().into_iter().flatten().any(|h| {
                h["command"]
                    .as_str()
                    .is_some_and(|c| c.contains("parley") && c.ends_with(" hook"))
            })
        });
        if present {
            continue;
        }
        let mut group =
            json!({ "hooks": [{ "type": "command", "command": command, "timeout": 5 }] });
        if let Some(m) = matcher {
            group["matcher"] = json!(m);
        }
        list.as_array_mut()
            .context("a hook event isn't a list")?
            .push(group);
        added.push(event);
    }

    if added.is_empty() {
        println!("parley's hooks are already in {}", path.display());
    } else {
        if path.exists() {
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
            let backup = path.with_extension(format!("json.bak.parley-{stamp}"));
            std::fs::write(&backup, &original)?;
            println!("backed up {} to {}", path.display(), backup.display());
        }
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, serde_json::to_string_pretty(&settings)? + "\n")?;
        println!("added hooks for {} to {}", added.join(", "), path.display());
    }
    println!(
        "\nPush-to-talk, for ~/.config/hypr/bindings.lua:\n\
         o.bind(\"F7\", \"Talk to Claude (push-to-talk)\", \"{exe} start\")\n\
         o.bind(\"F7\", \"Send to Claude (push-to-talk)\", \"{exe} stop\", {{ release = true }})\n\
         o.bind(\"F8\", \"Cancel push-to-talk\", \"{exe} cancel\")\n\
         \nPut your key in {}:\nGRADIUM_API_KEY=gd_...",
        config::dir().join("env").display(),
        exe = exe.display(),
    );
    Ok(())
}
