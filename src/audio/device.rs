//! macOS and Windows: the microphone and speakers through cpal (CoreAudio, WASAPI).
//!
//! A cpal stream has to stay on the thread that made it, so each one lives on a thread
//! of its own until parley lets go of it. The device's own rate and channels are
//! converted to what parley speaks: 24 kHz mono in, 48 kHz mono out.

use std::collections::VecDeque;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use tokio::sync::mpsc;

use super::resample::Resampler;
use super::{CHUNK_BYTES, IN_RATE};

/// What `Player` is fed: 48 kHz, 16-bit, mono (as pw-play is on Linux).
const OUT_RATE: u32 = 48_000;

/// The microphone, as 80 ms chunks of 24 kHz 16-bit mono.
pub struct Input {
    chunks: mpsc::Receiver<Vec<u8>>,
}

impl Input {
    /// `target`: an input whose name contains this; the default input if `None`.
    pub fn open(target: Option<&str>) -> Result<Input> {
        let (tx, chunks) = mpsc::channel(64);
        let (ready_tx, ready) = std_mpsc::channel();
        let target = target.map(str::to_string);
        std::thread::spawn(move || {
            let stream = match open_input(target.as_deref(), tx.clone()) {
                Ok(stream) => stream,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            let _ = ready_tx.send(Ok(()));
            // Keep the stream until the microphone is closed (the receiver dropped).
            while !tx.is_closed() {
                std::thread::sleep(Duration::from_millis(100));
            }
            drop(stream);
        });
        ready
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| anyhow!("the microphone didn't open"))??;
        Ok(Input { chunks })
    }

    /// Fills `chunk`; false once the input has ended.
    pub async fn read(&mut self, chunk: &mut [u8]) -> bool {
        match self.chunks.recv().await {
            Some(c) => {
                chunk.copy_from_slice(&c);
                true
            }
            None => false,
        }
    }
}

fn open_input(target: Option<&str>, tx: mpsc::Sender<Vec<u8>>) -> Result<cpal::Stream> {
    let host = cpal::default_host();
    let device = match target {
        Some(name) => host
            .input_devices()?
            .find(|d| {
                d.description()
                    .is_ok_and(|desc| desc.name().to_lowercase().contains(&name.to_lowercase()))
            })
            .with_context(|| format!("no input named {name:?} (PARLEY_MIC)"))?,
        None => host.default_input_device().context("no microphone")?,
    };
    let config = device.default_input_config()?;
    let format = config.sample_format();
    let config: cpal::StreamConfig = config.into();
    let to_pcm = ToPcm::new(config.sample_rate, config.channels as usize, tx);
    let stream = match format {
        SampleFormat::F32 => build_input::<f32>(&device, &config, to_pcm),
        SampleFormat::I16 => build_input::<i16>(&device, &config, to_pcm),
        SampleFormat::I32 => build_input::<i32>(&device, &config, to_pcm),
        SampleFormat::U16 => build_input::<u16>(&device, &config, to_pcm),
        other => bail!("unsupported microphone format {other}"),
    }?;
    stream.play()?;
    Ok(stream)
}

fn build_input<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut to_pcm: ToPcm,
) -> Result<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let stream = device.build_input_stream(
        *config,
        move |data: &[T], _: &cpal::InputCallbackInfo| to_pcm.push(data),
        |e| crate::daemon::log(&format!("microphone: {e}")),
        None,
    )?;
    Ok(stream)
}

/// Device samples to 24 kHz mono s16 chunks.
struct ToPcm {
    channels: usize,
    resampler: Resampler,
    mono: Vec<f32>,
    resampled: Vec<f32>,
    chunk: Vec<u8>,
    tx: mpsc::Sender<Vec<u8>>,
}

impl ToPcm {
    fn new(rate: u32, channels: usize, tx: mpsc::Sender<Vec<u8>>) -> ToPcm {
        ToPcm {
            channels: channels.max(1),
            resampler: Resampler::new(rate, IN_RATE),
            mono: Vec::new(),
            resampled: Vec::new(),
            chunk: Vec::with_capacity(CHUNK_BYTES),
            tx,
        }
    }

    fn push<T>(&mut self, data: &[T])
    where
        T: Sample,
        f32: FromSample<T>,
    {
        self.mono.clear();
        for frame in data.chunks(self.channels) {
            let sum: f32 = frame.iter().map(|s| f32::from_sample(*s)).sum();
            self.mono.push(sum / frame.len() as f32);
        }
        self.resampled.clear();
        self.resampler.process(&self.mono, &mut self.resampled);
        for &s in &self.resampled {
            let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            self.chunk.extend_from_slice(&v.to_le_bytes());
            if self.chunk.len() == CHUNK_BYTES {
                let full = std::mem::replace(&mut self.chunk, Vec::with_capacity(CHUNK_BYTES));
                let _ = self.tx.try_send(full); // the reader keeps up; never block the device
            }
        }
    }
}

/// The default output, fed 48 kHz 16-bit mono.
pub struct Player {
    queue: Arc<Mutex<VecDeque<f32>>>,
    resampler: Resampler,
    /// Dropping it closes the stream.
    _stop: std_mpsc::Sender<()>,
}

impl Player {
    pub fn start() -> Result<Player> {
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let (stop, stopped) = std_mpsc::channel::<()>();
        let (ready_tx, ready) = std_mpsc::channel();
        let shared = queue.clone();
        std::thread::spawn(move || {
            let stream = match open_output(shared) {
                Ok(s) => s,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            let _ = ready_tx.send(Ok(stream.1));
            let _ = stopped.recv(); // returns when the Player is dropped
            drop(stream.0);
        });
        let rate = ready
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| anyhow!("the speakers didn't open"))??;
        Ok(Player {
            queue,
            resampler: Resampler::new(OUT_RATE, rate),
            _stop: stop,
        })
    }

    pub async fn write(&mut self, pcm: &[u8]) -> Result<()> {
        let samples: Vec<f32> = pcm
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes(*b) as f32 / i16::MAX as f32)
            .collect();
        let mut out = Vec::with_capacity(samples.len() * 2);
        self.resampler.process(&samples, &mut out);
        self.queue.lock().unwrap().extend(out);
        Ok(())
    }

    /// Plays what it has, then returns.
    pub async fn finish(self) -> Result<()> {
        while !self.queue.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        // What the device has buffered still has to come out.
        tokio::time::sleep(Duration::from_millis(150)).await;
        Ok(())
    }
}

fn open_output(queue: Arc<Mutex<VecDeque<f32>>>) -> Result<(cpal::Stream, u32)> {
    let device = cpal::default_host()
        .default_output_device()
        .context("no speakers")?;
    let config = device.default_output_config()?;
    let format = config.sample_format();
    let config: cpal::StreamConfig = config.into();
    let stream = match format {
        SampleFormat::F32 => build_output::<f32>(&device, &config, queue),
        SampleFormat::I16 => build_output::<i16>(&device, &config, queue),
        SampleFormat::I32 => build_output::<i32>(&device, &config, queue),
        SampleFormat::U16 => build_output::<u16>(&device, &config, queue),
        other => bail!("unsupported speaker format {other}"),
    }?;
    stream.play()?;
    Ok((stream, config.sample_rate))
}

fn build_output<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    queue: Arc<Mutex<VecDeque<f32>>>,
) -> Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = (config.channels as usize).max(1);
    let stream = device.build_output_stream(
        *config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let mut q = queue.lock().unwrap();
            for frame in data.chunks_mut(channels) {
                let v = T::from_sample(q.pop_front().unwrap_or(0.0));
                frame.iter_mut().for_each(|s| *s = v);
            }
        },
        |e| crate::daemon::log(&format!("speakers: {e}")),
        None,
    )?;
    Ok(stream)
}
