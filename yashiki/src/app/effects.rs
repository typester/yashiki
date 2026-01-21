use std::cell::RefCell;

use crate::core::State;
use crate::effect::Effect;
use crate::layout::LayoutEngineManager;
use crate::platform::WindowManipulator;
use yashiki_ipc::CursorWarpMode;

use super::focus::{focus_visible_window_if_needed, notify_layout_focus};
use super::retile::{do_retile, do_retile_display};

/// Execute side effects.
pub fn execute_effects<M: WindowManipulator>(
    effects: Vec<Effect>,
    state: &RefCell<State>,
    layout_engine_manager: &RefCell<LayoutEngineManager>,
    manipulator: &M,
) -> Result<(), String> {
    for effect in effects {
        match effect {
            Effect::ApplyWindowMoves(moves) => {
                manipulator.apply_window_moves(&moves);
            }
            Effect::FocusWindow {
                window_id,
                pid,
                is_output_change,
            } => {
                manipulator.focus_window(window_id, pid);

                // Update state.focused immediately after focusing
                // This ensures consecutive focus commands work correctly
                // even if accessibility events are delayed or missing
                state.borrow_mut().set_focused(Some(window_id));

                // Warp cursor based on cursor_warp mode
                let cursor_warp_mode = state.borrow().config.cursor_warp;
                let should_warp = match cursor_warp_mode {
                    CursorWarpMode::Disabled => false,
                    CursorWarpMode::OnOutputChange => is_output_change,
                    CursorWarpMode::OnFocusChange => true,
                };

                if should_warp {
                    if let Some(window) = state.borrow().windows.get(&window_id) {
                        let (cx, cy) = window.center();
                        manipulator.warp_cursor(cx, cy);
                    }
                }

                if notify_layout_focus(state, layout_engine_manager, window_id) {
                    do_retile(state, layout_engine_manager, manipulator);
                }
            }
            Effect::MoveWindowToPosition {
                window_id,
                pid,
                x,
                y,
            } => {
                manipulator.move_window_to_position(window_id, pid, x, y);
            }
            Effect::SetWindowDimensions {
                window_id,
                pid,
                width,
                height,
            } => {
                manipulator.set_window_dimensions(window_id, pid, width, height);
            }
            Effect::CloseWindow { window_id, pid } => {
                manipulator.close_window(window_id, pid);
            }
            Effect::ApplyFullscreen {
                window_id,
                pid,
                display_id,
            } => {
                let state = state.borrow();
                let outer_gap = state.config.outer_gap;
                if let Some(display) = state.displays.get(&display_id) {
                    manipulator.set_window_frame(
                        window_id,
                        pid,
                        display.frame.x + outer_gap.left as i32,
                        display.frame.y + outer_gap.top as i32,
                        display.frame.width.saturating_sub(outer_gap.horizontal()),
                        display.frame.height.saturating_sub(outer_gap.vertical()),
                    );
                }
            }
            Effect::Retile => {
                do_retile(state, layout_engine_manager, manipulator);
            }
            Effect::RetileDisplays(display_ids) => {
                for display_id in display_ids {
                    do_retile_display(state, layout_engine_manager, manipulator, display_id);
                }
            }
            Effect::SendLayoutCommand { layout, cmd, args } => {
                let layout_name = layout
                    .clone()
                    .unwrap_or_else(|| state.borrow().current_layout().to_string());
                let mut manager = layout_engine_manager.borrow_mut();
                if let Err(e) = manager.send_command(&layout_name, &cmd, &args) {
                    return Err(format!("Layout command failed: {}", e));
                }
            }
            Effect::ExecCommand { command, path } => {
                manipulator.exec_command(&command, &path)?;
            }
            Effect::ExecCommandTracked { command, path } => {
                match manipulator.exec_command_tracked(&command, &path) {
                    Ok(pid) => {
                        state
                            .borrow_mut()
                            .tracked_processes
                            .push(crate::core::TrackedProcess {
                                pid,
                                _command: command.clone(),
                            });
                        tracing::info!("Tracked process started: {} (pid={})", command, pid);
                    }
                    Err(e) => return Err(e),
                }
            }
            Effect::UpdateLayoutExecPath { path } => {
                layout_engine_manager.borrow_mut().set_exec_path(&path);
            }
            Effect::FocusVisibleWindowIfNeeded => {
                focus_visible_window_if_needed(state, manipulator);
            }
            Effect::WarpCursorToDisplay { display_id } => {
                let cursor_warp_mode = state.borrow().config.cursor_warp;
                let should_warp = match cursor_warp_mode {
                    CursorWarpMode::Disabled => false,
                    CursorWarpMode::OnOutputChange | CursorWarpMode::OnFocusChange => true,
                };
                if should_warp {
                    if let Some(display) = state.borrow().displays.get(&display_id) {
                        let (cx, cy) = display.frame.center();
                        manipulator.warp_cursor(cx, cy);
                    }
                }
            }
        }
    }
    Ok(())
}
