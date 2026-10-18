//! Windows: the foreground window, focus and keys through Win32 (SendInput), the
//! clipboard through arboard, notifications as PowerShell toasts.
//!
//! Windows doesn't let a program type into a window that runs as administrator unless
//! it does too: run parley the same way as the terminal Claude Code is in.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::process::Command;
use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows_sys::Win32::System::Threading::{
    AttachThreadInput, GetCurrentThreadId, OpenProcess, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput,
    VK_CONTROL, VK_ESCAPE, VK_RETURN,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumWindows, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    IsIconic, IsWindow, IsWindowVisible, MSG, PM_NOREMOVE, PeekMessageW, SW_RESTORE,
    SetForegroundWindow, ShowWindow,
};
use windows_sys::core::BOOL;

use super::{Key, Window, toast};

/// CREATE_NO_WINDOW: no console flashes up for the notification's PowerShell.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// A window handle is a pointer: it's carried as text in `Window::address` and never
// held across an await, so the futures that use it stay Send.
fn address(hwnd: HWND) -> String {
    format!("{:x}", hwnd as usize)
}

fn hwnd(address: &str) -> Option<HWND> {
    let n = usize::from_str_radix(address, 16).ok()?;
    (n != 0).then_some(n as HWND)
}

fn describe(hwnd: HWND) -> Option<Window> {
    // SAFETY: plain Win32 queries on a handle checked with IsWindow; the buffers are
    // sized as passed.
    unsafe {
        if hwnd.is_null() || IsWindow(hwnd) == 0 {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let mut title = [0u16; 512];
        let len = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
        let title = String::from_utf16_lossy(&title[..len.max(0) as usize]);
        Some(Window {
            address: address(hwnd),
            pid: (pid != 0).then_some(pid),
            title,
            class: exe_name(pid).unwrap_or_default(),
            // Windows Terminal, the console and VS Code all paste with Ctrl+V.
            paste_with_shift_insert: false,
        })
    }
}

/// The file name of the program `pid` runs ("WindowsTerminal.exe").
fn exe_name(pid: u32) -> Option<String> {
    // SAFETY: the process handle is checked and closed; the buffer is sized as passed.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut path = [0u16; 1024];
        let mut len = path.len() as u32;
        let ok =
            QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut len);
        CloseHandle(process);
        if ok == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&path[..len as usize]);
        path.rsplit('\\').next().map(String::from)
    }
}

pub async fn active_window() -> Option<Window> {
    // SAFETY: no arguments; may return null, which describe() rejects.
    describe(unsafe { GetForegroundWindow() })
}

/// The window at `address`, if it's still open.
pub async fn window(address: &str) -> Option<Window> {
    describe(hwnd(address)?)
}

/// Whether another visible window belongs to the same process, as with Windows
/// Terminal, which runs all its windows in one: then the process tree can't tell which
/// window runs what.
pub async fn shares_process(window: &Window) -> bool {
    let (Some(pid), Some(own)) = (window.pid, hwnd(&window.address)) else {
        return false;
    };
    struct Search {
        pid: u32,
        own: HWND,
        found: bool,
    }
    unsafe extern "system" fn visit(hwnd: HWND, search: LPARAM) -> BOOL {
        // SAFETY: `search` is the &mut Search passed to EnumWindows below, alive for
        // the whole enumeration.
        unsafe {
            let search = &mut *(search as *mut Search);
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if hwnd != search.own && pid == search.pid && IsWindowVisible(hwnd) != 0 {
                search.found = true;
                return 0; // stop
            }
            1
        }
    }
    let mut search = Search {
        pid,
        own,
        found: false,
    };
    // SAFETY: the callback only touches `search`, which outlives the call.
    unsafe {
        EnumWindows(Some(visit), &mut search as *mut Search as LPARAM);
    }
    search.found
}

pub async fn focus(address: &str) -> Result<()> {
    let hwnd = hwnd(address).context("not a window")?;
    if !bring_forward(hwnd) {
        bail!("Windows didn't let parley switch to the window");
    }
    Ok(())
}

fn bring_forward(hwnd: HWND) -> bool {
    // SAFETY: Win32 calls on a window handle and thread ids; the input state is attached
    // only for the switch and detached right after.
    unsafe {
        if IsWindow(hwnd) == 0 {
            return false;
        }
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        if SetForegroundWindow(hwnd) != 0 && GetForegroundWindow() == hwnd {
            return true;
        }
        // Only the program you last used may take the foreground. Sharing the input
        // state of the window in front for a moment gets around that.
        let mut msg: MSG = std::mem::zeroed();
        PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE); // a message queue to attach
        let me = GetCurrentThreadId();
        let front = GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut());
        let attached = front != 0 && front != me && AttachThreadInput(me, front, 1) != 0;
        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
        if attached {
            AttachThreadInput(me, front, 0);
        }
        GetForegroundWindow() == hwnd
    }
}

/// Pastes `text` into the focused window with Ctrl+V, then presses Enter if `submit`.
/// The previous clipboard (its text) comes back a moment later.
pub async fn paste(text: &str, submit: bool) -> Result<()> {
    let saved = arboard::Clipboard::new()
        .and_then(|mut c| c.get_text())
        .ok();
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text))
        .context("setting the clipboard")?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    send(&[
        virtual_key(VK_CONTROL, false),
        virtual_key(VIRTUAL_V, false),
        virtual_key(VIRTUAL_V, true),
        virtual_key(VK_CONTROL, true),
    ])?;
    if submit {
        // Let the app take the paste in before Enter arrives.
        tokio::time::sleep(Duration::from_millis(150)).await;
        press(Key::Return).await?;
    }
    if let Some(saved) = saved {
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let _ = arboard::Clipboard::new().and_then(|mut c| c.set_text(saved));
        });
    }
    Ok(())
}

/// The V key's virtual-key code.
const VIRTUAL_V: u16 = 0x56;

/// Presses `key` in the focused window.
pub async fn press(key: Key) -> Result<()> {
    let vk = match key {
        Key::Escape => VK_ESCAPE,
        Key::Return => VK_RETURN,
    };
    send(&[virtual_key(vk, false), virtual_key(vk, true)])
}

/// Types `text` into the focused window.
pub async fn type_text(text: &str) -> Result<()> {
    let inputs: Vec<INPUT> = text
        .encode_utf16()
        .flat_map(|unit| [unicode(unit, false), unicode(unit, true)])
        .collect();
    send(&inputs)
}

fn virtual_key(vk: u16, up: bool) -> INPUT {
    keyboard(KEYBDINPUT {
        wVk: vk,
        wScan: 0,
        dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
        time: 0,
        dwExtraInfo: 0,
    })
}

fn unicode(unit: u16, up: bool) -> INPUT {
    keyboard(KEYBDINPUT {
        wVk: 0,
        wScan: unit,
        dwFlags: KEYEVENTF_UNICODE | if up { KEYEVENTF_KEYUP } else { 0 },
        time: 0,
        dwExtraInfo: 0,
    })
}

fn keyboard(ki: KEYBDINPUT) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki },
    }
}

fn send(inputs: &[INPUT]) -> Result<()> {
    // SAFETY: a slice of initialized INPUTs, with its length and element size.
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    if sent as usize != inputs.len() {
        bail!("Windows blocked the keys (is the window running as administrator?)");
    }
    Ok(())
}

pub fn notify(title: &str, body: &str) {
    let args = toast::powershell_args(title, body);
    tokio::spawn(async move {
        let _ = Command::new("powershell.exe")
            .args(&args)
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .await;
    });
}
