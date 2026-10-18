//! Linux: the microphone through pw-record, the speakers through pw-play.

use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use super::IN_RATE;

/// pw-record's output: 24 kHz, 16-bit, mono.
pub struct Input {
    _child: Child, // killed when dropped
    stdout: ChildStdout,
}

impl Input {
    /// `target`: a PipeWire input by name or serial; the default input if `None`.
    pub fn open(target: Option<&str>) -> Result<Input> {
        let mut command = Command::new("pw-record");
        if let Some(target) = target {
            command.args(["--target", target]);
        }
        let mut child = command
            .args([
                "--raw",
                "--rate",
                &IN_RATE.to_string(),
                "--channels",
                "1",
                "--format",
                "s16",
                "-",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .context("starting pw-record")?;
        let stdout = child.stdout.take().unwrap();
        Ok(Input {
            _child: child,
            stdout,
        })
    }

    /// Fills `chunk`; false once pw-record has ended.
    pub async fn read(&mut self, chunk: &mut [u8]) -> bool {
        self.stdout.read_exact(chunk).await.is_ok()
    }
}

/// pw-play, fed 48 kHz 16-bit mono.
pub struct Player {
    child: Child, // killed if dropped before it finishes
    stdin: ChildStdin,
}

impl Player {
    pub fn start() -> Result<Player> {
        let mut child = Command::new("pw-play")
            .args([
                "--raw",
                "--rate",
                "48000",
                "--channels",
                "1",
                "--format",
                "s16",
                "-",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .context("starting pw-play")?;
        let stdin = child.stdin.take().unwrap();
        Ok(Player { child, stdin })
    }

    pub async fn write(&mut self, pcm: &[u8]) -> Result<()> {
        self.stdin.write_all(pcm).await?;
        Ok(())
    }

    /// Plays what it has, then returns.
    pub async fn finish(self) -> Result<()> {
        let Player { mut child, stdin } = self;
        drop(stdin); // end of input: pw-play finishes what it has, then exits
        child.wait().await?;
        Ok(())
    }
}
