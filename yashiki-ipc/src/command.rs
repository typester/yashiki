use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    // Window operations
    FocusWindow { direction: Direction },
    SwapWindow { direction: Direction },
    CloseWindow,
    ToggleFloat,

    // Tag operations
    ViewTag { tag: u32 },
    ToggleViewTag { tag: u32 },
    MoveToTag { tag: u32 },
    ToggleWindowTag { tag: u32 },

    // Output (display) operations
    FocusOutput { direction: OutputDirection },
    SendToOutput { direction: OutputDirection },

    // Layout operations
    LayoutCommand { cmd: String, args: Vec<String> },
    Retile,

    // Keybinding operations
    Bind { key: String, action: Box<Command> },
    Unbind { key: String },
    ListBindings,

    // Queries
    ListWindows,
    GetState,

    // Control
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
    Next,
    Prev,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputDirection {
    Next,
    Prev,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Ok,
    Error { message: String },
    Windows { windows: Vec<WindowInfo> },
    State { state: StateInfo },
    Bindings { bindings: Vec<BindingInfo> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingInfo {
    pub key: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: u32,
    pub pid: i32,
    pub title: String,
    pub app_name: String,
    pub tags: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_focused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateInfo {
    pub visible_tags: u32,
    pub focused_window_id: Option<u32>,
    pub window_count: usize,
}
