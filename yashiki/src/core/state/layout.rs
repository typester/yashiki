use super::super::window::Rect;
use super::super::{Window, WindowId};
use crate::macos::DisplayId;

use super::super::state::{State, WindowMove};

/// Check if two ranges overlap (exclusive end)
fn ranges_overlap(a_start: i32, a_end: i32, b_start: i32, b_end: i32) -> bool {
    a_start < b_end && b_start < a_end
}

/// Check if there's a display immediately to the right that shares vertical range
fn has_right_adjacent_display(state: &State, display_id: DisplayId, frame: &Rect) -> bool {
    let right_edge = frame.x + frame.width as i32;
    state.displays.values().any(|other| {
        if other.id == display_id {
            return false;
        }
        other.frame.x == right_edge
            && ranges_overlap(
                frame.y,
                frame.y + frame.height as i32,
                other.frame.y,
                other.frame.y + other.frame.height as i32,
            )
    })
}

/// Check if there's a display immediately to the left that shares vertical range
fn has_left_adjacent_display(state: &State, display_id: DisplayId, frame: &Rect) -> bool {
    state.displays.values().any(|other| {
        if other.id == display_id {
            return false;
        }
        other.frame.x + other.frame.width as i32 == frame.x
            && ranges_overlap(
                frame.y,
                frame.y + frame.height as i32,
                other.frame.y,
                other.frame.y + other.frame.height as i32,
            )
    })
}

/// Check if there's a display immediately below that shares horizontal range
fn has_bottom_adjacent_display(state: &State, display_id: DisplayId, frame: &Rect) -> bool {
    let bottom_edge = frame.y + frame.height as i32;
    state.displays.values().any(|other| {
        if other.id == display_id {
            return false;
        }
        other.frame.y == bottom_edge
            && ranges_overlap(
                frame.x,
                frame.x + frame.width as i32,
                other.frame.x,
                other.frame.x + other.frame.width as i32,
            )
    })
}

/// Check if there's a display immediately above that shares horizontal range
fn has_top_adjacent_display(state: &State, display_id: DisplayId, frame: &Rect) -> bool {
    state.displays.values().any(|other| {
        if other.id == display_id {
            return false;
        }
        other.frame.y + other.frame.height as i32 == frame.y
            && ranges_overlap(
                frame.x,
                frame.x + frame.width as i32,
                other.frame.x,
                other.frame.x + other.frame.width as i32,
            )
    })
}

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
/// Selects a corner where the window body won't extend into adjacent displays.
/// Priority: bottom-right → bottom-left → top-right → top-left
///
/// macOS window position is the top-left corner, and the window body extends
/// right and down from that point. To hide a window at a corner while keeping
/// 1px visible (required by macOS), we must offset by window dimensions:
/// - bottom-right: no offset needed (window extends right & down into void)
/// - bottom-left: offset x by -(window_width - 1) so window extends left
/// - top-right: offset y by -(window_height - 1) so window extends up
/// - top-left: offset both x and y
///
/// A corner is unsafe if there's an adjacent display in the direction the window extends.
pub fn compute_hide_position_for_display(
    state: &State,
    display_id: DisplayId,
    window_width: u32,
    window_height: u32,
) -> (i32, i32) {
    let Some(display) = state.displays.get(&display_id) else {
        return compute_global_hide_position(state);
    };

    let frame = &display.frame;

    // Pre-compute adjacency for each direction
    let has_right = has_right_adjacent_display(state, display_id, frame);
    let has_left = has_left_adjacent_display(state, display_id, frame);
    let has_bottom = has_bottom_adjacent_display(state, display_id, frame);
    let has_top = has_top_adjacent_display(state, display_id, frame);

    // 4 corner candidates with their safety conditions
    // (corner_x, corner_y, is_unsafe)
    // Position is calculated so that 1px of window remains visible at the corner
    let corners = [
        // bottom-right: window at (display_right - 1, display_bottom - 1)
        // Window extends right & down into void, 1px visible at bottom-right corner
        (
            frame.x + frame.width as i32 - 1,
            frame.y + frame.height as i32 - 1,
            has_right || has_bottom,
        ),
        // bottom-left: window at (display_left - window_width + 1, display_bottom - 1)
        // Window extends right (into display) but only 1px visible, rest extends left
        (
            frame.x - window_width as i32 + 1,
            frame.y + frame.height as i32 - 1,
            has_left || has_bottom,
        ),
        // top-right: window at (display_right - 1, display_top - window_height + 1)
        // Window extends down (into display) but only 1px visible, rest extends up
        (
            frame.x + frame.width as i32 - 1,
            frame.y - window_height as i32 + 1,
            has_right || has_top,
        ),
        // top-left: offset both x and y
        (
            frame.x - window_width as i32 + 1,
            frame.y - window_height as i32 + 1,
            has_left || has_top,
        ),
    ];

    // Find first safe corner
    for (x, y, is_unsafe) in corners {
        if !is_unsafe {
            return (x, y);
        }
    }

    // Fallback: use bottom-right (shouldn't normally reach here unless surrounded)
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

    // First pass: collect windows that need to be shown or hidden
    // (window_id, should_show, saved_frame for show, or (width, height) for hide)
    let mut windows_to_show: Vec<(WindowId, Rect)> = Vec::new();
    let mut windows_to_hide: Vec<(WindowId, u32, u32)> = Vec::new();

    for window in state.windows.values() {
        if window.display_id != display_id {
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
            if let Some(saved) = &window.saved_frame {
                windows_to_show.push((window.id, *saved));
            }
        } else if !should_be_visible && is_visible {
            windows_to_hide.push((window.id, window.frame.width, window.frame.height));
        }
    }

    let mut moves = Vec::new();

    // Process shows
    for (window_id, saved) in windows_to_show {
        if let Some(window) = state.windows.get_mut(&window_id) {
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
            window.saved_frame = None;
            window.frame = saved;
        }
    }

    // Process hides (compute hide position per-window)
    for (window_id, width, height) in windows_to_hide {
        let (hide_x, hide_y) = compute_hide_position_for_display(state, display_id, width, height);

        if let Some(window) = state.windows.get_mut(&window_id) {
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
