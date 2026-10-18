//! Speaking Claude's answer, when you asked for it out loud: Gradium text-to-speech
//! into the speakers (pw-play on Linux).

use anyhow::Result;
use gradium::protocol::AudioFormat;
use gradium::protocol::tts::{Response, Setup};
use regex::Regex;

use crate::audio::Player;

/// Speaks `text` and returns once it has finished playing. Dropping the future stops it.
pub async fn speak(client: &gradium::Client, voice_id: &str, text: &str) -> Result<()> {
    let mut phraser = Phraser::default();
    let mut phrases = phraser.push(text);
    phrases.extend(phraser.finish());
    if phrases.is_empty() {
        return Ok(());
    }

    let mut player = Player::start()?;

    let setup = Setup::new(voice_id).with_output_format(AudioFormat::Pcm);
    let (mut tx, mut rx) = client.tts_stream(setup).await?.split();
    let send = async move {
        for phrase in phrases {
            tx.send_text(&phrase).await?;
        }
        tx.send_eos().await?;
        anyhow::Ok(())
    };
    let receive = async {
        while let Some(msg) = rx.next_message().await? {
            if let Response::Audio(a) = msg {
                player.write(&a.raw_audio()?).await?;
            }
        }
        anyhow::Ok(())
    };
    tokio::try_join!(send, receive)?;
    player.finish().await?;
    Ok(())
}

/// Turns streamed text into whole phrases fit to be spoken: cut at sentence ends,
/// never inside a word, with code blocks and markdown dropped. The screen shows those.
#[derive(Default)]
pub struct Phraser {
    buf: String,
    in_code: bool,
}

impl Phraser {
    pub fn push(&mut self, piece: &str) -> Vec<String> {
        self.buf.push_str(piece);
        let mut out = Vec::new();
        while let Some(cut) = cut(&self.buf) {
            let phrase: String = self.buf.drain(..cut).collect();
            if let Some(s) = self.speakable(&phrase) {
                out.push(s);
            }
        }
        out
    }

    pub fn finish(&mut self) -> Option<String> {
        let rest = std::mem::take(&mut self.buf);
        self.speakable(&rest)
    }

    fn speakable(&mut self, text: &str) -> Option<String> {
        let mut kept = String::new();
        for (i, part) in text.split("```").enumerate() {
            if i > 0 {
                self.in_code = !self.in_code;
            }
            if !self.in_code {
                kept.push_str(part);
                kept.push(' ');
            }
        }
        static LIST: std::sync::LazyLock<Regex> =
            std::sync::LazyLock::new(|| Regex::new(r"(?m)^\s*(?:[-+]|\d+\.)\s+").unwrap());
        static MARKDOWN: std::sync::LazyLock<Regex> =
            std::sync::LazyLock::new(|| Regex::new(r"[`*_#>|]+").unwrap());
        let kept = LIST.replace_all(&kept, "");
        let kept = MARKDOWN.replace_all(&kept, "");
        let s = kept.split_whitespace().collect::<Vec<_>>().join(" ");
        (!s.is_empty()).then_some(s)
    }
}

/// Where to split off the next phrase: after the last sentence or clause end followed by
/// whitespace, or at a line break; failing that, at a space once the text gets long.
fn cut(buf: &str) -> Option<usize> {
    let bytes = buf.as_bytes();
    let mut last = None;
    for (i, &b) in bytes.iter().enumerate() {
        let boundary = b == b'\n'
            || (matches!(b, b'.' | b'!' | b'?' | b':' | b';')
                && bytes.get(i + 1).is_some_and(|n| n.is_ascii_whitespace()));
        if boundary {
            last = Some(i + 1);
        }
    }
    if last.is_some() {
        return last;
    }
    if buf.len() > 100 {
        return buf.rfind(' ').filter(|&i| i > 0).map(|i| i + 1);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(pieces: &[&str]) -> Vec<String> {
        let mut p = Phraser::default();
        let mut out: Vec<String> = pieces.iter().flat_map(|s| p.push(s)).collect();
        out.extend(p.finish());
        out
    }

    #[test]
    fn splits_at_sentences_not_words() {
        assert_eq!(
            run(&[
                "Fixed it. The tok",
                "en was expiring early",
                ". Diff's on screen."
            ]),
            [
                "Fixed it.",
                "The token was expiring early.",
                "Diff's on screen."
            ]
        );
    }

    #[test]
    fn skips_code_and_markdown() {
        assert_eq!(
            run(&[
                "Here's the **fix**:\n```rust\nlet x = 1;\n```\nThat's ",
                "all."
            ]),
            ["Here's the fix:", "That's all."]
        );
    }

    #[test]
    fn decimals_stay_whole() {
        assert_eq!(run(&["Version 2.5 is out."]), ["Version 2.5 is out."]);
    }
}

/// Against the real Gradium API with the key in ~/.config/parley/env:
/// `cargo test -- --ignored --nocapture`. Speech made by TTS goes back through STT, so
/// no microphone or speakers are involved.
#[cfg(test)]
mod live {
    use gradium::protocol::stt;

    use crate::config::Settings;

    use super::*;

    async fn synthesize(client: &gradium::Client, voice: &str, text: &str) -> Vec<u8> {
        let setup = Setup::new(voice).with_output_format(AudioFormat::Pcm);
        let result = client.tts(text, setup).await.expect("tts");
        // 48 kHz from TTS to the 24 kHz STT expects: keep every other sample.
        result
            .raw_data()
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|s| [s[0], s[1]])
            .collect()
    }

    async fn transcribe(
        client: &gradium::Client,
        config: &crate::listen::Config,
        pcm: &[u8],
    ) -> String {
        let setup = stt::Setup::new()
            .with_model_name(&config.model)
            .with_json_config(&config.json_config);
        let mut stream = client.stt_stream(setup).await.expect("stt");
        for chunk in pcm.chunks(3840) {
            stream.send_audio(chunk.to_vec()).await.unwrap();
        }
        stream.send_audio(vec![0; 3840 * 25]).await.unwrap(); // 2 s of silence
        stream.send_eos().await.unwrap();
        let mut words = Vec::new();
        while let Some(msg) = stream.next_message().await.expect("stt message") {
            if let stt::Response::Text(t) = msg {
                words.push(t.text);
            }
        }
        words.join(" ")
    }

    fn client() -> (gradium::Client, Settings) {
        crate::tls();
        let s = Settings::load();
        (
            gradium::Client::new(s.api_key.as_deref().expect("GRADIUM_API_KEY")),
            s,
        )
    }

    #[tokio::test]
    #[ignore]
    async fn english_with_keywords() {
        let (client, s) = client();
        // This repository's own file names, the way a turn in it would boost them.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut words = crate::project::file_words(&crate::project::project_files(root));
        words.push("auth".into()); // as if the project had src/auth.rs
        let config = s.listen_config(&words);
        println!("keywords: {}", config.json_config["keywords"]);
        let pcm = synthesize(
            &client,
            "NbpkqMVS3CJeq2j8",
            "Open the auth file and fix the login bug.",
        )
        .await;
        let heard = transcribe(&client, &config, &pcm).await;
        println!("heard: {heard}");
        assert!(heard.to_lowercase().contains("auth"));
    }

    #[tokio::test]
    #[ignore]
    async fn french_translated_into_english() {
        let (client, mut s) = client();
        s.lang = "fr".into();
        s.translate = true;
        let pcm = synthesize(
            &client,
            "YhIHaAfQ0cQPDV9R",
            "Ouvre le fichier de configuration et corrige le bug de connexion.",
        )
        .await;
        let config = s.listen_config(&[]);
        assert_eq!(config.model, "stt-translate");
        let heard = transcribe(&client, &config, &pcm).await;
        println!("heard: {heard}");
        assert!(heard.to_lowercase().contains("open") || heard.to_lowercase().contains("config"));
    }

    #[tokio::test]
    #[ignore]
    async fn speaks_aloud() {
        let (client, s) = client();
        let answer = "**Yes.** The tests pass, so it's safe to merge.\n\n```\ncargo test\n```";
        let started = std::time::Instant::now();
        speak(&client, &s.voice, answer).await.expect("speak");
        println!("spoke in {:.1}s", started.elapsed().as_secs_f64());
    }

    /// A push-to-talk turn in real time: audio paced like the microphone, then the key
    /// "released", and how long each stage after that takes.
    async fn timed_turn(
        client: &gradium::Client,
        config: &crate::listen::Config,
        pcm: &[u8],
    ) -> String {
        let setup = stt::Setup::new()
            .with_model_name(&config.model)
            .with_json_config(&config.json_config);
        let t0 = std::time::Instant::now();
        let (mut tx, mut rx) = client.stt_stream(setup).await.expect("stt").split();
        println!("  connected after {} ms", t0.elapsed().as_millis());
        let pcm = pcm.to_vec();
        let sender = tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_millis(80));
            for chunk in pcm
                .chunks(3840)
                .chain(std::iter::repeat_n(&[0u8; 3840][..], 3))
            {
                tick.tick().await;
                tx.send_audio(chunk.to_vec()).await.unwrap();
            }
            let released = std::time::Instant::now();
            tx.send_flush(1).await.unwrap();
            (released, tx)
        });
        let mut words = Vec::new();
        let mut first_text = None;
        loop {
            match rx.next_message().await.expect("msg") {
                Some(stt::Response::Text(t)) => {
                    first_text.get_or_insert(t0.elapsed().as_millis());
                    words.push(t.text);
                }
                Some(stt::Response::Flushed { .. }) | None => break,
                _ => {}
            }
        }
        let (released, mut tx) = sender.await.unwrap();
        println!(
            "  first words after {:?} ms; flushed {} ms after release",
            first_text,
            released.elapsed().as_millis()
        );
        let _ = tx.send_eos().await;
        words.join(" ")
    }

    #[tokio::test]
    #[ignore]
    async fn realtime_latency_and_accuracy() {
        let (client, s) = client();
        let sentence = "Can you check why the login page is slow, and then add a loading spinner to the submit button?";
        let pcm = synthesize(&client, "NbpkqMVS3CJeq2j8", sentence).await;
        println!("audio: {:.1} s", pcm.len() as f64 / 48_000.0);
        let with_delay = |d: u32| {
            let mut c = s.listen_config(&[]);
            c.json_config["delay_in_frames"] = d.into();
            c
        };
        for (name, config) in [
            ("delay 24", with_delay(24)),
            ("delay 10", with_delay(10)),
            ("delay 16", with_delay(16)),
        ] {
            println!("{name}:");
            let heard = timed_turn(&client, &config, &pcm).await;
            println!("  heard: {heard}");
        }
        if let Some(key) = s.anthropic_key.as_deref() {
            let c = crate::cleanup::Cleaner::new(key)
                .clean(sentence, &[])
                .await
                .unwrap();
            println!("cleanup: {} ms, {}", c.ms, c.verdict);
        }
    }
}
