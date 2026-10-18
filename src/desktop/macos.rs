//! macOS: System Events through osascript (the frontmost app, focus, keys, notifications)
//! and pbcopy/pbpaste. System Events needs the Accessibility permission, and asks for
//! Automation the first time, for the app that runs parley.
//!
//! macOS doesn't name a window the way Hyprland and Win32 do without more permissions,
//! so here a "window" is the frontmost app: its process is what parley tells apart.

use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use super::{Key, Window};

/// Runs AppleScript, one `-e` per line, and returns what it printed.
async fn osascript(script: &str) -> Result<String> {
    let mut command = Command::new("osascript");
    for line in script.lines() {
        command.args(["-e", line]);
    }
    let out = command.output().await.context("running osascript")?;
    if !out.status.success() {
        bail!("osascript: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

/// `text` as an AppleScript string literal, on one line.
fn quote(text: &str) -> String {
    let one_line = text.replace(['\r', '\n'], " ");
    format!(
        "\"{}\"",
        one_line.replace('\\', "\\\\").replace('"', "\\\"")
    )
}

/// The process (`which` is an AppleScript filter on application processes) as a window:
/// pid, app name and the title of its front window, one per line.
async fn describe(which: &str) -> Option<Window> {
    let script = format!(
        "tell application \"System Events\"\n\
         set p to first application process whose {which}\n\
         set t to \"\"\n\
         try\n\
         set t to name of front window of p\n\
         end try\n\
         return (unix id of p as text) & linefeed & (name of p) & linefeed & t\n\
         end tell"
    );
    parse_window(&osascript(&script).await.ok()?)
}

fn parse_window(out: &str) -> Option<Window> {
    let mut lines = out.lines();
    let pid: u32 = lines.next()?.trim().parse().ok()?;
    let class = lines.next().unwrap_or_default().trim().to_string();
    let title = lines.collect::<Vec<_>>().join(" ");
    Some(Window {
        address: format!("pid:{pid}"),
        pid: Some(pid),
        title,
        class,
        // Every macOS app, terminals included, pastes with Cmd+V.
        paste_with_shift_insert: false,
    })
}

fn pid_of(address: &str) -> Option<u32> {
    address.strip_prefix("pid:")?.parse().ok()
}

pub async fn active_window() -> Option<Window> {
    describe("frontmost is true").await
}

/// The app at `address`, if it's still running.
pub async fn window(address: &str) -> Option<Window> {
    describe(&format!("unix id is {}", pid_of(address)?)).await
}

/// An app's windows all share its process; the process tree tells what runs in them.
pub async fn shares_process(_window: &Window) -> bool {
    false
}

pub async fn focus(address: &str) -> Result<()> {
    let pid = pid_of(address).context("not a macOS app")?;
    osascript(&format!(
        "tell application \"System Events\" to set frontmost of \
         (first application process whose unix id is {pid}) to true"
    ))
    .await?;
    Ok(())
}

/// Pastes `text` into the frontmost app with Cmd+V, then presses Enter if `submit`.
/// The previous clipboard (its text) comes back a moment later.
pub async fn paste(text: &str, submit: bool) -> Result<()> {
    let saved = pbpaste().await;
    pbcopy(text).await?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    osascript("tell application \"System Events\" to keystroke \"v\" using command down").await?;
    if submit {
        // Let the app take the paste in before Enter arrives.
        tokio::time::sleep(Duration::from_millis(150)).await;
        press(Key::Return).await?;
    }
    if let Some(saved) = saved {
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let _ = pbcopy(&saved).await;
        });
    }
    Ok(())
}

/// Presses `key` in the frontmost app.
pub async fn press(key: Key) -> Result<()> {
    let code = match key {
        Key::Escape => 53,
        Key::Return => 36,
    };
    osascript(&format!(
        "tell application \"System Events\" to key code {code}"
    ))
    .await?;
    Ok(())
}

/// Types `text` into the frontmost app.
pub async fn type_text(text: &str) -> Result<()> {
    osascript(&format!(
        "tell application \"System Events\" to keystroke {}",
        quote(text)
    ))
    .await?;
    Ok(())
}

/// pbcopy and pbpaste read the locale for the text's encoding; one started by a hotkey
/// app may have none.
fn utf8(command: &mut Command) -> &mut Command {
    command.env("LANG", "en_US.UTF-8")
}

async fn pbcopy(text: &str) -> Result<()> {
    let mut child = utf8(&mut Command::new("pbcopy"))
        .stdin(Stdio::piped())
        .spawn()
        .context("running pbcopy")?;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(text.as_bytes())
        .await?;
    child.wait().await?;
    Ok(())
}

/// The clipboard's text, if it holds any.
async fn pbpaste() -> Option<String> {
    let out = utf8(&mut Command::new("pbpaste")).output().await.ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    (out.status.success() && !text.is_empty()).then_some(text)
}

pub fn notify(title: &str, body: &str) {
    let script = format!(
        "display notification {} with title {}",
        quote(body),
        quote(title)
    );
    tokio::spawn(async move {
        let _ = osascript(&script).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmost_app() {
        let w = parse_window("612\nTerminal\n~/code/api — claude").unwrap();
        assert_eq!(w.address, "pid:612");
        assert_eq!(w.pid, Some(612));
        assert_eq!(w.class, "Terminal");
        assert_eq!(w.title, "~/code/api — claude");
        assert_eq!(pid_of(&w.address), Some(612));
        // No front window: no title.
        assert_eq!(parse_window("77\nFinder\n").unwrap().title, "");
        assert!(parse_window("").is_none());
    }

    #[test]
    fn applescript_strings() {
        assert_eq!(quote("/clear"), "\"/clear\"");
        assert_eq!(
            quote("say \"hi\"\nback\\slash"),
            "\"say \\\"hi\\\" back\\\\slash\""
        );
    }
}
