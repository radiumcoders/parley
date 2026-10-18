//! How `parley start`/`stop`/`hook` reach the daemon: a Unix socket on Linux and macOS
//! (`config::socket_path()`), a named pipe on Windows. One request per connection; the
//! client writes it and closes.

use anyhow::Result;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

/// The longest request accepted (hook events carry the last assistant message).
const MAX_REQUEST: u64 = 4 << 20;

/// Reads a whole request: until the client closes, or `MAX_REQUEST`.
pub async fn read_request(stream: impl AsyncRead + Unpin) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    let mut stream = stream.take(MAX_REQUEST);
    loop {
        match stream.read_buf(&mut buf).await {
            Ok(0) => return Some(buf),
            Ok(_) => {}
            // A named pipe reports the client closing as a broken pipe, not an end.
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => return Some(buf),
            Err(_) => return None,
        }
    }
}

#[cfg(unix)]
pub use unix::*;
#[cfg(windows)]
pub use windows::*;

#[cfg(unix)]
mod unix {
    use super::*;
    use crate::config;
    use anyhow::Context;
    use tokio::net::{UnixListener, UnixStream};

    pub struct Listener(UnixListener);

    /// Whether a daemon is already listening.
    pub async fn running() -> bool {
        UnixStream::connect(config::socket_path()).await.is_ok()
    }

    pub fn bind() -> Result<Listener> {
        let path = config::socket_path();
        let _ = std::fs::remove_file(&path);
        let listener =
            UnixListener::bind(&path).with_context(|| format!("binding {}", path.display()))?;
        crate::daemon::log(&format!("listening on {}", path.display()));
        Ok(Listener(listener))
    }

    impl Listener {
        pub async fn accept(&mut self) -> Result<UnixStream> {
            Ok(self.0.accept().await?.0)
        }
    }

    pub async fn send(request: &[u8]) -> Result<()> {
        let mut stream = UnixStream::connect(config::socket_path()).await?;
        stream.write_all(request).await?;
        stream.shutdown().await?;
        Ok(())
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::time::Duration;
    use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions};

    /// ERROR_PIPE_BUSY: every instance of the pipe is taken for the moment.
    const PIPE_BUSY: i32 = 231;

    fn name() -> String {
        let user = std::env::var("USERNAME").unwrap_or_default();
        format!(r"\\.\pipe\parley-{user}")
    }

    pub struct Listener(NamedPipeServer);

    /// Whether a daemon is already listening.
    pub async fn running() -> bool {
        connect().await.is_ok()
    }

    pub fn bind() -> Result<Listener> {
        // Only the first instance: a second daemon fails here instead of sharing it.
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(name())?;
        crate::daemon::log(&format!("listening on {}", name()));
        Ok(Listener(server))
    }

    impl Listener {
        pub async fn accept(&mut self) -> Result<NamedPipeServer> {
            self.0.connect().await?;
            // The next client gets a fresh instance; this one is handed out.
            let next = ServerOptions::new().create(name())?;
            Ok(std::mem::replace(&mut self.0, next))
        }
    }

    async fn connect() -> std::io::Result<tokio::net::windows::named_pipe::NamedPipeClient> {
        for _ in 0..50 {
            match ClientOptions::new().open(name()) {
                Err(e) if e.raw_os_error() == Some(PIPE_BUSY) => {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                other => return other,
            }
        }
        ClientOptions::new().open(name())
    }

    pub async fn send(request: &[u8]) -> Result<()> {
        let mut pipe = connect().await?;
        pipe.write_all(request).await?;
        pipe.flush().await?;
        Ok(())
    }
}
