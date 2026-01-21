use std::cell::RefCell;

use crate::core::{State, WindowMove};
use crate::layout::LayoutEngineManager;
use crate::platform::WindowManipulator;
use yashiki_ipc::CursorWarpMode;

pub fn focus_visible_window_if_needed<M: WindowManipulator>(
    state: &RefCell<State>,
    manipulator: &M,
) {
    let (window_to_focus, cursor_warp_mode) = {
        let state = state.borrow();
        let display_id = state.focused_display;
        let Some(display) = state.displays.get(&display_id) else {
            return;
        };

        // Get all visible windows on display (including fullscreen and floating)
        let all_visible: Vec<_> = state
            .windows
            .values()
            .filter(|w| {
                w.display_id == display_id
                    && w.tags.intersects(display.visible_tags)
                    && !w.is_hidden()
            })
            .collect();

        if all_visible.is_empty() {
            return;
        }

        // Check if current focus is on a visible window
        let focus_is_visible = state
            .focused
            .map(|id| all_visible.iter().any(|w| w.id == id))
            .unwrap_or(false);

        if focus_is_visible {
            return;
        }

        // Focus the first visible window (prefer tiled, then fullscreen, then floating)
        let window = all_visible
            .iter()
            .find(|w| w.is_tiled())
            .or_else(|| all_visible.iter().find(|w| w.is_fullscreen))
            .or_else(|| all_visible.first());

        match window {
            Some(w) => (Some((w.id, w.pid, w.center())), state.config.cursor_warp),
            None => return,
        }
    };

    if let Some((window_id, pid, (cx, cy))) = window_to_focus {
        tracing::info!("Focusing visible window {} after tag switch", window_id);
        manipulator.focus_window(window_id, pid);

        // Update internal state immediately after focusing
        // This ensures emit_state_change_events will detect the focus change
        state.borrow_mut().set_focused(Some(window_id));

        // Warp cursor if OnFocusChange mode (not OnOutputChange since this is not an output change)
        if cursor_warp_mode == CursorWarpMode::OnFocusChange {
            manipulator.warp_cursor(cx, cy);
        }
    }
}

pub fn switch_tag_for_focused_window(state: &RefCell<State>) -> Option<Vec<WindowMove>> {
    let (focused_id, window_tags, window_display_id, is_hidden) = {
        let s = state.borrow();
        let focused_id = s.focused?;
        let window = s.windows.get(&focused_id)?;
        (
            focused_id,
            window.tags,
            window.display_id,
            window.is_hidden(),
        )
    };

    // Check if window is visible on its display's current visible tags
    let is_visible = {
        let s = state.borrow();
        if let Some(display) = s.displays.get(&window_display_id) {
            window_tags.intersects(display.visible_tags) && !is_hidden
        } else {
            false
        }
    };

    if is_visible {
        return None;
    }

    // Window is hidden, switch to its tag
    let tag = window_tags.first_tag()?;
    tracing::info!(
        "Switching to tag {} for focused window {} (external focus change)",
        tag,
        focused_id
    );

    let moves = state.borrow_mut().view_tags(1 << (tag - 1));
    // Note: Retiling is handled by the caller after applying moves
    Some(moves)
}

/// Notify layout engine of focus change.
/// Returns true if the layout engine requests a retile.
pub fn notify_layout_focus(
    state: &RefCell<State>,
    layout_engine_manager: &RefCell<LayoutEngineManager>,
    window_id: u32,
) -> bool {
    let layout_name = state.borrow().current_layout().to_string();
    let mut manager = layout_engine_manager.borrow_mut();
    match manager.send_command(&layout_name, "focus-changed", &[window_id.to_string()]) {
        Ok(needs_retile) => needs_retile,
        Err(e) => {
            tracing::warn!("Failed to notify layout engine of focus change: {}", e);
            false
        }
    }
}
