//! The words parley should get right: programming terms it ships with, your own in
//! ~/.config/parley/words.txt, and what it learns from the fixes you make to a paste.
//!
//! One entry per line: `Term`, or `Term: note, often heard as "x", "y"`. The terms are
//! boosted in Gradium's recognition; the cleanup gets the whole entries, and a term
//! still heard as one of its "heard as" forms gets repaired.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::path::PathBuf;
use std::sync::LazyLock;

use regex::Regex;

use crate::config;

/// Hints only where the misheard form never means anything else in a prompt; a term
/// that can ("struck", "swell") is left to the cleanup's judgement of the context.
const BUILT_IN: &str = r#"
Claude: the AI assistant, often heard as "Claw" or "clawed"
Claude Code
Gradium
GitHub
pull request: often heard as "pool request"
useEffect: often heard as "use effect"
useState: often heard as "use state"
Postgres: often heard as "post gress"
Vercel
Next.js
Svelte
SvelteKit
TypeScript
JavaScript
Tailwind
Vite
React
Rust
struct
enum
async
regex
README
OAuth
auth
daemon
cargo clippy
rustfmt
tokio
serde
navbar
WebSocket
stdin
stdout
stderr
mutex
linter
nginx
SQLite
GraphQL
kubectl
JSON
YAML
TOML
repo
refactor
endpoint
middleware
webhook
localhost
npm
pnpm
env
diff
rebase
Hyprland
Wayland
"#;

/// A term and the ways speech recognition mishears it.
pub type Hint = (String, Vec<String>);

static QUOTED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""([^"]+)""#).unwrap());
const PUNCT: &[char] = &['.', ',', ';', ':', '!', '?', '"', '\'', '(', ')', '[', ']'];

/// Everyday words: swapping one for another ("there" -> "their") is grammar, not a
/// name or term to remember.
const COMMON: &str = "a about all also am an and any are as at be been being big but by can could day did do does done down eight even few first five for four from get go going good got great had has have he her here hers him his how i if in into is it its just know last left less like made make many may me might mine more most much must my need new next nine no not now of old on one only or our ours out over really right said say see seven shall she should six small so some take ten than that the their theirs them then there these they think this those three time to too two up us very want was way we well were what when where which who whom whose why will with would year yes you your yours";

pub fn path() -> PathBuf {
    config::dir().join("words.txt")
}

fn entries(text: &str) -> impl Iterator<Item = String> + '_ {
    text.lines()
        .map(str::trim)
        .filter(|w| !w.is_empty() && !w.starts_with('#'))
        .map(String::from)
}

/// Every entry, the built-in ones first.
pub fn vocabulary() -> Vec<String> {
    let mut words: Vec<String> = entries(BUILT_IN).collect();
    if let Ok(mine) = std::fs::read_to_string(path()) {
        words.extend(entries(&mine));
    }
    words
}

/// The terms alone ("Claude Code", "useEffect").
pub fn terms() -> Vec<String> {
    vocabulary()
        .iter()
        .map(|w| w.split(':').next().unwrap_or_default().trim().to_string())
        .collect()
}

pub fn is_built_in(term: &str) -> bool {
    entries(BUILT_IN).any(|e| e.split(':').next().unwrap_or_default().trim() == term)
}

/// (term, [ways it gets misheard]) from entries like 'Postgres: often heard as "post gress"'.
pub fn hints() -> Vec<Hint> {
    vocabulary()
        .iter()
        .filter_map(|w| {
            let (term, hint) = w.split_once(':')?;
            let (_, heard) = hint.split_once("heard as")?;
            let heard = QUOTED
                .captures_iter(heard)
                .map(|c| c[1].to_string())
                .collect();
            Some((term.trim().to_string(), heard))
        })
        .collect()
}

/// Adds corrections to your words.txt as 'Svelte: often heard as "Swell"' (just
/// 'GitHub' when only the spelling changed). Returns the pairs that were new.
pub fn learn(pairs: &[(String, String)]) -> std::io::Result<Vec<(String, String)>> {
    let path = path();
    let mut lines: Vec<String> = match std::fs::read_to_string(&path) {
        Ok(text) => text.lines().map(String::from).collect(),
        Err(_) => vec![
            "# Names and terms parley should get right, one per line, optionally".into(),
            "# 'Term: often heard as \"x\"'. parley adds the words you fix.".into(),
        ],
    };
    let mut known: HashSet<(String, String)> = hints()
        .into_iter()
        .flat_map(|(term, heard)| {
            heard
                .into_iter()
                .map(move |h| (term.to_lowercase(), h.to_lowercase()))
        })
        .collect();
    let mut terms: HashSet<String> = terms().iter().map(|t| t.to_lowercase()).collect();
    let mut added = vec![];
    for (heard, meant) in pairs {
        let (h, m) = (heard.to_lowercase(), meant.to_lowercase());
        if h == m {
            if terms.insert(m) {
                lines.push(meant.clone());
                added.push((heard.clone(), meant.clone()));
            }
            continue;
        }
        if known.contains(&(m.clone(), h.clone())) {
            continue;
        }
        let existing = lines.iter().position(|line| {
            !line.starts_with('#')
                && line
                    .split(':')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_lowercase()
                    == m
        });
        match existing {
            Some(i) => {
                let line = &lines[i];
                let (term, hint) = line.split_once(':').unwrap_or((line, ""));
                lines[i] = if hint.contains("heard as") {
                    format!("{line}, \"{heard}\"")
                } else if hint.trim().is_empty() {
                    format!("{}: often heard as \"{heard}\"", term.trim())
                } else {
                    format!("{line}, often heard as \"{heard}\"")
                };
            }
            None => lines.push(format!("{meant}: often heard as \"{heard}\"")),
        }
        known.insert((m.clone(), h));
        terms.insert(m);
        added.push((heard.clone(), meant.clone()));
    }
    if !added.is_empty() {
        std::fs::create_dir_all(config::dir())?;
        std::fs::write(&path, lines.join("\n") + "\n")?;
    }
    Ok(added)
}

fn is_common(w: &str) -> bool {
    COMMON.split(' ').any(|c| c == w.to_lowercase())
}

/// What you fixed in a pasted text: [(heard, meant)] for each short replacement (up to
/// 3 words a side), e.g. [("Swell", "Svelte")]. None when `after` isn't an edited
/// version of `before`.
pub fn corrections(before: &str, after: &str) -> Option<Vec<(String, String)>> {
    let tokens = |t: &str| -> Vec<String> {
        t.split_whitespace()
            .map(|w| w.trim_matches(PUNCT))
            .filter(|w| !w.is_empty())
            .map(String::from)
            .collect()
    };
    let (a, b) = (tokens(before), tokens(after));
    if a.is_empty() || b.is_empty() {
        return None;
    }
    let sm = SequenceMatcher::new(&a, &b);
    let same: usize = sm.matching_blocks().iter().map(|m| m.size).sum();
    if same < 2.max(b.len() / 2) {
        return None; // a different text, or rewritten rather than corrected
    }
    let mut pairs = vec![];
    for op in sm.opcodes() {
        let (heard, meant) = (a[op.i1..op.i2].join(" "), b[op.j1..op.j2].join(" "));
        if op.tag != Tag::Replace || op.i2 - op.i1 > 3 || op.j2 - op.j1 > 3 || heard == meant {
            continue;
        }
        if a[op.i1..op.i2]
            .iter()
            .chain(&b[op.j1..op.j2])
            .all(|w| is_common(w))
        {
            continue;
        }
        pairs.push((heard, meant.replace(':', "")));
    }
    Some(pairs)
}

/// The part of `text` (a whole message) that is the pasted text, possibly edited;
/// None if it isn't there.
pub fn edited_region(pasted: &str, text: &str) -> Option<String> {
    let a: Vec<&str> = pasted.split_whitespace().collect();
    let words: Vec<&str> = text.split_whitespace().collect();
    if a.is_empty() || words.is_empty() {
        return None;
    }
    let key = |w: &&str| w.trim_matches(PUNCT).to_lowercase();
    let ka: Vec<String> = a.iter().map(key).collect();
    let kw: Vec<String> = words.iter().map(key).collect();
    let sm = SequenceMatcher::new(&ka, &kw);
    let blocks: Vec<_> = sm
        .matching_blocks()
        .into_iter()
        .filter(|m| m.size > 0)
        .collect();
    if blocks.iter().map(|m| m.size).sum::<usize>() < 2.max(a.len() / 2) {
        return None;
    }
    let (first, last) = (blocks[0], blocks[blocks.len() - 1]);
    // Widen to the words around the matches that stand for unmatched pasted words at
    // either end, without crossing into another sentence.
    let stop = |w: &str| w.ends_with(['.', '!', '?']);
    let (mut start, mut need) = (first.b, first.a);
    while need > 0 && start > 0 && !stop(words[start - 1]) {
        (start, need) = (start - 1, need - 1);
    }
    let (mut end, mut need) = (last.b + last.size, a.len() - last.a - last.size);
    while need > 0 && end < words.len() {
        (end, need) = (end + 1, need - 1);
        if stop(a[a.len() - 1]) && stop(words[end - 1]) {
            break;
        }
    }
    Some(words[start..end].join(" "))
}

/// How alike two strings are, 0..1: Python's `SequenceMatcher(None, a, b).ratio()`.
pub fn ratio(a: &str, b: &str) -> f64 {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let matched: usize = SequenceMatcher::new(&a, &b)
        .matching_blocks()
        .iter()
        .map(|m| m.size)
        .sum();
    2.0 * matched as f64 / (a.len() + b.len()).max(1) as f64
}

// Python's difflib.SequenceMatcher (no junk heuristics), for finding what changed.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Match {
    a: usize,
    b: usize,
    size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tag {
    Equal,
    Replace,
    Delete,
    Insert,
}

#[derive(Debug, Clone, Copy)]
struct Opcode {
    tag: Tag,
    i1: usize,
    i2: usize,
    j1: usize,
    j2: usize,
}

struct SequenceMatcher<'a, T> {
    a: &'a [T],
    b: &'a [T],
    b2j: HashMap<&'a T, Vec<usize>>,
}

impl<'a, T: Eq + Hash> SequenceMatcher<'a, T> {
    fn new(a: &'a [T], b: &'a [T]) -> Self {
        let mut b2j: HashMap<&T, Vec<usize>> = HashMap::new();
        for (j, x) in b.iter().enumerate() {
            b2j.entry(x).or_default().push(j);
        }
        SequenceMatcher { a, b, b2j }
    }

    /// The longest matching block in a[alo..ahi] and b[blo..bhi]; of equally long
    /// ones, the one starting earliest in a, then earliest in b.
    fn find_longest_match(&self, alo: usize, ahi: usize, blo: usize, bhi: usize) -> Match {
        let (mut besti, mut bestj, mut bestsize) = (alo, blo, 0);
        let mut j2len: HashMap<usize, usize> = HashMap::new();
        for i in alo..ahi {
            let mut newj2len = HashMap::new();
            for &j in self
                .b2j
                .get(&self.a[i])
                .map(Vec::as_slice)
                .unwrap_or_default()
            {
                if j < blo {
                    continue;
                }
                if j >= bhi {
                    break;
                }
                let k = j
                    .checked_sub(1)
                    .and_then(|p| j2len.get(&p))
                    .copied()
                    .unwrap_or(0)
                    + 1;
                newj2len.insert(j, k);
                if k > bestsize {
                    (besti, bestj, bestsize) = (i + 1 - k, j + 1 - k, k);
                }
            }
            j2len = newj2len;
        }
        Match {
            a: besti,
            b: bestj,
            size: bestsize,
        }
    }

    /// Matching blocks in order, adjacent ones merged, ending with (len a, len b, 0).
    fn matching_blocks(&self) -> Vec<Match> {
        let (la, lb) = (self.a.len(), self.b.len());
        let mut queue = vec![(0, la, 0, lb)];
        let mut blocks = Vec::new();
        while let Some((alo, ahi, blo, bhi)) = queue.pop() {
            let m = self.find_longest_match(alo, ahi, blo, bhi);
            if m.size > 0 {
                blocks.push(m);
                if alo < m.a && blo < m.b {
                    queue.push((alo, m.a, blo, m.b));
                }
                if m.a + m.size < ahi && m.b + m.size < bhi {
                    queue.push((m.a + m.size, ahi, m.b + m.size, bhi));
                }
            }
        }
        blocks.sort_by_key(|m| (m.a, m.b, m.size));
        let mut merged: Vec<Match> = Vec::new();
        for m in blocks {
            match merged.last_mut() {
                Some(last) if last.a + last.size == m.a && last.b + last.size == m.b => {
                    last.size += m.size
                }
                _ => merged.push(m),
            }
        }
        merged.push(Match {
            a: la,
            b: lb,
            size: 0,
        });
        merged
    }

    fn opcodes(&self) -> Vec<Opcode> {
        let (mut i, mut j) = (0, 0);
        let mut ops = Vec::new();
        for m in self.matching_blocks() {
            let tag = match (i < m.a, j < m.b) {
                (true, true) => Some(Tag::Replace),
                (true, false) => Some(Tag::Delete),
                (false, true) => Some(Tag::Insert),
                (false, false) => None,
            };
            if let Some(tag) = tag {
                ops.push(Opcode {
                    tag,
                    i1: i,
                    i2: m.a,
                    j1: j,
                    j2: m.b,
                });
            }
            (i, j) = (m.a + m.size, m.b + m.size);
            if m.size > 0 {
                ops.push(Opcode {
                    tag: Tag::Equal,
                    i1: m.a,
                    i2: i,
                    j1: m.b,
                    j2: j,
                });
            }
        }
        ops
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_fixed_words() {
        assert_eq!(
            corrections(
                "Move the Swell store into its own file.",
                "Move the Svelte store into its own file."
            ),
            Some(vec![("Swell".into(), "Svelte".into())])
        );
        // Grammar between everyday words isn't a term to learn.
        assert_eq!(
            corrections("Put it over their.", "Put it over there."),
            Some(vec![])
        );
        assert_eq!(
            corrections("Run the tests.", "Something else entirely."),
            None
        );
    }

    #[test]
    fn finds_the_paste_in_a_longer_message() {
        assert_eq!(
            edited_region(
                "Rename the daemon struck to server.",
                "Rename the daemon struct to server. Then run the tests."
            )
            .as_deref(),
            Some("Rename the daemon struct to server.")
        );
    }

    #[test]
    fn hints_from_entries() {
        let hints = hints();
        assert!(hints.contains(&("Postgres".into(), vec!["post gress".into()])));
    }
}
