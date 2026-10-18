//! One turn of speech through Gradium speech-to-text.

use std::time::Duration;

use anyhow::{Result, bail};
use gradium::protocol::stt::{Response, Setup};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Instant, sleep_until};

use crate::audio;
use crate::overlay::Ui;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    /// Push-to-talk: the turn ends when the key is released.
    Hold,
    /// Opened by itself because the agent asked something: the turn ends when Gradium's
    /// semantic VAD decides you're done, or quietly if you say nothing.
    Answer,
}

/// Audio kept after the key comes up: people let go as the last word ends, and saved
/// turns showed speech still going 200 ms after the release.
const RELEASE_TAIL: Duration = Duration::from_millis(400);
/// An answer turn closes after this long without a word.
const ANSWER_SILENCE: Duration = Duration::from_secs(8);
const MAX_TURN: Duration = Duration::from_secs(300);
/// The answer is over once the model is this sure you'll stay quiet over its longest
/// horizon (3 s) for this many 80 ms steps in a row.
const END_PROB: f64 = 0.5;
const END_STEPS: u32 = 3;

pub struct Config {
    pub model: String,
    pub json_config: serde_json::Value,
}

/// What one turn heard, the audio it sent, and how long the stages took.
pub struct Turn {
    /// "" for nothing.
    pub text: String,
    /// 24 kHz 16-bit mono PCM, as sent to Gradium.
    pub audio: Vec<u8>,
    pub timings: String,
}

/// Transcribes one turn from `mic`, which the caller opened as early as it could. A
/// message on `release` ends a `Hold` turn. `pressed` is when the key went down. `ui`
/// shows the microphone's level and the words so far.
pub async fn turn(
    client: &gradium::Client,
    config: &Config,
    mode: Mode,
    mut mic: mpsc::Receiver<Vec<u8>>,
    mut release: oneshot::Receiver<()>,
    pressed: Instant,
    ui: &Ui,
) -> Result<Turn> {
    let setup = Setup::new()
        .with_model_name(&config.model)
        .with_json_config(&config.json_config);
    let (mut tx, mut rx) = client.stt_stream(setup).await?.split();
    let connected = pressed.elapsed();
    let mut audio = Vec::new();
    let mut released_at: Option<Instant> = None;
    let done = |words: &[String], audio: Vec<u8>, released_at: Option<Instant>| {
        let after = released_at.map_or(String::new(), |r| {
            format!(", text {} ms after release", r.elapsed().as_millis())
        });
        Turn {
            text: words.join(" ").trim().to_string(),
            timings: format!(
                "{:.1} s of audio, Gradium connected {} ms after the press{after}",
                audio.len() as f64 / (audio::IN_RATE as f64 * 2.0),
                connected.as_millis(),
            ),
            audio,
        }
    };

    // The receiver isn't safe to cancel mid-message, so it gets its own task.
    let (msg_tx, mut msgs) = mpsc::unbounded_channel();
    let reader = tokio::spawn(async move {
        loop {
            let msg = rx.next_message().await;
            let end = !matches!(msg, Ok(Some(_)));
            if msg_tx.send(msg).is_err() || end {
                break;
            }
        }
    });
    let _reader = AbortOnDrop(reader);

    let started = pressed;
    let far = started + MAX_TURN;
    // When to stop sending audio and flush; set on release or end of answer.
    let mut flush_at: Option<Instant> = None;
    let mut flushed_sent = false;
    // If Gradium never confirms the flush, give up on waiting and keep what arrived.
    let mut give_up = far;
    let mut released = false;
    let mut words: Vec<String> = Vec::new();
    let mut quiet_steps = 0;

    loop {
        let deadline = match (flush_at, mode) {
            (Some(at), _) => at,
            (None, Mode::Answer) if words.is_empty() => started + ANSWER_SILENCE,
            _ => far,
        };
        tokio::select! {
            r = &mut release, if !released => {
                released = true;
                if r.is_ok() && flush_at.is_none() {
                    released_at = Some(Instant::now());
                    flush_at = Some(Instant::now() + RELEASE_TAIL);
                    ui.working("Transcribing");
                }
            }
            chunk = mic.recv(), if !flushed_sent => {
                let Some(chunk) = chunk else { bail!("microphone stopped") };
                if flush_at.is_none() {
                    ui.level(audio::level(&chunk));
                }
                audio.extend_from_slice(&chunk);
                tx.send_audio(chunk).await?;
            }
            _ = sleep_until(deadline), if !flushed_sent => {
                if flush_at.is_none() && words.is_empty() {
                    return Ok(done(&[], audio, None)); // an answer turn nobody answered
                }
                flushed_sent = true;
                give_up = Instant::now() + Duration::from_secs(5);
                mic.close();
                tx.send_flush(1).await?;
            }
            _ = sleep_until(give_up), if flushed_sent => {
                return Ok(done(&words, audio, released_at));
            }
            msg = msgs.recv() => match msg {
                None | Some(Ok(None)) => bail!("Gradium closed the connection"),
                Some(Err(e)) => return Err(e),
                Some(Ok(Some(msg))) => match msg {
                    Response::Text(t) => {
                        words.push(t.text);
                        ui.heard(words.join(" ").trim());
                    }
                    Response::Vad(v) if mode == Mode::Answer && flush_at.is_none() => {
                        let p = v.vad.last().map_or(0.0, |h| h.inactivity_prob);
                        quiet_steps = if p > END_PROB { quiet_steps + 1 } else { 0 };
                        if quiet_steps >= END_STEPS && !words.is_empty() {
                            flush_at = Some(Instant::now());
                            ui.working("Transcribing");
                        }
                    }
                    Response::Flushed { .. } => {
                        let _ = tx.send_eos().await;
                        return Ok(done(&words, audio, released_at));
                    }
                    _ => {}
                },
            },
        }
    }
}

/// Transcribes recorded audio (24 kHz s16 mono) in one go: for replaying saved turns
/// and the benchmark.
pub async fn transcribe(client: &gradium::Client, config: &Config, pcm: &[u8]) -> Result<String> {
    let setup = Setup::new()
        .with_model_name(&config.model)
        .with_json_config(&config.json_config);
    let mut stream = client.stt_stream(setup).await?;
    for chunk in pcm.chunks(audio::CHUNK_BYTES) {
        stream.send_audio(chunk.to_vec()).await?;
    }
    stream.send_flush(1).await?;
    let mut words = Vec::new();
    while let Some(msg) = stream.next_message().await? {
        match msg {
            Response::Text(t) => words.push(t.text),
            Response::Flushed { .. } => break,
            _ => {}
        }
    }
    let _ = stream.send_eos().await;
    Ok(words.join(" ").trim().to_string())
}

struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}
