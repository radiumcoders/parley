//! Streaming sample-rate conversion for the cpal backend: the device runs at its own
//! rate (44.1 or 48 kHz, usually), Gradium STT takes 24 kHz and the chimes and spoken
//! answers come at 48 kHz.
//!
//! Going down, a windowed-sinc low-pass first keeps what lies above the new Nyquist
//! frequency from folding back into the speech band; then linear interpolation picks the
//! samples. Going up needs only the interpolation.

use std::collections::VecDeque;
use std::f64::consts::PI;

const TAPS: usize = 63;

pub struct Resampler {
    /// Input samples per output sample.
    step: f64,
    /// Where the next output sample falls, in input samples after `prev`.
    pos: f64,
    /// The last filtered input sample of the previous block.
    prev: f32,
    filter: Option<Fir>,
}

impl Resampler {
    pub fn new(from: u32, to: u32) -> Resampler {
        let filter = (to < from).then(|| Fir::low_pass(0.45 * to as f64 / from as f64));
        Resampler {
            step: from as f64 / to as f64,
            pos: 1.0,
            prev: 0.0,
            filter,
        }
    }

    /// Converts the next block of a mono stream.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let filtered: Vec<f32> = match &mut self.filter {
            Some(f) => input.iter().map(|&x| f.next(x)).collect(),
            None => input.to_vec(),
        };
        // Sample i of the virtual stream [prev, filtered...] sits at position i.
        let at = |i: usize| if i == 0 { self.prev } else { filtered[i - 1] };
        let end = filtered.len() as f64;
        while self.pos <= end {
            let i = self.pos.floor() as usize;
            let frac = (self.pos - i as f64) as f32;
            let a = at(i);
            let b = if frac > 0.0 { at(i + 1) } else { a };
            out.push(a + (b - a) * frac);
            self.pos += self.step;
        }
        self.pos -= end;
        if let Some(&last) = filtered.last() {
            self.prev = last;
        }
    }
}

/// A symmetric FIR low-pass (Blackman-windowed sinc), run sample by sample.
struct Fir {
    taps: Vec<f32>,
    history: VecDeque<f32>,
}

impl Fir {
    /// `cutoff`: as a fraction of the input sample rate (0..0.5).
    fn low_pass(cutoff: f64) -> Fir {
        let mid = (TAPS - 1) as f64 / 2.0;
        let mut taps: Vec<f64> = (0..TAPS)
            .map(|n| {
                let x = n as f64 - mid;
                let sinc = if x == 0.0 {
                    2.0 * cutoff
                } else {
                    (2.0 * PI * cutoff * x).sin() / (PI * x)
                };
                let w = 2.0 * PI * n as f64 / (TAPS - 1) as f64;
                sinc * (0.42 - 0.5 * w.cos() + 0.08 * (2.0 * w).cos())
            })
            .collect();
        let sum: f64 = taps.iter().sum();
        taps.iter_mut().for_each(|t| *t /= sum); // unity gain at 0 Hz
        Fir {
            taps: taps.into_iter().map(|t| t as f32).collect(),
            history: VecDeque::from(vec![0.0; TAPS]),
        }
    }

    fn next(&mut self, x: f32) -> f32 {
        self.history.pop_front();
        self.history.push_back(x);
        self.history
            .iter()
            .zip(&self.taps)
            .map(|(h, t)| h * t)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, rate: u32, secs: f64) -> Vec<f32> {
        let n = (rate as f64 * secs) as usize;
        (0..n)
            .map(|i| (2.0 * PI * freq * i as f64 / rate as f64).sin() as f32 * 0.5)
            .collect()
    }

    /// Converts in uneven blocks, as a device callback would deliver them.
    fn run(r: &mut Resampler, input: &[f32]) -> Vec<f32> {
        let mut out = Vec::new();
        let mut rest = input;
        for size in [480, 441, 1000, 7, 512].iter().cycle() {
            if rest.is_empty() {
                break;
            }
            let (block, tail) = rest.split_at((*size).min(rest.len()));
            r.process(block, &mut out);
            rest = tail;
        }
        out
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn keeps_the_length_ratio() {
        for (from, to) in [
            (48_000, 24_000),
            (44_100, 24_000),
            (24_000, 48_000),
            (48_000, 44_100),
        ] {
            let out = run(&mut Resampler::new(from, to), &sine(440.0, from, 2.0));
            let expected = 2.0 * to as f64;
            assert!(
                (out.len() as f64 - expected).abs() <= 2.0,
                "{from}->{to}: {}",
                out.len()
            );
        }
    }

    #[test]
    fn keeps_speech_and_drops_what_would_alias() {
        for from in [48_000, 44_100] {
            // 1 kHz passes at full level (skip the filter's start-up).
            let speech = run(&mut Resampler::new(from, 24_000), &sine(1000.0, from, 1.0));
            let level = rms(&speech[1000..]);
            assert!((level - 0.5 / 2f32.sqrt()).abs() < 0.01, "{from}: {level}");
            // 18 kHz would fold back to 6 kHz: it must be mostly gone.
            let hiss = run(
                &mut Resampler::new(from, 24_000),
                &sine(18_000.0, from, 1.0),
            );
            assert!(rms(&hiss[1000..]) < 0.01, "{from}: {}", rms(&hiss[1000..]));
        }
    }

    #[test]
    fn upsampling_is_smooth() {
        let out = run(
            &mut Resampler::new(24_000, 48_000),
            &sine(440.0, 24_000, 1.0),
        );
        let level = rms(&out[100..]);
        assert!((level - 0.5 / 2f32.sqrt()).abs() < 0.01, "{level}");
        // Every other sample is halfway between its neighbours.
        let reference = sine(440.0, 48_000, 1.0);
        let worst = out[100..47_000]
            .iter()
            .zip(&reference[100..47_000])
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(worst < 0.01, "{worst}");
    }
}
