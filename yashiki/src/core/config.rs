use yashiki_ipc::{CursorWarpMode, OuterGap};

/// Application configuration settings.
/// Grouped separately from window/display state for clarity.
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub exec_path: String,
    pub cursor_warp: CursorWarpMode,
    pub outer_gap: OuterGap,
    pub init_completed: bool,
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }
}
