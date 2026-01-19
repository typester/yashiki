use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

use anyhow::{Context, Result};

use yashiki_ipc::{Command, EventFilter, Response, StateEvent, SubscribeRequest};

const SOCKET_PATH: &str = "/tmp/yashiki.sock";
const EVENT_SOCKET_PATH: &str = "/tmp/yashiki-events.sock";

pub struct IpcClient {
    stream: UnixStream,
}

impl IpcClient {
    pub fn connect() -> Result<Self> {
        let stream =
            UnixStream::connect(SOCKET_PATH).context("Failed to connect to yashiki daemon")?;
        Ok(Self { stream })
    }

    pub fn send(&mut self, cmd: &Command) -> Result<Response> {
        let json = serde_json::to_string(cmd)?;
        writeln!(self.stream, "{}", json)?;
        self.stream.flush()?;

        let mut reader = BufReader::new(&self.stream);
        let mut line = String::new();
        reader.read_line(&mut line)?;

        let response: Response = serde_json::from_str(&line)?;
        Ok(response)
    }
}

/// Client for subscribing to state events
pub struct EventClient {
    reader: BufReader<UnixStream>,
}

impl EventClient {
    pub fn connect(request: &SubscribeRequest) -> Result<Self> {
        let mut stream = UnixStream::connect(EVENT_SOCKET_PATH)
            .context("Failed to connect to yashiki event server")?;

        // Send subscribe request
        let json = serde_json::to_string(request)?;
        writeln!(stream, "{}", json)?;
        stream.flush()?;

        let reader = BufReader::new(stream);
        Ok(Self { reader })
    }

    /// Read the next event (blocking)
    pub fn next_event(&mut self) -> Result<StateEvent> {
        let mut line = String::new();
        self.reader.read_line(&mut line)?;
        if line.is_empty() {
            anyhow::bail!("Connection closed");
        }
        let event: StateEvent = serde_json::from_str(&line)?;
        Ok(event)
    }
}

/// Subscribe and print events to stdout
pub fn subscribe_and_print(snapshot: bool, filter: Option<EventFilter>) -> Result<()> {
    let request = SubscribeRequest {
        snapshot,
        filter: filter.unwrap_or_default(),
    };

    let mut client = EventClient::connect(&request)?;

    loop {
        match client.next_event() {
            Ok(event) => {
                let json = serde_json::to_string(&event)?;
                println!("{}", json);
            }
            Err(e) => {
                if e.to_string().contains("Connection closed") {
                    break;
                }
                return Err(e);
            }
        }
    }

    Ok(())
}
