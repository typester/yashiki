use yashiki_ipc::{AutoRaiseMode, CursorWarpMode, OuterGap};

/// Application configuration settings.
/// Grouped separately from window/display state for clarity.
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub exec_path: String,
    pub cursor_warp: CursorWarpMode,
    pub auto_raise_mode: AutoRaiseMode,
    pub auto_raise_delay_ms: u64,
    pub outer_gap: OuterGap,
    pub init_completed: bool,
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }
}
