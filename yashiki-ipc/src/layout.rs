use serde::{Deserialize, Serialize};

/// Message from yashiki to layout engine
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LayoutMessage {
    /// Request layout calculation
    Layout {
        width: u32,
        height: u32,
        windows: Vec<u32>, // window IDs in stacking order
    },
    /// Send command to layout engine
    Command { cmd: String, args: Vec<String> },
}

/// Response from layout engine to yashiki
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LayoutResult {
    /// Layout calculation result
    Layout { windows: Vec<WindowGeometry> },
    /// Command succeeded
    Ok,
    /// Error occurred
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
