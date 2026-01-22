use super::super::{Window, WindowId};
use crate::macos::DisplayId;

use super::super::state::{State, WindowMove};

pub fn compute_global_hide_position(state: &State) -> (i32, i32) {
    let mut max_x = 0i32;
    let mut max_y = 0i32;

    for display in state.displays.values() {
        let right = display.frame.x + display.frame.width as i32;
        let bottom = display.frame.y + display.frame.height as i32;
        max_x = max_x.max(right);
        max_y = max_y.max(bottom);
    }

    (max_x - 1, max_y - 1)
}

/// Compute per-display hide position to avoid cross-display interference.
/// Selects a corner that doesn't overlap with other displays.
/// Priority: bottom-right → bottom-left → top-right → top-left
pub fn compute_hide_position_for_display(state: &State, display_id: DisplayId) -> (i32, i32) {
    let Some(display) = state.displays.get(&display_id) else {
        return compute_global_hide_position(state);
    };

    let frame = &display.frame;

    // 4 corner candidates (keep 1px inside the screen)
    let corners = [
        (
            frame.x + frame.width as i32 - 1,
            frame.y + frame.height as i32 - 1,
        ), // bottom-right
        (frame.x, frame.y + frame.height as i32 - 1), // bottom-left
        (frame.x + frame.width as i32 - 1, frame.y),  // top-right
        (frame.x, frame.y),                           // top-left
    ];

    // Find a corner that doesn't overlap with other displays
    for (x, y) in corners {
        let overlaps = state.displays.values().any(|other| {
            if other.id == display_id {
                return false;
            }
            let ox = other.frame.x;
            let oy = other.frame.y;
            let ow = other.frame.width as i32;
            let oh = other.frame.height as i32;
            x >= ox && x < ox + ow && y >= oy && y < oy + oh
        });
        if !overlaps {
            return (x, y);
        }
    }

    // Fallback (shouldn't normally reach here)
    (
        frame.x + frame.width as i32 - 1,
        frame.y + frame.height as i32 - 1,
    )
}

pub fn compute_layout_changes_for_display(
    state: &mut State,
    display_id: DisplayId,
) -> Vec<WindowMove> {
    let Some(display) = state.displays.get(&display_id) else {
        return vec![];
    };
    let visible_tags = display.visible_tags;
    let (hide_x, hide_y) = compute_hide_position_for_display(state, display_id);

    let mut moves = Vec::new();

    for window in state.windows.values_mut() {
        if window.display_id != display_id {
            tracing::trace!(
                "Skipping window {} - display {} != {}",
                window.id,
                window.display_id,
                display_id
            );
            continue;
        }

        let should_be_visible = window.tags.intersects(visible_tags);
        let is_visible = !window.is_hidden();

        tracing::debug!(
            "Window {}: tags={}, visible_tags={}, should_visible={}, frame=({},{}), is_visible={}, saved_frame={:?}",
            window.id,
            window.tags.mask(),
            visible_tags.mask(),
            should_be_visible,
            window.frame.x,
            window.frame.y,
            is_visible,
            window.saved_frame.as_ref().map(|f| (f.x, f.y))
        );

        if should_be_visible && !is_visible {
            if let Some(saved) = window.saved_frame.take() {
                tracing::debug!(
                    "Showing window {} from ({}, {}) to ({}, {})",
                    window.id,
                    window.frame.x,
                    window.frame.y,
                    saved.x,
                    saved.y
                );
                moves.push(WindowMove {
                    window_id: window.id,
                    pid: window.pid,
                    old_x: window.frame.x,
                    old_y: window.frame.y,
                    new_x: saved.x,
                    new_y: saved.y,
                });
                window.frame = saved;
            }
        } else if !should_be_visible && is_visible {
            tracing::debug!(
                "Hiding window {} from ({}, {}) to ({}, {})",
                window.id,
                window.frame.x,
                window.frame.y,
                hide_x,
                hide_y
            );
            moves.push(WindowMove {
                window_id: window.id,
                pid: window.pid,
                old_x: window.frame.x,
                old_y: window.frame.y,
                new_x: hide_x,
                new_y: hide_y,
            });
            window.saved_frame = Some(window.frame);
            window.frame.x = hide_x;
            window.frame.y = hide_y;
        }
    }

    moves
}

pub fn visible_windows_on_display(state: &State, display_id: DisplayId) -> Vec<&Window> {
    let Some(display) = state.displays.get(&display_id) else {
        return vec![];
    };
    let mut windows: Vec<&Window> = state
        .windows
        .values()
        .filter(|w| {
            w.display_id == display_id
                && w.tags.intersects(display.visible_tags)
                && !w.is_hidden()
                && w.is_tiled()
        })
        .collect();

    windows.sort_by_key(|w| {
        display
            .window_order
            .iter()
            .position(|&id| id == w.id)
            .map(|p| (0, p))
            .unwrap_or((1, w.id as usize))
    });
    windows
}

pub fn add_to_window_order(state: &mut State, window_id: WindowId, display_id: DisplayId) {
    if let Some(display) = state.displays.get_mut(&display_id) {
        if !display.window_order.contains(&window_id) {
            display.window_order.push(window_id);
        }
    }
}

pub fn remove_from_window_order(state: &mut State, window_id: WindowId) {
    for display in state.displays.values_mut() {
        display.window_order.retain(|&id| id != window_id);
    }
}

pub fn compute_layout_changes(state: &mut State, display_id: DisplayId) -> Vec<WindowMove> {
    compute_layout_changes_for_display(state, display_id)
}
