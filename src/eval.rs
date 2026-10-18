//! Accuracy benchmark against the real Gradium API:
//! `cargo test eval -- --ignored --nocapture`.
//!
//! Coding prompts are spoken by Gradium TTS (cached in target/eval, so they're paid for
//! once), in an English voice and in French and German voices that give them an accent.
//! Each goes through STT clean and as a quiet, noisy microphone hears it (speech around
//! -33 dBFS over a -44 dBFS noise floor, like the KREO KAST this was tuned on), and is
//! scored by word error rate. `PARLEY_EVAL_NOISE` can point at a 24 kHz s16 mono file of
//! real room noise; otherwise the noise is synthetic.

use std::path::PathBuf;

use gradium::protocol::AudioFormat;
use gradium::protocol::tts;

use crate::audio;
use crate::config::Settings;

const SENTENCES: &[&str] = &[
    "Refer to the main file and build the project.",
    "Open the auth file and fix the login bug.",
    "Can you check why the login page is slow, and then add a loading spinner to the submit button?",
    "Refactor the useEffect hook in the navbar component so it doesn't re-render on every scroll.",
    "Run cargo clippy and fix every warning it shows.",
    "Add a regex that matches email addresses to the validation module.",
    "What does the setup function return when the config file is missing?",
    "Rename the daemon struct to server and update the README.",
    "Write a unit test for the tokenizer that covers empty strings and emoji.",
    "Move the Svelte store into its own file and import it in the layout.",
    "Push the branch to GitHub and open a pull request.",
    "Why is the websocket connection dropping after thirty seconds?",
];

/// English, then French and German voices reading English: accented speech.
const VOICES: &[&str] = &["NbpkqMVS3CJeq2j8", "YhIHaAfQ0cQPDV9R", "aBNlTApBeOlVKa23"];

fn cache_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/eval")
}

async fn speech(client: &gradium::Client, voice: &str, i: usize) -> Vec<u8> {
    let path = cache_dir().join(format!("{voice}-{i}.pcm"));
    if let Ok(pcm) = std::fs::read(&path) {
        return pcm;
    }
    let setup = tts::Setup::new(voice).with_output_format(AudioFormat::Pcm);
    let result = client.tts(SENTENCES[i], setup).await.expect("tts");
    // 48 kHz from TTS to the 24 kHz STT expects: keep every other sample.
    let pcm: Vec<u8> = result
        .raw_data()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|s| [s[0], s[1]])
        .collect();
    std::fs::create_dir_all(cache_dir()).unwrap();
    std::fs::write(&path, &pcm).unwrap();
    pcm
}

fn samples(pcm: &[u8]) -> Vec<f32> {
    pcm.as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_le_bytes(*b) as f32)
        .collect()
}

fn bytes(samples: &[f32]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|s| (s.clamp(-32768.0, 32767.0) as i16).to_le_bytes())
        .collect()
}

fn rms_db(s: &[f32]) -> f32 {
    let rms = (s.iter().map(|x| x * x).sum::<f32>() / s.len().max(1) as f32).sqrt();
    20.0 * (rms.max(1.0) / 32768.0).log10()
}

/// Where speech starts: the first 10 ms louder than -45 dBFS.
fn onset(s: &[f32]) -> usize {
    let window = audio::IN_RATE as usize / 100;
    s.chunks(window)
        .position(|w| rms_db(w) > -45.0)
        .map_or(0, |i| i * window)
}

/// How the microphone delivers the speech.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Profile {
    /// As synthesized.
    Clean,
    /// Quiet speech (-33 dBFS) over a -44 dBFS noise floor, half a second of room noise
    /// before it, as when the key goes down before you speak.
    Quiet,
    /// Quiet, and the first 120 ms of the speech lost: you spoke as you pressed the
    /// key, before the microphone had opened.
    Clipped,
    /// Clipped, with 300 ms of silence sent ahead of the microphone's audio.
    ClippedPadded,
}

fn mic(pcm: &[u8], noise: &[f32], profile: Profile) -> Vec<u8> {
    let speech = samples(pcm);
    let speech_gain = 10f32.powf((-33.0 - rms_db(&speech)) / 20.0);
    let noise_gain = 10f32.powf((-44.0 - rms_db(noise)) / 20.0);
    let rate = audio::IN_RATE as usize;
    let (lead, skip) = match profile {
        Profile::Clean => return pcm.to_vec(),
        Profile::Quiet => (rate / 2, 0),
        Profile::Clipped | Profile::ClippedPadded => (0, onset(&speech) + rate * 120 / 1000),
    };
    let speech = &speech[skip.min(speech.len())..];
    let mut out: Vec<f32> = (0..lead + speech.len() + rate / 4)
        .map(|i| {
            let s = i
                .checked_sub(lead)
                .and_then(|j| speech.get(j))
                .map_or(0.0, |s| s * speech_gain);
            s + noise[i % noise.len()] * noise_gain
        })
        .collect();
    if let Profile::ClippedPadded = profile {
        out.splice(0..0, std::iter::repeat_n(0.0, rate * 3 / 10));
    }
    bytes(&out)
}

fn room_noise() -> Vec<f32> {
    if let Some(pcm) = std::env::var_os("PARLEY_EVAL_NOISE").and_then(|p| std::fs::read(p).ok()) {
        return samples(&pcm);
    }
    // Hiss with a little hum, from a fixed seed.
    let mut x: u32 = 1;
    (0..audio::IN_RATE as usize * 3)
        .map(|i| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            let white = (x as f32 / u32::MAX as f32 - 0.5) * 2000.0;
            white + 300.0 * (std::f32::consts::TAU * 50.0 * i as f32 / 24_000.0).sin()
        })
        .collect()
}

async fn transcribe(
    client: &gradium::Client,
    json_config: &serde_json::Value,
    pcm: &[u8],
) -> String {
    let config = crate::listen::Config {
        model: "default".into(),
        json_config: json_config.clone(),
    };
    crate::listen::transcribe(client, &config, pcm)
        .await
        .expect("stt")
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect()
}

/// Word-level edit distance.
fn errors(said: &[String], heard: &[String]) -> usize {
    let mut row: Vec<usize> = (0..=heard.len()).collect();
    for (i, s) in said.iter().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, h) in heard.iter().enumerate() {
            let next = (prev + usize::from(s != h))
                .min(row[j] + 1)
                .min(row[j + 1] + 1);
            prev = row[j + 1];
            row[j + 1] = next;
        }
    }
    row[heard.len()]
}

#[tokio::test]
#[ignore]
async fn eval() {
    crate::tls();
    let settings = Settings::load();
    let client = gradium::Client::new(settings.api_key.as_deref().expect("GRADIUM_API_KEY"));
    let noise = room_noise();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let project_words = crate::project::file_words(&crate::project::project_files(root));

    let mut clips = Vec::new();
    for voice in VOICES {
        for i in 0..SENTENCES.len() {
            clips.push((i, speech(&client, voice, i).await));
        }
    }

    // "delay 24, keywords|delay 40, no keywords": just those.
    let only = std::env::var("PARLEY_EVAL_ONLY").unwrap_or_default();
    let mut configs = Vec::new();
    for delay in [10, 16, 24, 40] {
        for keywords in [true, false] {
            let mut c = settings.listen_config(&project_words).json_config;
            c["delay_in_frames"] = delay.into();
            if !keywords {
                c.as_object_mut().unwrap().remove("keywords");
            }
            let name = format!(
                "delay {delay}, {}keywords",
                if keywords { "" } else { "no " }
            );
            if only.is_empty() || only.split('|').any(|o| o == name) {
                configs.push((name, c));
            }
        }
    }

    let profiles: Vec<Profile> = match std::env::var("PARLEY_EVAL_PROFILES") {
        Ok(p) => [
            Profile::Clean,
            Profile::Quiet,
            Profile::Clipped,
            Profile::ClippedPadded,
        ]
        .into_iter()
        .filter(|x| p.split(',').any(|n| n == format!("{x:?}")))
        .collect(),
        Err(_) => vec![Profile::Clean, Profile::Quiet],
    };

    // Gradium allows 3 sessions at once; leave one for a real turn.
    let limit = std::sync::Arc::new(tokio::sync::Semaphore::new(2));
    for (name, config) in configs {
        let mut runs = tokio::task::JoinSet::new();
        for (i, pcm) in clips.iter().cloned() {
            for &profile in &profiles {
                let (client, config, limit) = (client.clone(), config.clone(), limit.clone());
                let audio = mic(&pcm, &noise, profile);
                runs.spawn(async move {
                    let _permit = limit.acquire_owned().await.unwrap();
                    (i, profile, transcribe(&client, &config, &audio).await)
                });
            }
        }
        // Per profile: word errors, words, first words wrong, misses.
        let mut tally: Vec<(usize, usize, usize, Vec<String>)> =
            profiles.iter().map(|_| Default::default()).collect();
        while let Some(run) = runs.join_next().await {
            let (i, profile, heard) = run.unwrap();
            let t = &mut tally[profiles.iter().position(|p| p.eq(&profile)).unwrap()];
            let (said, got) = (words(SENTENCES[i]), words(&heard));
            let e = errors(&said, &got);
            t.0 += e;
            t.1 += said.len();
            t.2 += usize::from(said.first() != got.first());
            if e > 0 {
                t.3.push(heard);
            }
        }
        println!("{name}:");
        for (profile, (err, total, first, misses)) in profiles.iter().zip(&mut tally) {
            println!(
                "  {profile:?}: WER {:.1}%, first word wrong {first}/{}",
                100.0 * *err as f64 / *total as f64,
                clips.len()
            );
            misses.sort();
            for m in misses.iter() {
                println!("      {m}");
            }
        }
    }
}

/// What the cleanup makes of transcripts the benchmark got wrong.
#[tokio::test]
#[ignore]
async fn cleanup_fixes() {
    crate::tls();
    let key = Settings::load().anthropic_key.expect("ANTHROPIC_API_KEY");
    let cleaner = crate::cleanup::Cleaner::new(&key);
    for heard in [
        "Rename the daemon struck to server and update the README.",
        "Move this velvet store into its own file and import it in the layout.",
        "Open the Oath file and fix the login bug.",
        "Push the branch to GitHub and open a pool request.",
        "Write a unique test for the tokenizer that covers empty strings and emoji.",
        "refactor the use effect hook in the navbar component so it doesn't re-render on every scroll",
        "At Aregex that matches email addresses to the validation module.",
        "Run cargo creepy and fix every warning it shows.",
    ] {
        let c = cleaner.clean(heard, &[]).await.unwrap();
        println!("{heard}\n  -> {} ({} ms, {})", c.text, c.ms, c.verdict);
    }
}

/// Speech for any text, for end-to-end tests through a fake microphone:
/// `PARLEY_SAY="Stop." cargo test say -- --ignored` writes target/eval/say.pcm
/// (24 kHz s16 mono).
#[tokio::test]
#[ignore]
async fn say() {
    crate::tls();
    let settings = Settings::load();
    let client = gradium::Client::new(settings.api_key.as_deref().expect("GRADIUM_API_KEY"));
    let text = std::env::var("PARLEY_SAY").expect("PARLEY_SAY");
    let setup = tts::Setup::new(VOICES[0]).with_output_format(AudioFormat::Pcm);
    let result = client.tts(&text, setup).await.expect("tts");
    let pcm: Vec<u8> = result
        .raw_data()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|s| [s[0], s[1]])
        .collect();
    std::fs::create_dir_all(cache_dir()).unwrap();
    std::fs::write(cache_dir().join("say.pcm"), pcm).unwrap();
}
