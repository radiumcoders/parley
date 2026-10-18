# parley

Push-to-talk for Claude Code, in Rust: a daemon (Unix socket at `$XDG_RUNTIME_DIR/parley.sock`)
records while a key is held, transcribes with Gradium STT (`gradium` crate, rustls), and pastes
into the focused window; Claude Code hooks (`parley hook`) tell it when the agent asks or finishes.
Transcripts go through parley's cleanup (`src/cleanup.rs`, Claude Haiku with `ANTHROPIC_API_KEY`
from `~/.config/parley/env`; vocabulary and learning in `src/words.rs`, your words in
`~/.config/parley/words.txt`) before pasting. Speech is Gradium only (a hackathon entry): never
swap in another STT/TTS provider.
Linux is home: Hyprland, PipeWire (`pw-record`, `pw-play`), `wtype`, `wl-clipboard`, `notify-send`,
GTK 4 + gtk4-layer-shell for the on-screen pill (`src/overlay/`, run as `parley overlay`, Linux only).
macOS and Windows have their own backends behind the same functions: `src/audio/device.rs`
(cpal), `src/desktop/{macos,windows}.rs`, the process table in `src/project.rs`, and `src/ipc.rs`
(a named pipe on Windows). Nobody here runs them: type-check them with
`cargo clippy --all-targets --target x86_64-pc-windows-gnu` (or `aarch64-apple-darwin`), with a C
cross-compiler for aws-lc's build script (`CC_<target>` set to a `zig cc -target ...` wrapper).

- `cargo test`, `cargo clippy --all-targets`, `cargo fmt`. Install with `cargo install --path . --root ~/.local`;
  the Hyprland binding and the hooks call `~/.local/bin/parley`, so reinstall after changes and
  restart the daemon (`pkill -f '^/home/jay/.local/bin/parley daemon$'`, anchored so it can't
  match the shell running it; the next key press starts it again).
- Test the daemon in isolation by pointing `XDG_RUNTIME_DIR`, `XDG_CONFIG_HOME` and `XDG_STATE_HOME`
  at scratch dirs, with `PARLEY_DRY_RUN=1` in the scratch env file. The runtime dir must be short
  (a socket path over ~108 bytes fails: use /tmp/claude-1000/prun, not the scratchpad), and a moved
  `XDG_RUNTIME_DIR` hides PipeWire, Wayland and Hyprland: set `PIPEWIRE_RUNTIME_DIR=/run/user/1000`
  and an absolute `WAYLAND_DISPLAY`; hyprctl still won't find Hyprland, so there's no target window.
  For a fake microphone: `pactl load-module module-pipe-source source_name=parley_test_mic
  file=<fifo> format=s16le rate=24000 channels=1`, write PCM to the fifo in real time, and set
  `PARLEY_MIC=parley_test_mic`.
  Never test `paste` from a Claude Code session: it types into the focused window, which is this one.
- See the overlay by piping JSON lines into `parley overlay` (`{"s":"rec","answer":false}`,
  `{"s":"level","v":0.5}`, `{"s":"heard","t":"..."}`, `{"s":"work","t":"Cleaning up"}`,
  `{"s":"done","t":"..."}`) and `grim -g "x,y wxh"`; it sits above waybar, at the bottom of the
  focused monitor. Hyprland animates its resizes: screenshot 600 ms after a change.
- `cargo test eval -- --ignored --nocapture` is the accuracy benchmark (src/eval.rs; TTS clips cached
  in target/eval). Gradium allows 3 STT sessions at once on this account.
- `cargo test -- --ignored --nocapture` runs live Gradium tests (TTS→STT round trips, translation,
  keyword boosting, one sentence played aloud).
- pw-record/pw-play need `--raw` for headerless PCM: without it pw-play refuses stdin and
  pw-record writes an AU header.
- `tls()` in main.rs installs rustls's aws-lc-rs provider up front; a second provider in the build
  (e.g. a dependency pulling in ring) would otherwise make the first TLS connection panic.
- `stt-translate` needs `target_language` as well as `language`: with only `language: "en"`
  it translated French into German.
- Never test commands from here either: "stop" sends Esc to the focused window, which is
  this session.
- `parley hook` must never print or fail: Claude Code feeds UserPromptSubmit hook stdout to the model.
- Hook input (Claude Code 2.1.294): every event has `session_id`, `transcript_path`, `cwd`,
  `hook_event_name`; `Stop` has `stop_hook_active` and `last_assistant_message`.
- Hyprland 0.56 uses a Lua config: dispatchers are Lua, e.g.
  `hyprctl dispatch 'hl.dsp.focus({ window = "address:0x..." })'`.
- Gradium docs as markdown: append `.md` to any docs.gradium.ai page; index at
  https://docs.gradium.ai/llms.txt. Translation is the `stt-translate` STT model with
  `json_config.language` = output language. STT costs 3 credits per second of audio sent.
