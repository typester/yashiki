use super::super::{Window, WindowId};
use crate::macos::DisplayId;
use yashiki_ipc::Direction;

use super::super::state::State;

pub fn focus_window(state: &State, direction: Direction) -> Option<(WindowId, i32)> {
    let visible_tags = state.visible_tags();
    let visible: Vec<_> = state
        .windows
        .values()
        .filter(|w| {
            w.display_id == state.focused_display
                && w.tags.intersects(visible_tags)
                && !w.is_hidden()
        })
        .collect();

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

    let mut sorted: Vec<_> = visible.iter().map(|w| (w.id, w.pid)).collect();
    sorted.sort_by_key(|(id, _)| *id);

    let current_idx = state
        .focused
        .and_then(|id| sorted.iter().position(|(wid, _)| *wid == id));

    let next_idx = match current_idx {
        Some(idx) => {
            if forward {
                (idx + 1) % sorted.len()
            } else {
                (idx + sorted.len() - 1) % sorted.len()
            }
        }
        None => 0,
    };

    Some(sorted[next_idx])
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
    let target_id = find_swap_target(state, direction)?;

    if let Some(display) = state.displays.get_mut(&display_id) {
        let focused_idx = display
            .window_order
            .iter()
            .position(|&id| id == focused_id)?;
        let target_idx = display
            .window_order
            .iter()
            .position(|&id| id == target_id)?;
        display.window_order.swap(focused_idx, target_idx);
        tracing::info!(
            "Swapped window {} with {} in direction {:?}",
            focused_id,
            target_id,
            direction
        );
        Some(display_id)
    } else {
        None
    }
}

fn find_swap_target(state: &State, direction: Direction) -> Option<WindowId> {
    let visible_tags = state.visible_tags();
    let visible: Vec<_> = state
        .windows
        .values()
        .filter(|w| {
            w.display_id == state.focused_display
                && w.tags.intersects(visible_tags)
                && !w.is_hidden()
                && w.is_tiled()
        })
        .collect();

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
    let display = state.displays.get(&state.focused_display)?;

    let mut sorted: Vec<_> = visible.iter().map(|w| w.id).collect();
    sorted.sort_by_key(|&id| {
        display
            .window_order
            .iter()
            .position(|&wid| wid == id)
            .unwrap_or(usize::MAX)
    });

    let current_idx = sorted.iter().position(|&id| id == focused_id)?;

    let next_idx = if forward {
        (current_idx + 1) % sorted.len()
    } else {
        (current_idx + sorted.len() - 1) % sorted.len()
    };

    Some(sorted[next_idx])
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
