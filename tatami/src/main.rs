use anyhow::Result;
use std::io::{self, BufRead, Write};
use yashiki_ipc::layout::{LayoutMessage, LayoutResult, WindowGeometry};

struct LayoutState {
    main_count: u32,
    main_ratio: f64,
    inner_gap: u32,
    outer_gap: u32,
    smart_gaps: bool,
    main_window_id: Option<u32>,
}

impl Default for LayoutState {
    fn default() -> Self {
        Self {
            main_count: 1,
            main_ratio: 0.5,
            inner_gap: 0,
            outer_gap: 0,
            smart_gaps: true,
            main_window_id: None,
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
        "inc-main-ratio" => {
            let delta = args
                .first()
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.05);
            state.main_ratio = (state.main_ratio + delta).min(0.9);
            LayoutResult::Ok
        }
        "dec-main-ratio" => {
            let delta = args
                .first()
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.05);
            state.main_ratio = (state.main_ratio - delta).max(0.1);
            LayoutResult::Ok
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
        "set-inner-gap" => {
            if let Some(gap) = args.first().and_then(|s| s.parse::<u32>().ok()) {
                state.inner_gap = gap;
                return LayoutResult::Ok;
            }
            LayoutResult::Error {
                message: "invalid gap value".to_string(),
            }
        }
        "set-outer-gap" => {
            if let Some(gap) = args.first().and_then(|s| s.parse::<u32>().ok()) {
                state.outer_gap = gap;
                return LayoutResult::Ok;
            }
            LayoutResult::Error {
                message: "invalid gap value".to_string(),
            }
        }
        "inc-inner-gap" => {
            let delta = args
                .first()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(1);
            state.inner_gap = state.inner_gap.saturating_add(delta);
            LayoutResult::Ok
        }
        "dec-inner-gap" => {
            let delta = args
                .first()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(1);
            state.inner_gap = state.inner_gap.saturating_sub(delta);
            LayoutResult::Ok
        }
        "inc-outer-gap" => {
            let delta = args
                .first()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(1);
            state.outer_gap = state.outer_gap.saturating_add(delta);
            LayoutResult::Ok
        }
        "dec-outer-gap" => {
            let delta = args
                .first()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(1);
            state.outer_gap = state.outer_gap.saturating_sub(delta);
            LayoutResult::Ok
        }
        "set-smart-gaps" => {
            if let Some(val) = args.first() {
                match val.as_str() {
                    "on" | "true" | "1" => {
                        state.smart_gaps = true;
                        return LayoutResult::Ok;
                    }
                    "off" | "false" | "0" => {
                        state.smart_gaps = false;
                        return LayoutResult::Ok;
                    }
                    _ => {}
                }
            }
            LayoutResult::Error {
                message: "invalid value (use on/off)".to_string(),
            }
        }
        "zoom" => {
            if let Some(id) = args.first().and_then(|s| s.parse::<u32>().ok()) {
                state.main_window_id = Some(id);
                LayoutResult::Ok
            } else {
                LayoutResult::Error {
                    message: "usage: zoom <window_id>".to_string(),
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

    // Reorder windows so main_window_id is first (if present)
    let window_ids: Vec<u32> = if let Some(main_id) = state.main_window_id {
        if window_ids.contains(&main_id) {
            let mut reordered = vec![main_id];
            reordered.extend(window_ids.iter().filter(|&&id| id != main_id));
            reordered
        } else {
            window_ids.to_vec()
        }
    } else {
        window_ids.to_vec()
    };

    let window_count = window_ids.len() as u32;

    // Smart gaps: disable gaps when only one window
    let (outer_gap, inner_gap) = if state.smart_gaps && window_count == 1 {
        (0, 0)
    } else {
        (state.outer_gap, state.inner_gap)
    };

    // Calculate usable area after outer gaps
    let usable_width = width.saturating_sub(outer_gap * 2);
    let usable_height = height.saturating_sub(outer_gap * 2);

    let main_count = state.main_count.min(window_count);
    let stack_count = window_count - main_count;

    // Calculate main/stack widths
    // Total: main_width + inner_gap + stack_width = usable_width (when stack exists)
    let (main_width, stack_width) = if stack_count > 0 {
        let available_for_windows = usable_width.saturating_sub(inner_gap);
        let mw = (available_for_windows as f64 * state.main_ratio) as u32;
        let sw = available_for_windows.saturating_sub(mw);
        (mw, sw)
    } else {
        (usable_width, 0)
    };

    let mut windows = Vec::with_capacity(window_ids.len());

    // Main area - vertically stacked
    // Total: n * height + (n-1) * gap = usable_height
    // height = (usable_height - (n-1) * gap) / n
    let main_total_gaps = inner_gap.saturating_mul(main_count.saturating_sub(1));
    let main_window_height = usable_height.saturating_sub(main_total_gaps) / main_count.max(1);

    for i in 0..main_count as usize {
        let y = outer_gap + (i as u32 * (main_window_height + inner_gap));
        // Last window in main fills remaining space to handle rounding
        let h = if i == main_count as usize - 1 {
            (outer_gap + usable_height).saturating_sub(y)
        } else {
            main_window_height
        };
        windows.push(WindowGeometry {
            id: window_ids[i],
            x: outer_gap as i32,
            y: y as i32,
            width: main_width,
            height: h,
        });
    }

    // Stack area - vertically stacked
    if stack_count > 0 {
        let stack_total_gaps = inner_gap.saturating_mul(stack_count.saturating_sub(1));
        let stack_window_height = usable_height.saturating_sub(stack_total_gaps) / stack_count;
        let stack_x = outer_gap + main_width + inner_gap;

        for i in 0..stack_count as usize {
            let idx = main_count as usize + i;
            let y = outer_gap + (i as u32 * (stack_window_height + inner_gap));
            // Last window fills remaining space to handle rounding
            let h = if i == stack_count as usize - 1 {
                (outer_gap + usable_height).saturating_sub(y)
            } else {
                stack_window_height
            };
            windows.push(WindowGeometry {
                id: window_ids[idx],
                x: stack_x as i32,
                y: y as i32,
                width: stack_width,
                height: h,
            });
        }
    }

    windows
}
