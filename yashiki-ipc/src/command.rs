use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    // Window operations
    FocusWindow { direction: Direction },
    SwapWindow { direction: Direction },
    Zoom,
    CloseWindow,
    ToggleFloat,

    // Tag operations
    ViewTag { tag: u32 },
    ToggleViewTag { tag: u32 },
    ViewTagLast,
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
    FocusedWindow,

    // Exec
    Exec { command: String },
    ExecOrFocus { app_name: String, command: String },

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
    WindowId { id: Option<u32> },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_view_tag_serialization() {
        let cmd = Command::ViewTag { tag: 1 };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"view_tag\""));
        assert!(json.contains("\"tag\":1"));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::ViewTag { tag } => assert_eq!(tag, 1),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_command_focus_window_serialization() {
        let cmd = Command::FocusWindow {
            direction: Direction::Next,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"focus_window\""));
        assert!(json.contains("\"direction\":\"next\""));
    }

    #[test]
    fn test_command_bind_serialization() {
        let cmd = Command::Bind {
            key: "alt-1".to_string(),
            action: Box::new(Command::ViewTag { tag: 1 }),
        };
        let json = serde_json::to_string(&cmd).unwrap();

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::Bind { key, action } => {
                assert_eq!(key, "alt-1");
                match *action {
                    Command::ViewTag { tag } => assert_eq!(tag, 1),
                    _ => panic!("Wrong inner variant"),
                }
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_command_layout_command_serialization() {
        let cmd = Command::LayoutCommand {
            cmd: "set-main-ratio".to_string(),
            args: vec!["0.6".to_string()],
        };
        let json = serde_json::to_string(&cmd).unwrap();

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::LayoutCommand { cmd, args } => {
                assert_eq!(cmd, "set-main-ratio");
                assert_eq!(args, vec!["0.6"]);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_direction_serialization() {
        let cases = [
            (Direction::Left, "\"left\""),
            (Direction::Right, "\"right\""),
            (Direction::Up, "\"up\""),
            (Direction::Down, "\"down\""),
            (Direction::Next, "\"next\""),
            (Direction::Prev, "\"prev\""),
        ];

        for (direction, expected) in cases {
            let json = serde_json::to_string(&direction).unwrap();
            assert_eq!(json, expected);

            let deserialized: Direction = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, direction);
        }
    }

    #[test]
    fn test_output_direction_serialization() {
        let next = OutputDirection::Next;
        let prev = OutputDirection::Prev;

        assert_eq!(serde_json::to_string(&next).unwrap(), "\"next\"");
        assert_eq!(serde_json::to_string(&prev).unwrap(), "\"prev\"");
    }

    #[test]
    fn test_response_ok_serialization() {
        let resp = Response::Ok;
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, "{\"type\":\"ok\"}");

        let deserialized: Response = serde_json::from_str(&json).unwrap();
        matches!(deserialized, Response::Ok);
    }

    #[test]
    fn test_response_error_serialization() {
        let resp = Response::Error {
            message: "something went wrong".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();

        let deserialized: Response = serde_json::from_str(&json).unwrap();
        match deserialized {
            Response::Error { message } => assert_eq!(message, "something went wrong"),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_response_windows_serialization() {
        let resp = Response::Windows {
            windows: vec![WindowInfo {
                id: 123,
                pid: 456,
                title: "Test Window".to_string(),
                app_name: "TestApp".to_string(),
                tags: 0b0001,
                x: 100,
                y: 200,
                width: 800,
                height: 600,
                is_focused: true,
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();

        let deserialized: Response = serde_json::from_str(&json).unwrap();
        match deserialized {
            Response::Windows { windows } => {
                assert_eq!(windows.len(), 1);
                assert_eq!(windows[0].id, 123);
                assert_eq!(windows[0].title, "Test Window");
                assert!(windows[0].is_focused);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_response_state_serialization() {
        let resp = Response::State {
            state: StateInfo {
                visible_tags: 0b0011,
                focused_window_id: Some(42),
                window_count: 5,
            },
        };
        let json = serde_json::to_string(&resp).unwrap();

        let deserialized: Response = serde_json::from_str(&json).unwrap();
        match deserialized {
            Response::State { state } => {
                assert_eq!(state.visible_tags, 0b0011);
                assert_eq!(state.focused_window_id, Some(42));
                assert_eq!(state.window_count, 5);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_response_bindings_serialization() {
        let resp = Response::Bindings {
            bindings: vec![BindingInfo {
                key: "alt-1".to_string(),
                action: "view-tag 1".to_string(),
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();

        let deserialized: Response = serde_json::from_str(&json).unwrap();
        match deserialized {
            Response::Bindings { bindings } => {
                assert_eq!(bindings.len(), 1);
                assert_eq!(bindings[0].key, "alt-1");
            }
            _ => panic!("Wrong variant"),
        }
    }
}
