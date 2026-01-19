use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use anyhow::{Context, Result};

use yashiki_ipc::layout::{LayoutMessage, LayoutResult, WindowGeometry};

fn find_layout_engine(name: &str) -> Option<PathBuf> {
    let command_name = format!("yashiki-layout-{}", name);

    // 1. .app bundle (Contents/Resources/layouts/)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(macos_dir) = exe_path.parent() {
            if let Some(contents_dir) = macos_dir.parent() {
                let layout_path = contents_dir
                    .join("Resources")
                    .join("layouts")
                    .join(&command_name);
                if layout_path.exists() {
                    tracing::debug!("Found layout engine in bundle: {:?}", layout_path);
                    return Some(layout_path);
                }
            }
        }
    }

    // 2. Same directory as executable (development)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let layout_path = exe_dir.join(&command_name);
            if layout_path.exists() {
                tracing::debug!("Found layout engine in exe dir: {:?}", layout_path);
                return Some(layout_path);
            }
        }
    }

    // Not found in local paths
    None
}

pub struct LayoutEngine {
    // Keep process alive until this struct is dropped
    _child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl LayoutEngine {
    pub fn spawn(name: &str, exec_path: &str) -> Result<Self> {
        let command_name = format!("yashiki-layout-{}", name);

        let mut cmd = if let Some(path) = find_layout_engine(name) {
            // Found in bundle or exe directory
            Command::new(path)
        } else {
            // Search in exec_path
            let mut cmd = Command::new(&command_name);
            if !exec_path.is_empty() {
                cmd.env("PATH", exec_path);
            }
            cmd
        };

        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("Failed to spawn layout engine: {}", command_name))?;

        let stdin = child.stdin.take().context("Failed to get stdin")?;
        let stdout = child.stdout.take().context("Failed to get stdout")?;

        tracing::info!("Layout engine '{}' spawned", command_name);

        Ok(Self {
            _child: child,
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
            LayoutResult::Ok | LayoutResult::NeedsRetile => {
                anyhow::bail!("Unexpected 'ok' or 'needs_retile' response for layout request")
            }
        }
    }

    /// Send a command to the layout engine.
    /// Returns Ok(true) if the layout engine requests a retile, Ok(false) otherwise.
    pub fn send_command(&mut self, cmd: &str, args: &[String]) -> Result<bool> {
        let msg = LayoutMessage::Command {
            cmd: cmd.to_string(),
            args: args.to_vec(),
        };

        let result = self.send(&msg)?;

        match result {
            LayoutResult::Ok => Ok(false),
            LayoutResult::NeedsRetile => Ok(true),
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

pub struct LayoutEngineManager {
    engines: HashMap<String, LayoutEngine>,
    exec_path: String,
}

impl LayoutEngineManager {
    pub fn new() -> Self {
        Self {
            engines: HashMap::new(),
            exec_path: String::new(),
        }
    }

    pub fn set_exec_path(&mut self, exec_path: &str) {
        self.exec_path = exec_path.to_string();
    }

    pub fn get_or_spawn(&mut self, name: &str) -> Result<&mut LayoutEngine> {
        if !self.engines.contains_key(name) {
            let engine = LayoutEngine::spawn(name, &self.exec_path)?;
            self.engines.insert(name.to_string(), engine);
        }
        Ok(self.engines.get_mut(name).unwrap())
    }

    pub fn request_layout(
        &mut self,
        name: &str,
        width: u32,
        height: u32,
        window_ids: &[u32],
    ) -> Result<Vec<WindowGeometry>> {
        let engine = self.get_or_spawn(name)?;
        engine.request_layout(width, height, window_ids)
    }

    pub fn send_command(&mut self, name: &str, cmd: &str, args: &[String]) -> Result<bool> {
        let engine = self.get_or_spawn(name)?;
        engine.send_command(cmd, args)
    }
}

impl Default for LayoutEngineManager {
    fn default() -> Self {
        Self::new()
    }
}
