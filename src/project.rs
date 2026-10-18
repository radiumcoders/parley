//! Which project you're talking about and which of its files you named, so the cleanup
//! can turn "the auth file" into `@src/auth.ts`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

use regex::Regex;

/// Coding agents: their working directory is the project.
const AGENTS: &[&str] = &[
    "claude",
    "codex",
    "opencode",
    "aider",
    "gemini",
    "cursor-agent",
    "amp",
    "crush",
    "goose",
    "qwen",
];
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "__pycache__",
    "venv",
    "dist",
    "build",
    "target",
];
const MAX_FILES: usize = 20_000;
const MAX_CANDIDATES: usize = 12;

/// Words that point at a file; without one, no file list goes to the cleanup.
static FILE_TALK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:files?|folders?|director(?:y|ies)|scripts?|modules?|components?|configs?|read ?me|dot [a-z0-9]{1,5}|attach\w*|refer\w*|mention\w*|tag)\b",
    )
    .unwrap()
});
/// Words around a file's name ("the settings file"), not part of it.
const NAMING: &[&str] = &[
    "the",
    "a",
    "an",
    "file",
    "files",
    "folder",
    "folders",
    "directory",
    "script",
    "module",
    "component",
    "config",
    "to",
    "at",
    "and",
    "in",
    "of",
    "on",
    "my",
    "our",
    "this",
    "that",
    "look",
    "refer",
    "check",
];

/// The directory you're working in, in a terminal window: one named in its title (many
/// shells put it there), else that of a coding agent running in it, else that of the
/// newest program in it.
pub fn terminal_cwd(pid: u32, title: &str) -> Option<PathBuf> {
    static PATH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"~?/[^\s:'"|()]*"#).unwrap());
    let home = crate::config::home();
    for m in PATH.find_iter(title) {
        let s = m.as_str();
        let path = match s.strip_prefix('~') {
            Some(rest) => home.join(rest.trim_start_matches('/')),
            None => PathBuf::from(s),
        };
        if s.len() > 2 && path != home && path.is_dir() {
            return Some(path);
        }
    }

    let (under, names) = processes_under(pid);
    let is_agent = |p: &u32| names.get(p).is_some_and(|n| AGENTS.contains(&n.as_str()));
    let mut under = under;
    under.sort_by_key(|p| std::cmp::Reverse((is_agent(p), *p))); // agents, then newest
    under.iter().take(30).find_map(|&p| process_cwd(p))
}

/// Whether a coding agent (Claude Code, Codex, ...) runs in the window of `pid`.
pub fn agent_in(pid: u32) -> bool {
    let (under, names) = processes_under(pid);
    under
        .iter()
        .any(|p| names.get(p).is_some_and(|n| AGENTS.contains(&n.as_str())))
}

/// The processes descended from `pid`, and every process's name.
fn processes_under(pid: u32) -> (Vec<u32>, HashMap<u32, String>) {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut names: HashMap<u32, String> = HashMap::new();
    for (p, ppid, name) in processes() {
        children.entry(ppid).or_default().push(p);
        names.insert(p, name);
    }
    let mut under = Vec::new();
    let mut todo = children.get(&pid).cloned().unwrap_or_default();
    while let Some(p) = todo.pop() {
        under.push(p);
        todo.extend(children.get(&p).into_iter().flatten());
    }
    (under, names)
}

/// Every process: its id, its parent's, and its name ("claude", as in AGENTS).
#[cfg(target_os = "linux")]
fn processes() -> Vec<(u32, u32, String)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Ok(p) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{p}/stat")) else {
            continue;
        };
        // "pid (comm) state ppid ...": comm may contain spaces, so split at the last ')'.
        let Some((head, rest)) = stat.rsplit_once(')') else {
            continue;
        };
        let comm = head.split_once('(').map_or("", |(_, c)| c).to_string();
        let Some(ppid) = rest.split_whitespace().nth(1).and_then(|s| s.parse().ok()) else {
            continue;
        };
        out.push((p, ppid, comm));
    }
    out
}

#[cfg(target_os = "linux")]
fn process_cwd(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
}

#[cfg(target_os = "macos")]
fn processes() -> Vec<(u32, u32, String)> {
    let Ok(out) = Command::new("ps")
        .args(["-axo", "pid=,ppid=,comm="])
        .output()
    else {
        return Vec::new();
    };
    parse_ps(&String::from_utf8_lossy(&out.stdout))
}

/// `ps -axo pid=,ppid=,comm=` lines; macOS gives comm as the executable's path.
#[cfg(any(target_os = "macos", test))]
fn parse_ps(text: &str) -> Vec<(u32, u32, String)> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.trim_start().splitn(2, char::is_whitespace);
            let pid = parts.next()?.parse().ok()?;
            let rest = parts.next()?.trim_start();
            let (ppid, comm) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            let comm = comm.trim();
            let name = comm.rsplit('/').next().unwrap_or(comm);
            Some((pid, ppid.parse().ok()?, name.to_string()))
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn process_cwd(pid: u32) -> Option<PathBuf> {
    let out = Command::new("lsof")
        .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
        .output()
        .ok()?;
    parse_lsof_cwd(&String::from_utf8_lossy(&out.stdout))
}

/// `lsof -Fn` output: the path is on the line that starts with "n".
#[cfg(any(target_os = "macos", test))]
fn parse_lsof_cwd(text: &str) -> Option<PathBuf> {
    text.lines()
        .find_map(|l| l.strip_prefix('n'))
        .filter(|p| p.starts_with('/'))
        .map(PathBuf::from)
}

#[cfg(target_os = "windows")]
fn processes() -> Vec<(u32, u32, String)> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    let mut out = Vec::new();
    // SAFETY: a snapshot handle, walked with a correctly sized entry, then closed.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return out;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(snapshot, &mut entry) != 0;
        while more {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
            let exe = String::from_utf16_lossy(&entry.szExeFile[..len]);
            out.push((
                entry.th32ProcessID,
                entry.th32ParentProcessID,
                windows_exe_name(&exe),
            ));
            more = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
    }
    out
}

/// "claude.exe" → "claude", to match AGENTS.
#[cfg(any(target_os = "windows", test))]
fn windows_exe_name(exe: &str) -> String {
    let lower = exe.to_lowercase();
    lower.strip_suffix(".exe").unwrap_or(&lower).to_string()
}

/// Windows doesn't expose another process's working directory without reading its
/// memory; the project comes from Claude Code's hooks instead.
#[cfg(target_os = "windows")]
fn process_cwd(_pid: u32) -> Option<PathBuf> {
    None
}

/// The git repository around `cwd`, or `cwd` itself (not home or /).
pub fn project_root(cwd: &Path) -> Option<PathBuf> {
    for d in cwd.ancestors() {
        if d.join(".git").exists() {
            return Some(d.to_path_buf());
        }
    }
    (cwd != crate::config::home() && cwd.parent().is_some()).then(|| cwd.to_path_buf())
}

/// The project's files and folders ("src/", with a slash), relative to `root`. Skips
/// what git ignores.
pub fn project_files(root: &Path) -> Vec<String> {
    let git = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-co", "--exclude-standard", "-z"])
        .output()
        .ok()
        .filter(|o| o.status.success());
    let mut files: Vec<String> = match git {
        Some(out) => String::from_utf8_lossy(&out.stdout)
            .split('\0')
            .filter(|f| !f.is_empty())
            .take(MAX_FILES)
            .map(String::from)
            .collect(),
        None => walk(root),
    };
    let folders: HashSet<String> = files
        .iter()
        .flat_map(|f| {
            let parts: Vec<&str> = f.split('/').collect();
            (1..parts.len()).map(move |i| parts[..i].join("/") + "/")
        })
        .collect();
    let mut folders: Vec<String> = folders.into_iter().collect();
    folders.sort();
    files.extend(folders);
    files
}

fn walk(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    let mut todo = vec![root.to_path_buf()];
    while let Some(dir) = todo.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let path = e.path();
            if path.is_dir() {
                if !name.starts_with('.') && !SKIP_DIRS.contains(&name.as_str()) {
                    todo.push(path);
                }
            } else if let Ok(rel) = path.strip_prefix(root) {
                files.push(rel.to_string_lossy().replace('\\', "/"));
                if files.len() >= MAX_FILES {
                    return files;
                }
            }
        }
    }
    files
}

/// The project paths the speech most likely names, best first; empty when it doesn't
/// sound like it mentions a file.
pub fn file_candidates<'a>(text: &str, files: &'a [String]) -> Vec<String> {
    if files.is_empty() || !FILE_TALK.is_match(text) {
        return Vec::new();
    }
    let compact = |t: &str| {
        t.to_lowercase()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect::<String>()
    };
    let mut index: HashMap<String, HashSet<&'a str>> = HashMap::new();
    for path in files {
        let name = path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(path);
        let stem = if name.starts_with('.') {
            name
        } else {
            name.split('.').next().unwrap_or(name)
        };
        for key in [compact(name), compact(stem), compact(path)] {
            if key.len() >= 3 {
                index.entry(key).or_default().insert(path);
            }
        }
    }

    static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[a-z0-9]+").unwrap());
    let lower = text.to_lowercase();
    let words: Vec<&str> = WORD.find_iter(&lower).map(|m| m.as_str()).collect();
    let mut grams = HashSet::new();
    for n in 1..=4 {
        for ws in words.windows(n) {
            // "settings", "dictate dot py", "test local flow", "read me"
            let (first, last) = (ws[0], ws[n - 1]);
            if !NAMING.contains(&first) && !NAMING.contains(&last) && last != "dot" {
                let gram: String = ws.iter().filter(|w| **w != "dot").copied().collect();
                if gram.len() >= 3 {
                    grams.insert(gram);
                }
            }
        }
    }

    let mut score: HashMap<&str, f64> = HashMap::new();
    let bump = |score: &mut HashMap<&'a str, f64>, path: &'a str, s: f64| {
        let e = score.entry(path).or_insert(0.0);
        *e = e.max(s);
    };
    for g in &grams {
        for path in index.get(g).into_iter().flatten() {
            bump(&mut score, path, 2.0 * g.len() as f64);
        }
    }
    // Near misses ("e to e" for e2e, "settings" for setting).
    if grams.len() * index.len() <= 400_000 {
        for g in grams.iter().filter(|g| g.len() >= 5) {
            let mut close: Vec<(f64, &String)> = index
                .keys()
                .map(|k| (ratio(g, k), k))
                .filter(|(r, _)| *r >= 0.8)
                .collect();
            close.sort_by(|a, b| b.0.total_cmp(&a.0));
            for (r, key) in close.into_iter().take(3) {
                for path in &index[key] {
                    bump(&mut score, path, r * g.len() as f64);
                }
            }
        }
    }
    let mut best: Vec<(&str, f64)> = score.into_iter().collect();
    best.sort_by(|(pa, sa), (pb, sb)| {
        sb.total_cmp(sa)
            .then(pa.matches('/').count().cmp(&pb.matches('/').count()))
            .then(pa.len().cmp(&pb.len()))
            .then(pa.cmp(pb))
    });
    best.into_iter()
        .take(MAX_CANDIDATES)
        .map(|(p, _)| p.to_string())
        .collect()
}

fn ratio(a: &str, b: &str) -> f64 {
    crate::words::ratio(a, b)
}

/// Words from the project's file names ("auth" from src/auth.rs, "user" and "settings"
/// from user-settings.tsx), shallowest files first, to boost in speech recognition.
pub fn file_words(files: &[String]) -> Vec<String> {
    const MAX: usize = 200;
    let mut by_depth: Vec<&String> = files.iter().filter(|f| !f.ends_with('/')).collect();
    by_depth.sort_by_key(|f| f.matches('/').count());
    let mut seen = HashSet::new();
    let mut words = Vec::new();
    for f in by_depth {
        let name = f.rsplit('/').next().unwrap_or(f);
        let stem = name.split('.').next().unwrap_or(name);
        for w in stem.split(|c: char| !c.is_alphanumeric()) {
            if w.len() >= 4 && w.chars().all(char::is_alphabetic) && seen.insert(w.to_lowercase()) {
                words.push(w.to_string());
                if words.len() == MAX {
                    return words;
                }
            }
        }
    }
    words
}

/// The project a window is working in: a known Claude session's directory, else what
/// the terminal shows.
pub fn window_project(
    known_cwd: Option<PathBuf>,
    window: &crate::desktop::Window,
) -> Option<(PathBuf, PathBuf)> {
    let cwd = known_cwd.or_else(|| terminal_cwd(window.pid?, &window.title))?;
    let root = project_root(&cwd)?;
    Some((cwd, root))
}

/// `path` (relative to `root`) as Claude Code in `cwd` refers to it.
pub fn relative_to_cwd(root: &Path, path: &str, cwd: &Path) -> String {
    let full = root.join(path);
    let shown = match full.strip_prefix(cwd) {
        Ok(rel) => rel.to_string_lossy().into_owned(),
        Err(_) => full.to_string_lossy().into_owned(),
    };
    if path.ends_with('/') && !shown.ends_with('/') {
        shown + "/"
    } else {
        shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn names_the_file_spoken() {
        let fs = files(&["src/auth.rs", "src/main.rs", "README.md", "src/"]);
        assert_eq!(
            file_candidates("fix the bug in the auth file", &fs)[0],
            "src/auth.rs"
        );
        assert_eq!(
            file_candidates("update the read me file", &fs)[0],
            "README.md"
        );
    }

    #[test]
    fn nothing_without_file_talk() {
        let fs = files(&["src/auth.rs"]);
        assert!(file_candidates("make the auth faster", &fs).is_empty());
    }

    #[test]
    fn near_misses_match() {
        let fs = files(&["src/settings.rs"]);
        assert_eq!(
            file_candidates("open the setting file", &fs),
            ["src/settings.rs"]
        );
    }

    #[test]
    fn macos_process_list() {
        let ps = "    1     0 /sbin/launchd\n  612     1 /Applications/Utilities/Terminal.app/Contents/MacOS/Terminal\n 4810   612 -zsh\n 4900  4810 /Users/me/.local/bin/claude\n 5000     1 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome\n";
        let list = parse_ps(ps);
        assert_eq!(list[0], (1, 0, "launchd".to_string()));
        assert_eq!(list[2], (4810, 612, "-zsh".to_string()));
        assert_eq!(list[3], (4900, 4810, "claude".to_string()));
        assert_eq!(list[4], (5000, 1, "Google Chrome".to_string()));
    }

    #[test]
    fn macos_working_directory() {
        assert_eq!(
            parse_lsof_cwd("p4900\nfcwd\nn/Users/me/code/api\n"),
            Some(PathBuf::from("/Users/me/code/api"))
        );
        assert_eq!(parse_lsof_cwd(""), None);
    }

    #[test]
    fn windows_process_names() {
        assert_eq!(windows_exe_name("claude.exe"), "claude");
        assert_eq!(windows_exe_name("WindowsTerminal.EXE"), "windowsterminal");
        assert_eq!(windows_exe_name("node"), "node");
    }
}
