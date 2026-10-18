# parley

Push-to-talk for Claude Code. Hold a key, talk, let go: your words are cleaned up
("wait, no", "um" and misheard names fixed) and pasted into
Claude Code and sent. Speak English, French, German, Spanish or Portuguese; Claude gets
English either way. In any other app it's plain dictation: it types, and never sends.

- **See it working**: a pill at the bottom of the screen shows the microphone's level and
  your words as you say them, then "Transcribing" and "Cleaning up", then what went out.
- **Files by name**: "fix the bug in the auth file" reaches Claude as
  "Fix the bug in @src/auth.ts".
- **Commands**: say just "stop" (Esc), "undo that" (Esc Esc, Claude Code's rewind),
  "clear the chat" (`/clear`) or "compact" (`/compact`).
- **Answers out loud**: say "answer me", "answer out loud", "tell me by voice" or "talk me
  through it", and Claude's reply is read to you. Pressing the key while it talks stops it.
- **Asking back**: when Claude ends its turn with a question, parley chimes and opens the
  microphone by itself, so you just answer. When it needs a permission or finishes, it
  chimes, and if you're in another window it sends a notification.
- **Learns your words**: fix a word parley got wrong before you send the prompt, and it
  remembers the right spelling (in `~/.config/parley/words.txt`).
- **Nothing lost**: `parley history` shows what you said and what went out;
  `parley replay` runs a saved turn through again to see what went wrong.

```
F7 held ─▶ mic (+400 ms from before the press) ─▶ Gradium STT ─▶ pill shows the words
                                  │
        coding agent + command? ── yes ──▶ keys (Esc, /clear, /compact)
                                  │ no
               cleanup (Claude Haiku), file names → @paths
                                  │
           paste (+ Enter in a coding agent; just paste anywhere else)
                                  │
Claude Code hooks ─▶ chime / notification / mic opens / answer read aloud (Gradium TTS)
```

## Setup

Needs a [Gradium](https://gradium.ai) API key, Hyprland on Wayland, PipeWire, `wtype`,
`wl-clipboard`, `notify-send`, and GTK 4 with `gtk4-layer-shell` for the pill.

```sh
cargo install --path . --root ~/.local
parley setup     # adds parley's hooks to ~/.claude/settings.json (backs it up first)
```

Put your keys in `~/.config/parley/env` (`GRADIUM_API_KEY=gd_...`, and
`ANTHROPIC_API_KEY=sk-ant-...` for the cleanup), and bind keys in
`~/.config/hypr/bindings.lua`:

```lua
o.bind("F7", "Talk to Claude (push-to-talk)", "~/.local/bin/parley start")
o.bind("F7", "Send to Claude (push-to-talk)", "~/.local/bin/parley stop", { release = true })
o.bind("F8", "Cancel push-to-talk", "~/.local/bin/parley cancel")  -- press while holding F7
```

The first press starts the background process. Its log is
`~/.local/state/parley/daemon.log`.

## Commands

| | |
| --- | --- |
| `parley start` / `stop` | Start listening; stop and send (bind to a key's press and release) |
| `parley toggle` | Start, or stop and send (for a single key press) |
| `parley cancel` | Stop and throw it away; or stop an answer being read aloud |
| `parley history [N]` | The last N turns: what was heard and what went out (`--last` prints just the last text) |
| `parley replay [N]` | Transcribe the Nth-last turn's audio again with the current settings and print it (costs Gradium credits) |
| `parley setup` | Add the Claude Code hooks |

## Settings

In `~/.config/parley/env`, read on every turn:

| Setting | Default | |
| --- | --- | --- |
| `GRADIUM_API_KEY` | | Required: all speech goes through Gradium |
| `ANTHROPIC_API_KEY` | | For the cleanup; without it transcripts go out as heard |
| `PARLEY_LANG` | `en` | The language you speak: `en`, `fr`, `de`, `es`, `pt` |
| `PARLEY_TRANSLATE` | `1` | Speech in another language reaches Claude in English |
| `PARLEY_SUBMIT` | `1` | Press Enter after pasting into a coding agent; `always` everywhere; `0` never |
| `PARLEY_CLEANUP` | `1` | Clean up every turn; `auto` skips short plain turns (faster, less accurate); `0` never |
| `PARLEY_WARM_MIC` | `300` | Seconds the microphone stays open after a turn, so the next one starts with what you said just before the press (`always`, or `0` to close it after each turn) |
| `PARLEY_STT_DELAY` | `16` | Gradium's lookahead in 80 ms frames: higher is more accurate and finishes later |
| `PARLEY_MIC` | default input | A PipeWire input to record from (name or serial, as in `pw-record --target`) |
| `PARLEY_OVERLAY` | `1` | Show the pill at the bottom of the screen |
| `PARLEY_AUTO_ANSWER` | `1` | Open the microphone when Claude ends its turn with a question |
| `PARLEY_VOICE` | per language | Gradium voice id for answers read aloud |
| `PARLEY_DRY_RUN` | `0` | Log what would be typed instead of typing it |

## Accuracy

`cargo test eval -- --ignored --nocapture` (`src/eval.rs`) measures it: twelve coding
prompts spoken by Gradium TTS in an English voice and, for an accent, French and German
ones, through STT as a quiet, noisy desk microphone hears them (speech at -33 dBFS over
a -44 dBFS noise floor, the levels of a real saved turn). Word error rates:

| | Words wrong | First word wrong |
| --- | --- | --- |
| Lookahead 10 frames (before) | 9.5% | |
| Lookahead 16 (now) or 24 | 7.8% | 2 of 36 |
| First 120 ms of speech lost (speaking as you press, cold microphone) | 16.5% | 27 of 36 |
| ...with 300 ms of silence sent first | 17.3% | 25 of 36 |
| Lookahead 40 | 9.2% | |
| Automatic gain on the quiet microphone | worse (+0.7) | |

What parley does about it:

- **The moment before the press**: pw-record takes ~110 ms to start, and the key binding
  a little more, so a word said as the key goes down lost its start, and the model
  guessed ("Refer to the main file" came out as "He referred to..." and "prefer to...").
  After a turn the microphone stays open for `PARLEY_WARM_MIC` (5 minutes), keeping only
  the last 400 ms in memory, and the next turn starts with them. Nothing is sent or saved
  until you press the key. The first press after it closed still starts cold.
- **The end of the turn**: 400 ms of audio after the release (it was 200, and saved turns
  showed speech still going when it cut).
- **Lookahead 16** instead of 10: about 200 ms more after the release.
- **Coding words boosted**: struct, enum, regex, README, OAuth, Svelte, useEffect,
  WebSocket, clippy... (`src/words.rs`), with your `words.txt` and the project's file
  names.
- **Cleanup on every turn**: it turns "struck" into "struct", "Oath" into "OAuth",
  "pool request" into "pull request", "use effect" into "useEffect", "cargo creepy" into
  "cargo clippy". Short turns used to skip it; `PARLEY_CLEANUP=auto` still does.
- **Learning**: a word you fix in a pasted prompt before sending it goes to your
  `words.txt` (only if the cleanup model judges it a misheard word rather than a change
  of mind), and from then on is boosted in STT and preferred by the cleanup. To fix
  before sending, set `PARLEY_SUBMIT=0`.

## How it works

- **Push-to-talk** (`src/listen.rs`): Gradium STT gets audio only while the key is down
  (plus the 400 ms before and after), so you pay for speech, not silence. On release the
  audio is flushed and the transcript pasted into the window that was focused when you
  pressed the key. With a language other than English it uses Gradium's `stt-translate`
  model, which writes the transcript in English.
- **Back to back**: cleanup, paste and spoken commands run in order on their own task,
  so a key press during the previous turn's cleanup starts listening at once.
- **The microphone** (`src/audio.rs`): one pw-record process, kept open between turns
  for `PARLEY_WARM_MIC`; between turns only a 400 ms ring buffer is kept.
- **The pill** (`src/overlay.rs`): a separate `parley overlay` process, GTK 4 on a
  layer-shell surface that never takes keyboard focus and lets clicks through, started on
  first use and fed JSON lines by the daemon. Without it parley works the same.
- **Recognising names** (`src/config.rs`): Gradium's keyword boosting gets the coding
  words, your `words.txt` and words from the
  project's file names, so "auth" isn't heard as "off".
- **Files** (`src/project.rs`): the project is the
  Claude session's directory (from its hooks), or the directory of the agent running in
  the terminal. When what you said mentions a file, the project files it most likely
  names go to the cleanup, which writes them as `@path`.
- **Coding agents and other apps**: a window counts as a coding agent if a Claude session
  you've talked to lives there or an agent (claude, codex, aider, ...) runs in its
  terminal. Only there does parley press Enter, run spoken commands and offer answers out
  loud; elsewhere (a browser, Slack, an editor) it pastes the cleaned text and stops.
- **Commands** (`src/commands.rs`): only a whole turn counts ("stop.", "clear the
  chat"); "stop using the old API" is a prompt.
- **Answers out loud** (`src/speak.rs`): asking for one appends a note to the prompt so
  Claude answers in a few speakable sentences (in your language). When that session's
  `Stop` hook fires, the answer is read with Gradium TTS, code blocks and markdown left
  out; if it ends with a question, the microphone opens once it has finished speaking.
- **Cleanup** (`src/cleanup.rs`): Claude Haiku with your `ANTHROPIC_API_KEY`, in two
  steps: a draft from rules and worked examples (self-corrections, filler, lists,
  programming terms, `@path` files), then automatic checks for concrete mistakes (a
  "maybe" or "only" dropped, a retraction left in, a term from `words.txt` still
  misheard). Only when a check finds something does a second call repair it, and the
  repair is kept only if it fixes problems without adding any. If the cleanup fails or
  comes back implausible (much longer than what you said), the transcript goes out as
  it was said. Without a key, transcripts go out as Gradium wrote them.
- **Your words** (`src/words.rs`): `~/.config/parley/words.txt`, one per line,
  optionally `Term: often heard as "x"`. Its terms are boosted in recognition and the
  cleanup prefers their spellings.
- **Which Claude sessions** (`src/daemon.rs`): the Claude Code hooks fire for every
  session, but parley only reacts to sessions you've spoken to. A prompt submitted within
  5 s of a parley paste into the same window marks that session as yours and remembers
  its window. The first prompt sent from a window within 10 minutes of a paste into it is
  compared with the paste, for learning.
- **Asking**: the `Stop` hook carries Claude's last message; if its last paragraph has a
  question mark, Claude is asking. Permission prompts come from the `Notification` hook,
  multiple-choice questions from `PreToolUse` on `AskUserQuestion`.
- **Answering**: an answer turn opens the microphone without a key and ends when
  Gradium's semantic VAD is confident you're done (or after 8 s of nothing). The answer
  goes to that session's window, which is focused first if you've moved away. Pressing
  the key during an answer turn replaces it with a normal one.
- **Pasting** (`src/desktop.rs`): `wl-copy` + `wtype`, Shift+Insert in terminals and
  VS Code-family editors, Ctrl+V elsewhere; the clipboard is restored a second later.
- **History** (`src/history.rs`): the last 500 turns in
  `~/.local/state/parley/history.jsonl`, the audio of the last 10 in `turns/`.

Cost: Gradium charges 3 credits per second of audio sent (10 minutes of talking is about
1,800 credits) and 1 credit per character read aloud (about 750 a minute).

Tests: `cargo test`; `cargo test -- --ignored --nocapture` also runs the live ones against
Gradium with your key (speech made by TTS goes back through STT, one sentence is played
aloud, and the accuracy benchmark runs: about 2,000 credits; `PARLEY_EVAL_ONLY` and
`PARLEY_EVAL_PROFILES` narrow it).
