use std::collections::{HashMap, HashSet};

use super::super::WindowId;
use crate::macos::DisplayId;
use crate::platform::WindowSystem;
use yashiki_ipc::OutputDirection;

use super::super::state::{DisplayChangeResult, FocusOutputResult, State};
use super::layout::{compute_layout_changes_for_display, visible_windows_on_display};
use super::sync::sync_all;

pub fn handle_display_change<W: WindowSystem>(state: &mut State, ws: &W) -> DisplayChangeResult {
    let display_infos = ws.get_all_displays();
    let current_ids: HashSet<_> = display_infos.iter().map(|d| d.id).collect();
    let previous_ids: HashSet<_> = state.displays.keys().copied().collect();

    let removed_ids: Vec<_> = previous_ids.difference(&current_ids).copied().collect();
    let added_ids: HashSet<_> = current_ids.difference(&previous_ids).copied().collect();

    if removed_ids.is_empty() {
        let visible_window_displays: HashMap<WindowId, DisplayId> = state
            .windows
            .iter()
            .filter(|(_, w)| !w.is_hidden())
            .map(|(id, w)| (*id, w.display_id))
            .collect();

        sync_all(state, ws);

        let mut stolen_window_displays: HashSet<DisplayId> = HashSet::new();
        for (window_id, original_display_id) in &visible_window_displays {
            if let Some(window) = state.windows.get_mut(window_id) {
                if !added_ids.contains(original_display_id)
                    && added_ids.contains(&window.display_id)
                {
                    tracing::info!(
                        "Restoring window {} to original display {} (macOS moved it to new display {})",
                        window_id,
                        original_display_id,
                        window.display_id
                    );
                    window.display_id = *original_display_id;
                    stolen_window_displays.insert(*original_display_id);
                }
            }
        }

        let added: Vec<_> = state
            .displays
            .values()
            .filter(|d| added_ids.contains(&d.id))
            .cloned()
            .collect();

        let mut displays_to_retile: Vec<_> = added_ids.iter().copied().collect();
        displays_to_retile.extend(stolen_window_displays);

        return DisplayChangeResult {
            window_moves: vec![],
            displays_to_retile,
            added,
            removed: vec![],
        };
    }

    tracing::info!("Displays disconnected: {:?}", removed_ids);

    let fallback_display = display_infos
        .iter()
        .find(|d| d.is_main)
        .or_else(|| display_infos.first())
        .map(|d| d.id);

    let Some(fallback_id) = fallback_display else {
        tracing::warn!("No fallback display available");
        return DisplayChangeResult {
            window_moves: vec![],
            displays_to_retile: vec![],
            added: vec![],
            removed: removed_ids,
        };
    };

    let mut window_moves = Vec::new();
    let mut affected_displays = HashSet::new();

    for window in state.windows.values_mut() {
        if removed_ids.contains(&window.display_id) {
            tracing::info!(
                "Moving orphaned window {} ({}) from display {} to {}",
                window.id,
                window.app_name,
                window.display_id,
                fallback_id
            );
            window.display_id = fallback_id;
            affected_displays.insert(fallback_id);
        }
    }

    if removed_ids.contains(&state.focused_display) {
        tracing::info!(
            "Focused display {} was removed, switching to {}",
            state.focused_display,
            fallback_id
        );
        state.focused_display = fallback_id;
    }

    for id in &removed_ids {
        state.displays.remove(id);
    }

    sync_all(state, ws);

    let added: Vec<_> = state
        .displays
        .values()
        .filter(|d| added_ids.contains(&d.id))
        .cloned()
        .collect();

    for display_id in &affected_displays {
        let moves = compute_layout_changes_for_display(state, *display_id);
        window_moves.extend(moves);
    }

    let displays_to_retile: Vec<_> = affected_displays.into_iter().collect();

    DisplayChangeResult {
        window_moves,
        displays_to_retile,
        added,
        removed: removed_ids,
    }
}

pub fn focus_output(state: &mut State, direction: OutputDirection) -> Option<FocusOutputResult> {
    if state.displays.len() <= 1 {
        return None;
    }

    let mut display_ids: Vec<_> = state.displays.keys().copied().collect();
    display_ids.sort();

    let current_idx = display_ids
        .iter()
        .position(|&id| id == state.focused_display)?;

    let next_idx = match direction {
        OutputDirection::Next => (current_idx + 1) % display_ids.len(),
        OutputDirection::Prev => (current_idx + display_ids.len() - 1) % display_ids.len(),
    };

    let target_display_id = display_ids[next_idx];
    tracing::info!(
        "Focus output: {} -> {} ({:?})",
        state.focused_display,
        target_display_id,
        direction
    );
    state.focused_display = target_display_id;

    let visible = visible_windows_on_display(state, target_display_id);
    if let Some(w) = visible.first() {
        Some(FocusOutputResult::Window {
            window_id: w.id,
            pid: w.pid,
        })
    } else {
        Some(FocusOutputResult::EmptyDisplay {
            display_id: target_display_id,
        })
    }
}

pub fn send_to_output(
    state: &mut State,
    direction: OutputDirection,
) -> Option<(DisplayId, DisplayId)> {
    let focused_id = state.focused?;

    if state.displays.len() <= 1 {
        return None;
    }

    let mut display_ids: Vec<_> = state.displays.keys().copied().collect();
    display_ids.sort();

    let source_display_id = state.windows.get(&focused_id)?.display_id;
    let current_idx = display_ids.iter().position(|&id| id == source_display_id)?;

    let next_idx = match direction {
        OutputDirection::Next => (current_idx + 1) % display_ids.len(),
        OutputDirection::Prev => (current_idx + display_ids.len() - 1) % display_ids.len(),
    };

    let target_display_id = display_ids[next_idx];

    if source_display_id == target_display_id {
        return None;
    }

    let target_display = state.displays.get(&target_display_id)?;
    let target_x = target_display.frame.x;
    let target_y = target_display.frame.y;

    let window = state.windows.get_mut(&focused_id)?;
    tracing::info!(
        "Send window {} to output: {} -> {}",
        window.id,
        source_display_id,
        target_display_id
    );
    window.display_id = target_display_id;
    window.frame.x = target_x;
    window.frame.y = target_y;

    state.focused_display = target_display_id;

    Some((source_display_id, target_display_id))
}
