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
    let hook = hook_entry(&exe.display().to_string());
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
            group["hooks"]
                .as_array()
                .into_iter()
                .flatten()
                .any(is_parley)
        });
        if present {
            continue;
        }
        let mut group = json!({ "hooks": [hook.clone()] });
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
        "\n{}\nPut your key in {}:\nGRADIUM_API_KEY=gd_...",
        key_binding(&exe.display().to_string()),
        config::dir().join("env").display(),
    );
    Ok(())
}

/// The hook Claude Code runs. Elsewhere it's a shell command; on Windows the shell is Git
/// Bash or PowerShell, which quote paths differently, so the program and its argument go
/// in separately and no shell is involved.
fn hook_entry(exe: &str) -> Value {
    if cfg!(windows) {
        json!({ "type": "command", "command": exe, "args": ["hook"], "timeout": 5 })
    } else {
        json!({ "type": "command", "command": format!("'{exe}' hook"), "timeout": 5 })
    }
}

/// A hook `parley setup` added: `'.../parley' hook`, or parley.exe with ["hook"].
fn is_parley(hook: &Value) -> bool {
    let command = hook["command"].as_str().unwrap_or_default();
    command.contains("parley") && (command.ends_with(" hook") || hook["args"] == json!(["hook"]))
}

/// How to bind the push-to-talk key, for this system.
fn key_binding(exe: &str) -> String {
    if cfg!(target_os = "macos") {
        format!(
            "Push-to-talk, with Hammerspoon (https://www.hammerspoon.org), in ~/.hammerspoon/init.lua:\n\
             local parley = \"{exe}\"\n\
             local function run(arg) hs.task.new(parley, nil, {{ arg }}):start() end\n\
             hs.hotkey.bind({{}}, \"F7\", function() run(\"start\") end, function() run(\"stop\") end)\n\
             hs.hotkey.bind({{}}, \"F8\", function() run(\"cancel\") end)\n\
             \nThen give Hammerspoon the Microphone and Accessibility permissions (System \
             Settings → Privacy & Security). If F7 controls media, press fn+F7, or turn on \
             \"Use F1, F2, etc. keys as standard function keys\" in Keyboard settings.\n"
        )
    } else if cfg!(windows) {
        format!(
            "Push-to-talk, with AutoHotkey v2 (https://www.autohotkey.com), in a parley.ahk:\n\
             #Requires AutoHotkey v2.0\n\
             parley := '\"{exe}\"'\n\
             F7:: {{\n\
             \x20   Run(parley ' start', , 'Hide')\n\
             \x20   KeyWait('F7')\n\
             \x20   Run(parley ' stop', , 'Hide')\n\
             }}\n\
             F8::Run(parley ' cancel', , 'Hide')\n"
        )
    } else {
        format!(
            "Push-to-talk, for ~/.config/hypr/bindings.lua:\n\
             o.bind(\"F7\", \"Talk to Claude (push-to-talk)\", \"{exe} start\")\n\
             o.bind(\"F7\", \"Send to Claude (push-to-talk)\", \"{exe} stop\", {{ release = true }})\n\
             o.bind(\"F8\", \"Cancel push-to-talk\", \"{exe} cancel\")\n"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_its_own_hooks() {
        assert!(is_parley(
            &json!({ "command": "'/home/me/.local/bin/parley' hook" })
        ));
        assert!(is_parley(&json!({
            "command": "C:\\Users\\me\\.local\\bin\\parley.exe",
            "args": ["hook"]
        })));
        assert!(!is_parley(&json!({ "command": "parley-notes sync" })));
        assert!(!is_parley(
            &json!({ "command": "C:\\tools\\parley.exe", "args": ["start"] })
        ));
    }
}
