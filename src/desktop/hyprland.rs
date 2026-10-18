//! Linux: Hyprland on Wayland (hyprctl, wl-clipboard, wtype).

use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use super::{Key, Window};

impl Window {
    fn from_json(w: &Value) -> Option<Window> {
        let terminal = w["tags"].as_array().into_iter().flatten().any(|t| {
            t.as_str()
                .is_some_and(|t| t.trim_end_matches('*') == "terminal")
        });
        let class = w["class"].as_str().unwrap_or_default().to_lowercase();
        let editor = [
            "cursor", "code", "code-oss", "codium", "vscodium", "windsurf",
        ]
        .contains(&class.as_str());
        Some(Window {
            address: w["address"].as_str()?.to_string(),
            pid: w["pid"].as_u64().and_then(|p| u32::try_from(p).ok()),
            title: w["title"].as_str().unwrap_or_default().to_string(),
            class: w["class"].as_str().unwrap_or_default().to_string(),
            paste_with_shift_insert: terminal || editor,
        })
    }
}

async fn hyprctl_json(what: &str) -> Option<Value> {
    let out = Command::new("hyprctl")
        .args([what, "-j"])
        .output()
        .await
        .ok()?;
    serde_json::from_slice(&out.stdout).ok()
}

pub async fn active_window() -> Option<Window> {
    Window::from_json(&hyprctl_json("activewindow").await?)
}

/// The window at `address`, if it's still open.
pub async fn window(address: &str) -> Option<Window> {
    let clients = hyprctl_json("clients").await?;
    clients
        .as_array()?
        .iter()
        .find(|w| w["address"] == address)
        .and_then(Window::from_json)
}

/// Whether another window belongs to the same process, as with a single-instance
/// terminal (Ghostty, kitty with `--single-instance`): then the process tree can't tell
/// which window runs what.
pub async fn shares_process(window: &Window) -> bool {
    let Some(clients) = hyprctl_json("clients").await else {
        return false;
    };
    clients.as_array().into_iter().flatten().any(|w| {
        w["address"] != window.address.as_str()
            && w["pid"]
                .as_u64()
                .is_some_and(|p| Some(p as u32) == window.pid)
    })
}

pub async fn focus(address: &str) -> Result<()> {
    let out = Command::new("hyprctl")
        // Hyprland's Lua config takes dispatchers as Lua.
        .args([
            "dispatch",
            &format!("hl.dsp.focus({{ window = \"address:{address}\" }})"),
        ])
        .output()
        .await?;
    if !String::from_utf8_lossy(&out.stdout)
        .trim()
        .eq_ignore_ascii_case("ok")
    {
        bail!(
            "couldn't focus the window: {}",
            String::from_utf8_lossy(&out.stdout).trim()
        );
    }
    Ok(())
}

/// Pastes `text` into the focused window, then presses Enter if `submit`. Pasting
/// instead of typing keeps newlines from pressing Enter. The previous clipboard comes
/// back a moment later, once the app has read the new one.
pub async fn paste(text: &str, submit: bool) -> Result<()> {
    let window = active_window().await.context("no focused window")?;
    let saved = save_clipboard().await;

    copy(text.as_bytes(), None).await?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let keys: &[&str] = if window.paste_with_shift_insert {
        &["-M", "shift", "-k", "Insert", "-m", "shift"]
    } else {
        &["-M", "ctrl", "-k", "v", "-m", "ctrl"]
    };
    wtype(keys).await?;
    if submit {
        // Let the app take the paste in before Enter arrives.
        tokio::time::sleep(Duration::from_millis(150)).await;
        wtype(&["-k", "Return"]).await?;
    }

    if let Some((mime, data)) = saved {
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let _ = copy(&data, Some(&mime)).await;
        });
    }
    Ok(())
}

/// Presses `key` in the focused window.
pub async fn press(key: Key) -> Result<()> {
    match key {
        Key::Escape => wtype(&["-k", "Escape"]).await,
        Key::Return => wtype(&["-k", "Return"]).await,
    }
}

/// Types `text` into the focused window.
pub async fn type_text(text: &str) -> Result<()> {
    wtype(&[text]).await
}

/// Presses keys in the focused window (wtype arguments, e.g. `["-k", "Escape"]`), or
/// types text with `["text"]`.
async fn wtype(args: &[&str]) -> Result<()> {
    let status = Command::new("wtype")
        .args(args)
        .status()
        .await
        .context("running wtype")?;
    if !status.success() {
        bail!("wtype couldn't send the keys");
    }
    Ok(())
}

async fn copy(data: &[u8], mime: Option<&str>) -> Result<()> {
    let mut cmd = Command::new("wl-copy");
    if let Some(mime) = mime {
        cmd.args(["--type", mime]);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .spawn()
        .context("running wl-copy")?;
    child.stdin.take().unwrap().write_all(data).await?;
    child.wait().await?;
    Ok(())
}

async fn save_clipboard() -> Option<(String, Vec<u8>)> {
    let types = Command::new("wl-paste")
        .arg("--list-types")
        .output()
        .await
        .ok()?;
    let mime = String::from_utf8_lossy(&types.stdout)
        .lines()
        .next()?
        .to_string();
    let data = Command::new("wl-paste")
        .args(["--no-newline", "--type", &mime])
        .output()
        .await
        .ok()?;
    data.status.success().then_some((mime, data.stdout))
}

pub fn notify(title: &str, body: &str) {
    let (title, body) = (title.to_string(), body.to_string());
    tokio::spawn(async move {
        let _ = Command::new("notify-send")
            .args(["-a", "parley", &title, &body])
            .status()
            .await;
    });
}
