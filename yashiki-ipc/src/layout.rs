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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowGeometry {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_message_layout_serialization() {
        let msg = LayoutMessage::Layout {
            width: 1920,
            height: 1080,
            windows: vec![1, 2, 3],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"layout\""));
        assert!(json.contains("\"width\":1920"));
        assert!(json.contains("\"height\":1080"));
        assert!(json.contains("\"windows\":[1,2,3]"));

        let deserialized: LayoutMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            LayoutMessage::Layout {
                width,
                height,
                windows,
            } => {
                assert_eq!(width, 1920);
                assert_eq!(height, 1080);
                assert_eq!(windows, vec![1, 2, 3]);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_layout_message_command_serialization() {
        let msg = LayoutMessage::Command {
            cmd: "set-main-ratio".to_string(),
            args: vec!["0.6".to_string()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"command\""));

        let deserialized: LayoutMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            LayoutMessage::Command { cmd, args } => {
                assert_eq!(cmd, "set-main-ratio");
                assert_eq!(args, vec!["0.6"]);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_layout_result_layout_serialization() {
        let result = LayoutResult::Layout {
            windows: vec![
                WindowGeometry {
                    id: 1,
                    x: 0,
                    y: 0,
                    width: 960,
                    height: 1080,
                },
                WindowGeometry {
                    id: 2,
                    x: 960,
                    y: 0,
                    width: 960,
                    height: 540,
                },
            ],
        };
        let json = serde_json::to_string(&result).unwrap();

        let deserialized: LayoutResult = serde_json::from_str(&json).unwrap();
        match deserialized {
            LayoutResult::Layout { windows } => {
                assert_eq!(windows.len(), 2);
                assert_eq!(windows[0].id, 1);
                assert_eq!(windows[0].width, 960);
                assert_eq!(windows[1].x, 960);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_layout_result_ok_serialization() {
        let result = LayoutResult::Ok;
        let json = serde_json::to_string(&result).unwrap();
        assert_eq!(json, "{\"type\":\"ok\"}");

        let deserialized: LayoutResult = serde_json::from_str(&json).unwrap();
        matches!(deserialized, LayoutResult::Ok);
    }

    #[test]
    fn test_layout_result_error_serialization() {
        let result = LayoutResult::Error {
            message: "Invalid ratio".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();

        let deserialized: LayoutResult = serde_json::from_str(&json).unwrap();
        match deserialized {
            LayoutResult::Error { message } => assert_eq!(message, "Invalid ratio"),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_window_geometry_equality() {
        let g1 = WindowGeometry {
            id: 1,
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        };
        let g2 = WindowGeometry {
            id: 1,
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        };
        let g3 = WindowGeometry {
            id: 2,
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        };

        assert_eq!(g1, g2);
        assert_ne!(g1, g3);
    }
}
