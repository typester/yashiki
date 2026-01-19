use std::path::PathBuf;

use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;

use yashiki_ipc::{Command, Response};

pub struct IpcServer {
    socket_path: PathBuf,
    cmd_tx: mpsc::Sender<(Command, mpsc::Sender<Response>)>,
}

impl IpcServer {
    pub fn new(cmd_tx: mpsc::Sender<(Command, mpsc::Sender<Response>)>) -> Self {
        Self {
            socket_path: PathBuf::from("/tmp/yashiki.sock"),
            cmd_tx,
        }
    }

    pub async fn run(&self) -> Result<()> {
        // Remove existing socket file if it exists
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        tracing::info!("IPC server listening on {:?}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let cmd_tx = self.cmd_tx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, cmd_tx).await {
                            tracing::error!("Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("Accept error: {}", e);
                }
            }
        }
    }

    async fn handle_connection(
        stream: UnixStream,
        cmd_tx: mpsc::Sender<(Command, mpsc::Sender<Response>)>,
    ) -> Result<()> {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        loop {
            line.clear();
            let n = reader.read_line(&mut line).await?;
            if n == 0 {
                break; // EOF
            }

            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let response = match serde_json::from_str::<Command>(line) {
                Ok(cmd) => {
                    tracing::debug!("Received command: {:?}", cmd);
                    let (resp_tx, mut resp_rx) = mpsc::channel(1);

                    if cmd_tx.send((cmd, resp_tx)).await.is_err() {
                        Response::Error {
                            message: "Internal error: command channel closed".to_string(),
                        }
                    } else {
                        resp_rx.recv().await.unwrap_or(Response::Error {
                            message: "Internal error: no response".to_string(),
                        })
                    }
                }
                Err(e) => Response::Error {
                    message: format!("Invalid command: {}", e),
                },
            };

            let response_json = serde_json::to_string(&response)?;
            writer.write_all(response_json.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;
        }

        Ok(())
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}
