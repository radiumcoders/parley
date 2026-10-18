//! Microphone and chimes: PipeWire's pw-record and pw-play on Linux, CoreAudio and
//! WASAPI (through cpal) on macOS and Windows.

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod device;
#[cfg(target_os = "linux")]
mod pipewire;
#[cfg(any(target_os = "macos", target_os = "windows", test))]
mod resample;

#[cfg(any(target_os = "macos", target_os = "windows"))]
use device as backend;
#[cfg(target_os = "linux")]
use pipewire as backend;

pub use backend::Player;

use std::collections::VecDeque;
use std::f32::consts::TAU;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use tokio::sync::mpsc;

/// What Gradium STT expects for "pcm": 24 kHz, 16-bit, mono.
pub const IN_RATE: u32 = 24_000;
/// 80 ms of input audio.
pub const CHUNK_BYTES: usize = 1920 * 2;

/// Audio kept from before the key goes down. pw-record takes ~110 ms to start and the
/// key binding a little more, so words said as the key is pressed would be cut; in the
/// benchmark (src/eval.rs) losing the first 120 ms doubled the error rate.
const PREROLL_CHUNKS: usize = 5; // 400 ms

/// The default input (pw-record on Linux). Between turns it can stay open ("warm") so a
/// turn can start with the moment before the key press. Warm audio only sits in a
/// 400 ms ring in memory: nothing is sent or saved until a turn takes it.
pub struct Mic {
    shared: Arc<Mutex<Shared>>,
    reader: tokio::task::JoinHandle<()>,
}

#[derive(Default)]
struct Shared {
    ring: VecDeque<Vec<u8>>,
    turn: Option<mpsc::Sender<Vec<u8>>>,
}

impl Mic {
    /// `target`: an input by name (or, with PipeWire, serial); the default input if `None`.
    pub fn open(target: Option<&str>) -> Result<Mic> {
        let mut input = backend::Input::open(target)?;
        let shared = Arc::new(Mutex::new(Shared::default()));
        let reader = tokio::spawn({
            let shared = shared.clone();
            async move {
                let mut chunk = vec![0u8; CHUNK_BYTES];
                // The input stops when this task ends.
                while input.read(&mut chunk).await {
                    let mut s = shared.lock().unwrap();
                    if let Some(turn) = &s.turn {
                        match turn.try_send(chunk.clone()) {
                            Ok(()) => continue,
                            // 30 s behind: Gradium is stuck, and the turn will fail.
                            Err(mpsc::error::TrySendError::Full(_)) => continue,
                            Err(mpsc::error::TrySendError::Closed(_)) => s.turn = None,
                        }
                    }
                    if s.ring.len() == PREROLL_CHUNKS {
                        s.ring.pop_front();
                    }
                    s.ring.push_back(chunk.clone());
                }
                // The input ended: the turn's audio ends too, so it doesn't wait forever.
                shared.lock().unwrap().turn = None;
            }
        });
        Ok(Mic { shared, reader })
    }

    /// The input is still running.
    pub fn alive(&self) -> bool {
        !self.reader.is_finished()
    }

    /// The audio from now until the receiver is dropped, after what the ring holds from
    /// just before if `preroll`.
    pub fn listen(&self, preroll: bool) -> mpsc::Receiver<Vec<u8>> {
        // Enough to hold everything said while the Gradium connection opens.
        let (tx, rx) = mpsc::channel(400);
        let mut s = self.shared.lock().unwrap();
        for chunk in s.ring.drain(..) {
            if preroll {
                let _ = tx.try_send(chunk);
            }
        }
        s.turn = Some(tx);
        rx
    }
}

impl Drop for Mic {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

/// Root mean square of s16 PCM.
pub fn rms(pcm: &[u8]) -> f32 {
    let samples = pcm.as_chunks::<2>().0;
    let sum: f64 = samples
        .iter()
        .map(|b| (i16::from_le_bytes(*b) as f64).powi(2))
        .sum();
    (sum / samples.len().max(1) as f64).sqrt() as f32
}

/// Loudness for a level meter, 0..1 (RMS on a log scale; speech lands around 0.4-0.9).
pub fn level(pcm: &[u8]) -> f32 {
    ((20.0 * rms(pcm).max(1.0).log10() - 25.0) / 45.0).clamp(0.0, 1.0)
}

#[derive(Clone, Copy)]
pub enum Chime {
    /// The microphone opened.
    Listen,
    /// The microphone closed.
    Done,
    /// The agent is asking you something.
    Ask,
    /// The agent finished.
    Finished,
}

/// Saves `pcm` (as recorded) as a WAV file.
pub fn save_wav(path: &std::path::Path, pcm: &[u8]) -> std::io::Result<()> {
    let mut wav = Vec::with_capacity(44 + pcm.len());
    let len = pcm.len() as u32;
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&IN_RATE.to_le_bytes());
    wav.extend_from_slice(&(IN_RATE * 2).to_le_bytes()); // bytes per second
    wav.extend_from_slice(&2u16.to_le_bytes()); // block align
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&len.to_le_bytes());
    wav.extend_from_slice(pcm);
    std::fs::write(path, wav)
}

/// Plays a short tone in the background.
pub fn chime(chime: Chime) {
    let notes: &[(f32, f32)] = match chime {
        Chime::Listen => &[(880.0, 0.07)],
        Chime::Done => &[(660.0, 0.07)],
        Chime::Ask => &[(660.0, 0.09), (990.0, 0.12)],
        Chime::Finished => &[(523.0, 0.14)],
    };
    let pcm = tones(notes);
    tokio::spawn(async move {
        let Ok(mut player) = Player::start() else {
            return;
        };
        let _ = player.write(&pcm).await;
        let _ = player.finish().await;
    });
}

/// Quiet sine notes with short fades, so they don't click.
fn tones(notes: &[(f32, f32)]) -> Vec<u8> {
    const RATE: f32 = 48_000.0;
    const VOLUME: f32 = 0.12;
    const FADE: f32 = 0.01;
    let mut out = Vec::new();
    for &(freq, secs) in notes {
        let n = (secs * RATE) as usize;
        for i in 0..n {
            let t = i as f32 / RATE;
            let env = (t / FADE).min((secs - t) / FADE).clamp(0.0, 1.0);
            let s = (TAU * freq * t).sin() * env * VOLUME;
            out.extend_from_slice(&((s * i16::MAX as f32) as i16).to_le_bytes());
        }
    }
    out
}
