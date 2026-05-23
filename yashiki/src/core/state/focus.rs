use std::collections::HashSet;

use super::super::{Tag, Window, WindowId};
use crate::macos::DisplayId;
use yashiki_ipc::Direction;

use super::super::state::State;
use super::layout::{add_to_tag_orders, focusable_windows_on_display, visible_windows_on_display};

/// Pick the best focus target for a display's currently-visible tags.
///
/// Prefers the per-tag last-focused window (latest timestamp across visible
/// tag bits, validated against the current `state.windows` — must exist, be on
/// this display, still carry that tag bit, and be in the focusable set). Falls
/// back to the first window in `focusable_windows_on_display` order (which
/// follows `tag_orders`, so it matches the layout engine's preferred head —
/// e.g. tatami master).
///
/// Returns `None` if there is nothing focusable on the display.
pub fn pick_focus_target(state: &State, display_id: DisplayId) -> Option<&Window> {
    let display = state.displays.get(&display_id)?;
    let focusable = focusable_windows_on_display(state, display_id);
    if focusable.is_empty() {
        return None;
    }

    let focusable_ids: HashSet<WindowId> = focusable.iter().map(|w| w.id).collect();
    let restore = display
        .visible_tags
        .iter_bits()
        .filter_map(|bit| {
            let (id, ts) = display.last_focused_per_tag.get(&bit)?;
            let w = state.windows.get(id)?;
            let bit_mask = 1u32 << (bit - 1);
            if w.display_id != display_id
                || (w.tags.mask() & bit_mask) == 0
                || !focusable_ids.contains(id)
            {
                return None;
            }
            Some((w, *ts))
        })
        .max_by_key(|&(_, ts)| ts)
        .map(|(w, _)| w);

    restore.or_else(|| focusable.first().copied())
}

pub fn focus_window(state: &State, direction: Direction) -> Option<(WindowId, i32)> {
    let visible: Vec<&Window> = focusable_windows_on_display(state, state.focused_display);

    if visible.is_empty() {
        return None;
    }

    match direction {
        Direction::Next | Direction::Prev => {
            focus_window_stack(state, &visible, direction == Direction::Next)
        }
        Direction::Left | Direction::Right | Direction::Up | Direction::Down => {
            focus_window_directional(state, &visible, direction)
        }
    }
}

fn focus_window_stack(
    state: &State,
    visible: &[&Window],
    forward: bool,
) -> Option<(WindowId, i32)> {
    if visible.is_empty() {
        return None;
    }

    let current_idx = state
        .focused
        .and_then(|id| visible.iter().position(|w| w.id == id));

    let next_idx = match current_idx {
        Some(idx) => {
            if forward {
                (idx + 1) % visible.len()
            } else {
                (idx + visible.len() - 1) % visible.len()
            }
        }
        None => 0,
    };

    let w = visible[next_idx];
    Some((w.id, w.pid))
}

fn focus_window_directional(
    state: &State,
    visible: &[&Window],
    direction: Direction,
) -> Option<(WindowId, i32)> {
    let focused_id = state.focused?;
    let focused = visible.iter().find(|w| w.id == focused_id)?;

    let (fx, fy) = focused.center();
    let mut best: Option<(&Window, i32)> = None;

    for window in visible {
        if window.id == focused_id {
            continue;
        }

        let (wx, wy) = window.center();

        let is_candidate = match direction {
            Direction::Left => wx < fx,
            Direction::Right => wx > fx,
            Direction::Up => wy < fy,
            Direction::Down => wy > fy,
            _ => false,
        };

        if !is_candidate {
            continue;
        }

        let distance = (wx - fx).abs() + (wy - fy).abs();

        match &best {
            Some((_, best_dist)) if distance < *best_dist => {
                best = Some((window, distance));
            }
            None => {
                best = Some((window, distance));
            }
            _ => {}
        }
    }

    best.map(|(w, _)| (w.id, w.pid))
}

pub fn swap_window(state: &mut State, direction: Direction) -> Option<DisplayId> {
    let focused_id = state.focused?;
    let focused_window = state.windows.get(&focused_id)?;

    if !focused_window.is_tiled() {
        return None;
    }

    let display_id = focused_window.display_id;
    let focused_tags = focused_window.tags;
    let target_id = find_swap_target(state, direction)?;

    let target_tags = state.windows.get(&target_id)?.tags;
    let shared = Tag::from_mask(focused_tags.mask() & target_tags.mask());
    if shared.mask() == 0 {
        tracing::info!(
            "swap_window: focused {} and target {} share no tag — \
             cross-tag swap is not supported. In multi-tag visible mode, \
             use 'layout-cmd zoom' (or similar layout-engine command) to \
             rearrange across tags.",
            focused_id,
            target_id
        );
        return None;
    }

    // Auto-register stragglers: ensure both windows are present in tag_orders
    // for every shared bit before locating positions. add_to_tag_orders is
    // idempotent (pushes only if absent).
    add_to_tag_orders(state, focused_id, display_id, shared);
    add_to_tag_orders(state, target_id, display_id, shared);

    let display = state.displays.get_mut(&display_id)?;

    for tag_bit in shared.iter_bits() {
        let Some(order) = display.tag_orders.get_mut(&tag_bit) else {
            continue;
        };
        let focused_idx = order.iter().position(|&id| id == focused_id);
        let target_idx = order.iter().position(|&id| id == target_id);
        if let (Some(fi), Some(ti)) = (focused_idx, target_idx) {
            order.swap(fi, ti);
        }
    }
    tracing::info!(
        "Swapped window {} with {} in direction {:?} (shared tags mask={:#b})",
        focused_id,
        target_id,
        direction,
        shared.mask()
    );
    Some(display_id)
}

fn find_swap_target(state: &State, direction: Direction) -> Option<WindowId> {
    let visible: Vec<&Window> = visible_windows_on_display(state, state.focused_display);

    if visible.len() <= 1 {
        return None;
    }

    match direction {
        Direction::Next | Direction::Prev => {
            find_swap_target_stack(state, &visible, direction == Direction::Next)
        }
        Direction::Left | Direction::Right | Direction::Up | Direction::Down => {
            find_swap_target_directional(state, &visible, direction)
        }
    }
}

fn find_swap_target_stack(state: &State, visible: &[&Window], forward: bool) -> Option<WindowId> {
    let focused_id = state.focused?;
    let current_idx = visible.iter().position(|w| w.id == focused_id)?;

    let next_idx = if forward {
        (current_idx + 1) % visible.len()
    } else {
        (current_idx + visible.len() - 1) % visible.len()
    };

    Some(visible[next_idx].id)
}

fn find_swap_target_directional(
    state: &State,
    visible: &[&Window],
    direction: Direction,
) -> Option<WindowId> {
    let focused_id = state.focused?;
    let focused = visible.iter().find(|w| w.id == focused_id)?;

    let (fx, fy) = focused.center();
    let mut best: Option<(WindowId, i32)> = None;

    for window in visible {
        if window.id == focused_id {
            continue;
        }

        let (wx, wy) = window.center();

        let is_candidate = match direction {
            Direction::Left => wx < fx,
            Direction::Right => wx > fx,
            Direction::Up => wy < fy,
            Direction::Down => wy > fy,
            _ => false,
        };

        if !is_candidate {
            continue;
        }

        let distance = (wx - fx).abs() + (wy - fy).abs();

        match &best {
            Some((_, best_dist)) if distance < *best_dist => {
                best = Some((window.id, distance));
            }
            None => {
                best = Some((window.id, distance));
            }
            _ => {}
        }
    }

    best.map(|(id, _)| id)
}
