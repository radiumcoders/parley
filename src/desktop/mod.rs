//! The desktop side: which window is focused, pasting into it, pressing keys,
//! notifications. Hyprland on Wayland (hyprctl, wl-clipboard, wtype) on Linux; System
//! Events (osascript) on macOS; Win32 on Windows.

#[cfg(target_os = "linux")]
mod hyprland;
// Also built for Linux's tests, which check what it parses.
#[cfg(any(target_os = "macos", test))]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
// Also built for Linux's tests, which check its escaping.
#[cfg(any(target_os = "windows", test))]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
mod toast;

#[cfg(target_os = "linux")]
pub use hyprland::*;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(target_os = "windows")]
pub use windows::*;

#[derive(Clone, Debug)]
pub struct Window {
    /// What identifies the window: Hyprland's address, a Win32 window handle, or (on
    /// macOS) the app's process.
    pub address: String,
    pub pid: Option<u32>,
    pub title: String,
    /// The app, as the desktop names it ("kitty", "firefox").
    pub class: String,
    /// Terminals (Omarchy tags them "terminal") and VS Code-family editors ignore a
    /// synthetic Ctrl+V, so they get Shift+Insert (Linux; elsewhere they take the usual
    /// paste keys).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub paste_with_shift_insert: bool,
}

/// Keys the spoken commands press.
#[derive(Clone, Copy, Debug)]
pub enum Key {
    Escape,
    Return,
}

/// A coding agent's window by its title: Claude Code starts it with its status glyph
/// ("✳ Claude Code", "◐ Fixing the build"); others put their name in it.
pub fn agent_title(title: &str) -> bool {
    let first = title.trim_start().chars().next().unwrap_or(' ');
    let glyph = "✳✻✽✶✢◐◓◑◒".contains(first) || ('\u{2800}'..='\u{28ff}').contains(&first);
    let lower = title.to_lowercase();
    glyph
        || ["claude", "codex", "aider", "opencode", "gemini", "crush"]
            .iter()
            .any(|a| lower.split(|c: char| !c.is_alphanumeric()).any(|w| w == *a))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_titles() {
        assert!(agent_title("✳ Claude Code"));
        assert!(agent_title("◐ Recording and accuracy"));
        assert!(agent_title("⠂ Fixing the build"));
        assert!(agent_title("codex - ~/Projects/app"));
        assert!(!agent_title("~/Projects/parley"));
        assert!(!agent_title("Inbox - Zen Browser"));
        assert!(!agent_title("Claudette's notes"));
    }
}
