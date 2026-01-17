use anyhow::Result;
use std::io::{self, BufRead, Write};
use yashiki_ipc::layout::{LayoutMessage, LayoutResult, WindowGeometry};

struct LayoutState {
    main_count: u32,
    main_ratio: f64,
}

impl Default for LayoutState {
    fn default() -> Self {
        Self {
            main_count: 1,
            main_ratio: 0.5,
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
        "set-main-ratio" => {
            if let Some(ratio) = args.first().and_then(|s| s.parse::<f64>().ok()) {
                if (0.1..=0.9).contains(&ratio) {
                    state.main_ratio = ratio;
                    return LayoutResult::Ok;
                }
            }
            LayoutResult::Error {
                message: "invalid ratio (must be 0.1-0.9)".to_string(),
            }
        }
        "inc-main-count" => {
            state.main_count = state.main_count.saturating_add(1);
            LayoutResult::Ok
        }
        "dec-main-count" => {
            if state.main_count > 1 {
                state.main_count -= 1;
            }
            LayoutResult::Ok
        }
        "set-main-count" => {
            if let Some(count) = args.first().and_then(|s| s.parse::<u32>().ok()) {
                if count >= 1 {
                    state.main_count = count;
                    return LayoutResult::Ok;
                }
            }
            LayoutResult::Error {
                message: "invalid count (must be >= 1)".to_string(),
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

    let window_count = window_ids.len() as u32;
    let main_count = state.main_count.min(window_count);
    let stack_count = window_count - main_count;

    let main_width = if stack_count > 0 {
        (width as f64 * state.main_ratio) as u32
    } else {
        width
    };
    let stack_width = width - main_width;

    let mut windows = Vec::with_capacity(window_ids.len());

    // Main area
    let main_height = height / main_count.max(1);
    for i in 0..main_count as usize {
        windows.push(WindowGeometry {
            id: window_ids[i],
            x: 0,
            y: (i as u32 * main_height) as i32,
            width: main_width,
            height: main_height,
        });
    }

    // Stack area
    if stack_count > 0 {
        let stack_height = height / stack_count;
        for i in 0..stack_count as usize {
            let idx = main_count as usize + i;
            windows.push(WindowGeometry {
                id: window_ids[idx],
                x: main_width as i32,
                y: (i as u32 * stack_height) as i32,
                width: stack_width,
                height: stack_height,
            });
        }
    }

    windows
}
