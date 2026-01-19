use std::path::PathBuf;

use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;

use yashiki_ipc::{StateEvent, SubscribeRequest};

const EVENT_SOCKET_PATH: &str = "/tmp/yashiki-events.sock";

pub struct EventServer {
    socket_path: PathBuf,
    event_rx: broadcast::Receiver<StateEvent>,
    snapshot_tx: tokio::sync::mpsc::Sender<tokio::sync::oneshot::Sender<StateEvent>>,
}

impl EventServer {
    pub fn new(
        event_rx: broadcast::Receiver<StateEvent>,
        snapshot_tx: tokio::sync::mpsc::Sender<tokio::sync::oneshot::Sender<StateEvent>>,
    ) -> Self {
        Self {
            socket_path: PathBuf::from(EVENT_SOCKET_PATH),
            event_rx,
            snapshot_tx,
        }
    }

    pub async fn run(self) -> Result<()> {
        // Remove existing socket file if it exists
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        tracing::info!("Event server listening on {:?}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let event_rx = self.event_rx.resubscribe();
                    let snapshot_tx = self.snapshot_tx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, event_rx, snapshot_tx).await
                        {
                            // Only log if it's not a normal disconnection
                            if !e.to_string().contains("connection reset")
                                && !e.to_string().contains("Broken pipe")
                            {
                                tracing::debug!("Event subscriber disconnected: {}", e);
                            }
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("Event server accept error: {}", e);
                }
            }
        }
    }

    async fn handle_connection(
        stream: UnixStream,
        mut event_rx: broadcast::Receiver<StateEvent>,
        snapshot_tx: tokio::sync::mpsc::Sender<tokio::sync::oneshot::Sender<StateEvent>>,
    ) -> Result<()> {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        // Read subscribe request
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Ok(()); // EOF
        }

        let request: SubscribeRequest = serde_json::from_str(line.trim()).unwrap_or_default();
        let filter = request.effective_filter();

        tracing::debug!("New event subscriber with filter: {:?}", filter);

        // Send snapshot if requested
        if request.snapshot {
            let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
            if snapshot_tx.send(resp_tx).await.is_ok() {
                if let Ok(snapshot) = resp_rx.await {
                    let json = serde_json::to_string(&snapshot)?;
                    writer.write_all(json.as_bytes()).await?;
                    writer.write_all(b"\n").await?;
                    writer.flush().await?;
                }
            }
        }

        // Stream events
        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    if filter.matches(&event) {
                        let json = serde_json::to_string(&event)?;
                        writer.write_all(json.as_bytes()).await?;
                        writer.write_all(b"\n").await?;
                        writer.flush().await?;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("Event subscriber lagged by {} messages", n);
                    // Continue receiving
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }

        Ok(())
    }
}

impl Drop for EventServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

/// Event broadcaster that holds the sender side of the broadcast channel
#[derive(Clone)]
pub struct EventBroadcaster {
    event_tx: broadcast::Sender<StateEvent>,
}

impl EventBroadcaster {
    pub fn new(capacity: usize) -> Self {
        let (event_tx, _) = broadcast::channel(capacity);
        Self { event_tx }
    }

    /// Get a receiver for the event server
    pub fn subscribe(&self) -> broadcast::Receiver<StateEvent> {
        self.event_tx.subscribe()
    }

    /// Send an event to all subscribers
    pub fn send(&self, event: StateEvent) {
        // Ignore send errors (no subscribers)
        let _ = self.event_tx.send(event);
    }
}
