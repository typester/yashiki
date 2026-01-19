use std::io::{self, BufRead, Write};

use anyhow::Result;

use yashiki_ipc::layout::{LayoutMessage, LayoutResult, WindowGeometry};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Orientation {
    Horizontal,
    Vertical,
}

struct LayoutState {
    padding: u32,
    orientation: Orientation,
    focused_window_id: Option<u32>,
}

impl Default for LayoutState {
    fn default() -> Self {
        Self {
            padding: 30,
            orientation: Orientation::Horizontal,
            focused_window_id: None,
        }
    }
}

fn main() -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut state = LayoutState::default();

    for line in stdin.lock().lines() {
        let line = line?;
        let msg: LayoutMessage = serde_json::from_str(&line)?;
        let result = handle_message(&mut state, msg);
        serde_json::to_writer(&mut stdout, &result)?;
        writeln!(stdout)?;
        stdout.flush()?;
    }

    Ok(())
}

fn handle_message(state: &mut LayoutState, msg: LayoutMessage) -> LayoutResult {
    match msg {
        LayoutMessage::Layout {
            width,
            height,
            windows,
        } => {
            let geometries = generate_layout(state, width, height, &windows);
            LayoutResult::Layout {
                windows: geometries,
            }
        }
        LayoutMessage::Command { cmd, args } => handle_command(state, &cmd, &args),
    }
}

fn handle_command(state: &mut LayoutState, cmd: &str, args: &[String]) -> LayoutResult {
    match cmd {
        "set-padding" => {
            if let Some(padding) = args.first().and_then(|s| s.parse::<u32>().ok()) {
                state.padding = padding;
                return LayoutResult::Ok;
            }
            LayoutResult::Error {
                message: "invalid padding value".to_string(),
            }
        }
        "inc-padding" => {
            let delta = args
                .first()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(5);
            state.padding = state.padding.saturating_add(delta);
            LayoutResult::Ok
        }
        "dec-padding" => {
            let delta = args
                .first()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(5);
            state.padding = state.padding.saturating_sub(delta);
            LayoutResult::Ok
        }
        "set-orientation" => {
            if let Some(orient) = args.first() {
                match orient.as_str() {
                    "horizontal" | "h" => {
                        state.orientation = Orientation::Horizontal;
                        return LayoutResult::Ok;
                    }
                    "vertical" | "v" => {
                        state.orientation = Orientation::Vertical;
                        return LayoutResult::Ok;
                    }
                    _ => {}
                }
            }
            LayoutResult::Error {
                message: "invalid orientation (use horizontal/h or vertical/v)".to_string(),
            }
        }
        "toggle-orientation" => {
            state.orientation = match state.orientation {
                Orientation::Horizontal => Orientation::Vertical,
                Orientation::Vertical => Orientation::Horizontal,
            };
            LayoutResult::Ok
        }
        "focus-changed" => {
            if let Some(id) = args.first().and_then(|s| s.parse::<u32>().ok()) {
                state.focused_window_id = Some(id);
                LayoutResult::NeedsRetile
            } else {
                LayoutResult::Error {
                    message: "usage: focus-changed <window_id>".to_string(),
                }
            }
        }
        _ => LayoutResult::Error {
            message: format!("unknown command: {}", cmd),
        },
    }
}

fn generate_layout(
    state: &LayoutState,
    width: u32,
    height: u32,
    window_ids: &[u32],
) -> Vec<WindowGeometry> {
    if window_ids.is_empty() {
        return vec![];
    }

    // Single window: full size, no padding
    if window_ids.len() == 1 {
        return vec![WindowGeometry {
            id: window_ids[0],
            x: 0,
            y: 0,
            width,
            height,
        }];
    }

    // Find the focused window index
    let focused_index = if let Some(focused_id) = state.focused_window_id {
        window_ids
            .iter()
            .position(|&id| id == focused_id)
            .unwrap_or(0)
    } else {
        0
    };

    // Reorder windows: focused window goes to the end (rightmost/frontmost)
    let mut ordered_ids: Vec<u32> = window_ids
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != focused_index)
        .map(|(_, &id)| id)
        .collect();
    ordered_ids.push(window_ids[focused_index]);

    let window_count = ordered_ids.len();
    let padding = state.padding;

    // Each window is offset by index * padding
    // All windows have the same size, leaving room for all tabs
    let total_offset = padding * (window_count as u32 - 1);

    ordered_ids
        .iter()
        .enumerate()
        .map(|(index, &id)| {
            let offset = padding * index as u32;

            match state.orientation {
                Orientation::Horizontal => WindowGeometry {
                    id,
                    x: offset as i32,
                    y: 0,
                    width: width.saturating_sub(total_offset),
                    height,
                },
                Orientation::Vertical => WindowGeometry {
                    id,
                    x: 0,
                    y: offset as i32,
                    width,
                    height: height.saturating_sub(total_offset),
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_window() {
        let state = LayoutState::default();
        let windows = generate_layout(&state, 1920, 1080, &[1]);
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].x, 0);
        assert_eq!(windows[0].y, 0);
        assert_eq!(windows[0].width, 1920);
        assert_eq!(windows[0].height, 1080);
    }

    #[test]
    fn test_two_windows_focused_first() {
        let mut state = LayoutState::default();
        state.padding = 30;
        state.focused_window_id = Some(1); // window ID 1 is at index 0

        let windows = generate_layout(&state, 1920, 1080, &[1, 2]);
        assert_eq!(windows.len(), 2);

        // Reordered: [2, 1] (focused 1 goes to end)
        // total_offset = 30 * 1 = 30
        // Window 2 (index 0): x=0
        assert_eq!(windows[0].id, 2);
        assert_eq!(windows[0].x, 0);
        assert_eq!(windows[0].width, 1920 - 30);

        // Window 1 (index 1, focused): x=30
        assert_eq!(windows[1].id, 1);
        assert_eq!(windows[1].x, 30);
        assert_eq!(windows[1].width, 1920 - 30);
    }

    #[test]
    fn test_two_windows_focused_second() {
        let mut state = LayoutState::default();
        state.padding = 30;
        state.focused_window_id = Some(2); // window ID 2 is at index 1

        let windows = generate_layout(&state, 1920, 1080, &[1, 2]);
        assert_eq!(windows.len(), 2);

        // Reordered: [1, 2] (focused 2 already at end)
        // Window 1 (index 0): x=0
        assert_eq!(windows[0].id, 1);
        assert_eq!(windows[0].x, 0);

        // Window 2 (index 1, focused): x=30
        assert_eq!(windows[1].id, 2);
        assert_eq!(windows[1].x, 30);
    }

    #[test]
    fn test_three_windows_middle_focused() {
        let mut state = LayoutState::default();
        state.padding = 30;
        state.focused_window_id = Some(2); // window ID 2 is at index 1

        let windows = generate_layout(&state, 1920, 1080, &[1, 2, 3]);

        // Reordered: [1, 3, 2] (focused 2 goes to end)
        // total_offset = 30 * 2 = 60
        assert_eq!(windows[0].id, 1);
        assert_eq!(windows[0].x, 0);
        assert_eq!(windows[0].width, 1920 - 60);

        assert_eq!(windows[1].id, 3);
        assert_eq!(windows[1].x, 30);
        assert_eq!(windows[1].width, 1920 - 60);

        assert_eq!(windows[2].id, 2); // focused, rightmost
        assert_eq!(windows[2].x, 60);
        assert_eq!(windows[2].width, 1920 - 60);
    }

    #[test]
    fn test_vertical_orientation() {
        let mut state = LayoutState::default();
        state.padding = 30;
        state.orientation = Orientation::Vertical;
        state.focused_window_id = Some(1);

        let windows = generate_layout(&state, 1920, 1080, &[1, 2]);

        // Reordered: [2, 1] (focused 1 goes to end)
        assert_eq!(windows[0].id, 2);
        assert_eq!(windows[0].y, 0);
        assert_eq!(windows[0].height, 1080 - 30);

        assert_eq!(windows[1].id, 1); // focused
        assert_eq!(windows[1].y, 30);
        assert_eq!(windows[1].height, 1080 - 30);
    }

    #[test]
    fn test_five_windows_staggered() {
        let mut state = LayoutState::default();
        state.padding = 30;
        state.focused_window_id = Some(3); // window ID 3 is at index 2

        let windows = generate_layout(&state, 1920, 1080, &[1, 2, 3, 4, 5]);

        // Reordered: [1, 2, 4, 5, 3] (focused 3 goes to end)
        // total_offset = 30 * 4 = 120
        // All windows have width = 1920 - 120 = 1800
        assert_eq!(windows[0].id, 1);
        assert_eq!(windows[0].x, 0);
        assert_eq!(windows[0].width, 1920 - 120);

        assert_eq!(windows[1].id, 2);
        assert_eq!(windows[1].x, 30);
        assert_eq!(windows[1].width, 1920 - 120);

        assert_eq!(windows[2].id, 4);
        assert_eq!(windows[2].x, 60);
        assert_eq!(windows[2].width, 1920 - 120);

        assert_eq!(windows[3].id, 5);
        assert_eq!(windows[3].x, 90);
        assert_eq!(windows[3].width, 1920 - 120);

        assert_eq!(windows[4].id, 3); // focused, rightmost
        assert_eq!(windows[4].x, 120);
        assert_eq!(windows[4].width, 1920 - 120);
    }

    #[test]
    fn test_focus_changed_command() {
        let mut state = LayoutState::default();
        let result = handle_command(&mut state, "focus-changed", &["42".to_string()]);
        assert!(matches!(result, LayoutResult::NeedsRetile));
        assert_eq!(state.focused_window_id, Some(42));
    }

    #[test]
    fn test_set_padding_command() {
        let mut state = LayoutState::default();
        let result = handle_command(&mut state, "set-padding", &["50".to_string()]);
        assert!(matches!(result, LayoutResult::Ok));
        assert_eq!(state.padding, 50);
    }

    #[test]
    fn test_toggle_orientation_command() {
        let mut state = LayoutState::default();
        assert_eq!(state.orientation, Orientation::Horizontal);

        handle_command(&mut state, "toggle-orientation", &[]);
        assert_eq!(state.orientation, Orientation::Vertical);

        handle_command(&mut state, "toggle-orientation", &[]);
        assert_eq!(state.orientation, Orientation::Horizontal);
    }
}
