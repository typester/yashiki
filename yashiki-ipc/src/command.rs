use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    // Window operations
    WindowFocus {
        direction: Direction,
    },
    WindowSwap {
        direction: Direction,
    },
    WindowClose,
    WindowToggleFloat,
    WindowMoveToTag {
        tags: u32,
    },
    WindowToggleTag {
        tags: u32,
    },

    // Tag operations
    TagView {
        tags: u32,
        output: Option<OutputSpecifier>,
    },
    TagToggle {
        tags: u32,
        output: Option<OutputSpecifier>,
    },
    TagViewLast,

    // Output (display) operations
    OutputFocus {
        direction: OutputDirection,
    },
    OutputSend {
        direction: OutputDirection,
    },

    // Layout operations
    LayoutSetDefault {
        layout: String,
    },
    LayoutSet {
        tags: Option<u32>,
        output: Option<OutputSpecifier>,
        layout: String,
    },
    LayoutGet {
        tags: Option<u32>,
        output: Option<OutputSpecifier>,
    },
    LayoutCommand {
        layout: Option<String>,
        cmd: String,
        args: Vec<String>,
    },
    Retile {
        output: Option<OutputSpecifier>,
    },

    // Keybinding operations
    Bind {
        key: String,
        action: Box<Command>,
    },
    Unbind {
        key: String,
    },
    ListBindings,

    // Queries
    ListWindows,
    ListOutputs,
    GetState,
    FocusedWindow,

    // Exec
    Exec {
        command: String,
    },
    ExecOrFocus {
        app_name: String,
        command: String,
    },

    // Exec path
    GetExecPath,
    SetExecPath {
        path: String,
    },
    AddExecPath {
        path: String,
        append: bool,
    },

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
#[serde(untagged)]
pub enum OutputSpecifier {
    Id(u32),
    Name(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Ok,
    Error { message: String },
    Windows { windows: Vec<WindowInfo> },
    Outputs { outputs: Vec<OutputInfo> },
    State { state: StateInfo },
    Bindings { bindings: Vec<BindingInfo> },
    WindowId { id: Option<u32> },
    Layout { layout: String },
    ExecPath { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingInfo {
    pub key: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputInfo {
    pub id: u32,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_main: bool,
    pub visible_tags: u32,
    pub is_focused: bool,
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
    pub default_layout: String,
    pub current_layout: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_tag_view_serialization() {
        let cmd = Command::TagView {
            tags: 1,
            output: None,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"tag_view\""));
        assert!(json.contains("\"tags\":1"));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::TagView { tags, .. } => assert_eq!(tags, 1),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_command_window_focus_serialization() {
        let cmd = Command::WindowFocus {
            direction: Direction::Next,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"window_focus\""));
        assert!(json.contains("\"direction\":\"next\""));
    }

    #[test]
    fn test_command_bind_serialization() {
        let cmd = Command::Bind {
            key: "alt-1".to_string(),
            action: Box::new(Command::TagView {
                tags: 1,
                output: None,
            }),
        };
        let json = serde_json::to_string(&cmd).unwrap();

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::Bind { key, action } => {
                assert_eq!(key, "alt-1");
                match *action {
                    Command::TagView { tags, .. } => assert_eq!(tags, 1),
                    _ => panic!("Wrong inner variant"),
                }
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_command_layout_command_serialization() {
        let cmd = Command::LayoutCommand {
            layout: None,
            cmd: "set-main-ratio".to_string(),
            args: vec!["0.6".to_string()],
        };
        let json = serde_json::to_string(&cmd).unwrap();

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::LayoutCommand { layout, cmd, args } => {
                assert_eq!(layout, None);
                assert_eq!(cmd, "set-main-ratio");
                assert_eq!(args, vec!["0.6"]);
            }
            _ => panic!("Wrong variant"),
        }

        // With layout specified
        let cmd = Command::LayoutCommand {
            layout: Some("tatami".to_string()),
            cmd: "set-outer-gap".to_string(),
            args: vec!["10".to_string()],
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"layout\":\"tatami\""));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::LayoutCommand { layout, cmd, args } => {
                assert_eq!(layout, Some("tatami".to_string()));
                assert_eq!(cmd, "set-outer-gap");
                assert_eq!(args, vec!["10"]);
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
                default_layout: "tatami".to_string(),
                current_layout: Some("byobu".to_string()),
            },
        };
        let json = serde_json::to_string(&resp).unwrap();

        let deserialized: Response = serde_json::from_str(&json).unwrap();
        match deserialized {
            Response::State { state } => {
                assert_eq!(state.visible_tags, 0b0011);
                assert_eq!(state.focused_window_id, Some(42));
                assert_eq!(state.window_count, 5);
                assert_eq!(state.default_layout, "tatami");
                assert_eq!(state.current_layout, Some("byobu".to_string()));
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_command_layout_set_default_serialization() {
        let cmd = Command::LayoutSetDefault {
            layout: "tatami".to_string(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"layout_set_default\""));
        assert!(json.contains("\"layout\":\"tatami\""));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::LayoutSetDefault { layout } => assert_eq!(layout, "tatami"),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_command_layout_set_serialization() {
        // Without tags (current tag)
        let cmd = Command::LayoutSet {
            tags: None,
            output: None,
            layout: "byobu".to_string(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"layout_set\""));
        assert!(json.contains("\"layout\":\"byobu\""));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::LayoutSet { tags, layout, .. } => {
                assert_eq!(tags, None);
                assert_eq!(layout, "byobu");
            }
            _ => panic!("Wrong variant"),
        }

        // With tags
        let cmd = Command::LayoutSet {
            tags: Some(3),
            output: None,
            layout: "tatami".to_string(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"tags\":3"));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::LayoutSet { tags, layout, .. } => {
                assert_eq!(tags, Some(3));
                assert_eq!(layout, "tatami");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_command_layout_get_serialization() {
        // Without tags (current layout)
        let cmd = Command::LayoutGet {
            tags: None,
            output: None,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"layout_get\""));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::LayoutGet { tags, .. } => assert_eq!(tags, None),
            _ => panic!("Wrong variant"),
        }

        // With tags
        let cmd = Command::LayoutGet {
            tags: Some(2),
            output: None,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"tags\":2"));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::LayoutGet { tags, .. } => assert_eq!(tags, Some(2)),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_response_layout_serialization() {
        let resp = Response::Layout {
            layout: "tatami".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"type\":\"layout\""));
        assert!(json.contains("\"layout\":\"tatami\""));

        let deserialized: Response = serde_json::from_str(&json).unwrap();
        match deserialized {
            Response::Layout { layout } => assert_eq!(layout, "tatami"),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_response_bindings_serialization() {
        let resp = Response::Bindings {
            bindings: vec![BindingInfo {
                key: "alt-1".to_string(),
                action: "tag-view 1".to_string(),
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

    #[test]
    fn test_command_get_exec_path_serialization() {
        let cmd = Command::GetExecPath;
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"get_exec_path\""));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, Command::GetExecPath));
    }

    #[test]
    fn test_command_set_exec_path_serialization() {
        let cmd = Command::SetExecPath {
            path: "/opt/homebrew/bin:/usr/local/bin".to_string(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"set_exec_path\""));
        assert!(json.contains("\"path\":\"/opt/homebrew/bin:/usr/local/bin\""));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::SetExecPath { path } => {
                assert_eq!(path, "/opt/homebrew/bin:/usr/local/bin");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_command_add_exec_path_serialization() {
        // Prepend (default)
        let cmd = Command::AddExecPath {
            path: "/opt/homebrew/bin".to_string(),
            append: false,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"add_exec_path\""));
        assert!(json.contains("\"append\":false"));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::AddExecPath { path, append } => {
                assert_eq!(path, "/opt/homebrew/bin");
                assert!(!append);
            }
            _ => panic!("Wrong variant"),
        }

        // Append
        let cmd = Command::AddExecPath {
            path: "/usr/local/bin".to_string(),
            append: true,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"append\":true"));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::AddExecPath { path, append } => {
                assert_eq!(path, "/usr/local/bin");
                assert!(append);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_response_exec_path_serialization() {
        let resp = Response::ExecPath {
            path: "/opt/homebrew/bin:/usr/local/bin".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"type\":\"exec_path\""));
        assert!(json.contains("\"path\":\"/opt/homebrew/bin:/usr/local/bin\""));

        let deserialized: Response = serde_json::from_str(&json).unwrap();
        match deserialized {
            Response::ExecPath { path } => {
                assert_eq!(path, "/opt/homebrew/bin:/usr/local/bin");
            }
            _ => panic!("Wrong variant"),
        }
    }
}
