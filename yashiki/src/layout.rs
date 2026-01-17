use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use yashiki_ipc::layout::{LayoutMessage, LayoutResult, WindowGeometry};

pub struct LayoutEngine {
    #[allow(dead_code)]
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl LayoutEngine {
    pub fn spawn(command: &str) -> Result<Self> {
        let mut child = Command::new(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("Failed to spawn layout engine: {}", command))?;

        let stdin = child.stdin.take().context("Failed to get stdin")?;
        let stdout = child.stdout.take().context("Failed to get stdout")?;

        tracing::info!("Layout engine '{}' spawned", command);

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    pub fn request_layout(
        &mut self,
        width: u32,
        height: u32,
        window_ids: &[u32],
    ) -> Result<Vec<WindowGeometry>> {
        let msg = LayoutMessage::Layout {
            width,
            height,
            windows: window_ids.to_vec(),
        };

        let result = self.send(&msg)?;

        match result {
            LayoutResult::Layout { windows } => Ok(windows),
            LayoutResult::Error { message } => {
                anyhow::bail!("Layout engine error: {}", message)
            }
            LayoutResult::Ok => {
                anyhow::bail!("Unexpected 'ok' response for layout request")
            }
        }
    }

    pub fn send_command(&mut self, cmd: &str, args: &[String]) -> Result<()> {
        let msg = LayoutMessage::Command {
            cmd: cmd.to_string(),
            args: args.to_vec(),
        };

        let result = self.send(&msg)?;

        match result {
            LayoutResult::Ok => Ok(()),
            LayoutResult::Error { message } => {
                anyhow::bail!("Layout engine error: {}", message)
            }
            LayoutResult::Layout { .. } => {
                anyhow::bail!("Unexpected 'layout' response for command")
            }
        }
    }

    fn send(&mut self, msg: &LayoutMessage) -> Result<LayoutResult> {
        serde_json::to_writer(&mut self.stdin, msg)?;
        writeln!(self.stdin)?;
        self.stdin.flush()?;

        let mut line = String::new();
        self.stdout.read_line(&mut line)?;

        let result: LayoutResult = serde_json::from_str(&line)
            .with_context(|| format!("Failed to parse layout response: {}", line.trim()))?;

        Ok(result)
    }
}
