use crate::core::WindowMove;
use crate::macos::DisplayId;

use yashiki_ipc::Response;

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    ApplyWindowMoves(Vec<WindowMove>),
    FocusWindow {
        window_id: u32,
        pid: i32,
        is_output_change: bool,
    },
    MoveWindowToPosition {
        window_id: u32,
        pid: i32,
        x: i32,
        y: i32,
    },
    SetWindowDimensions {
        window_id: u32,
        pid: i32,
        width: u32,
        height: u32,
    },
    CloseWindow {
        window_id: u32,
        pid: i32,
    },
    ApplyFullscreen {
        window_id: u32,
        pid: i32,
        display_id: DisplayId,
    },
    Retile,
    RetileDisplays(Vec<DisplayId>),
    SendLayoutCommand {
        layout: Option<String>,
        cmd: String,
        args: Vec<String>,
    },
    ExecCommand {
        command: String,
        path: String,
    },
    UpdateLayoutExecPath {
        path: String,
    },
    FocusVisibleWindowIfNeeded,
}

pub struct CommandResult {
    pub response: Response,
    pub effects: Vec<Effect>,
}

impl CommandResult {
    pub fn ok() -> Self {
        Self {
            response: Response::Ok,
            effects: vec![],
        }
    }

    pub fn ok_with_effects(effects: Vec<Effect>) -> Self {
        Self {
            response: Response::Ok,
            effects,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            response: Response::Error {
                message: message.into(),
            },
            effects: vec![],
        }
    }

    pub fn with_response(response: Response) -> Self {
        Self {
            response,
            effects: vec![],
        }
    }
}
