//! IPC client for communicating with the daemon via Unix domain sockets.
//!
//! Uses newline-delimited JSON (NDJSON) over a Unix socket.

use std::path::PathBuf;

use tokio::net::UnixStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use addon_core::ipc::IpcMessage;

/// Returns the path to the daemon socket.
fn get_socket_path() -> PathBuf {
    std::env::temp_dir().join("addon").join("daemon.sock")
}

/// A client that connects to the daemon via Unix domain socket.
pub struct IpcClient {
    socket_path: PathBuf,
}

impl IpcClient {
    /// Create a new IPC client.
    pub fn new() -> Result<Self, std::io::Error> {
        Ok(Self {
            socket_path: get_socket_path(),
        })
    }

    /// Send a message to the daemon and receive the response.
    ///
    /// This is a request-response pattern: the message is sent,
    /// then the client waits for a single response line.
    pub async fn send(&self, msg: IpcMessage) -> Result<IpcMessage, std::io::Error> {
        let mut stream = UnixStream::connect(&self.socket_path).await?;

        // Send request.
        let json = serde_json::to_string(&msg).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;
        stream.write_all(json.as_bytes()).await?;
        stream.write_all(b"\n").await?;
        stream.flush().await?;

        // Read response.
        let mut reader = stream.reader();
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        let line = line.trim_end_matches(|c| c == '\n' || c == '\r');
        let response: IpcMessage = serde_json::from_str(line).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Invalid JSON: {}", e))
        })?;

        Ok(response)
    }
}
