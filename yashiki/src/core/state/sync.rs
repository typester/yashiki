use std::collections::HashSet;

use super::super::{Display, Rect, Window, WindowId};
use crate::macos::DisplayId;
use crate::platform::WindowSystem;

use super::super::state::{State, WindowMove};
use super::layout::{
    add_to_window_order, compute_hide_position_for_display, remove_from_window_order,
};
use super::rules::{has_matching_non_ignore_rule, should_ignore_window_extended};

/// Check if a hidden window needs to be re-hidden (returns Some if moved from hide position)
fn check_window_rehide(
    window: &Window,
    current_x: i32,
    current_y: i32,
    hide_x: i32,
    hide_y: i32,
) -> Option<WindowMove> {
    if !window.is_hidden() {
        return None;
    }
    if current_x != hide_x || current_y != hide_y {
        tracing::debug!(
            "Re-hiding window {} (macOS moved it from hide position)",
            window.id
        );
        Some(WindowMove {
            window_id: window.id,
            pid: window.pid,
            old_x: current_x,
            old_y: current_y,
            new_x: hide_x,
            new_y: hide_y,
        })
    } else {
        None
    }
}

/// Detect hidden windows that macOS moved from the hide position.
fn detect_rehide_moves(
    state: &State,
    window_infos: &[crate::macos::WindowInfo],
) -> Vec<WindowMove> {
    let mut rehide_moves = Vec::new();

    for window in state.windows.values() {
        if let Some(info) = window_infos.iter().find(|i| i.window_id == window.id) {
            let (hide_x, hide_y) = compute_hide_position_for_display(state, window.display_id);
            if let Some(mv) = check_window_rehide(
                window,
                info.bounds.x as i32,
                info.bounds.y as i32,
                hide_x,
                hide_y,
            ) {
                rehide_moves.push(mv);
            }
        }
    }
    rehide_moves
}

pub fn sync_all<W: WindowSystem>(state: &mut State, ws: &W) -> Vec<WindowMove> {
    let display_infos = ws.get_all_displays();
    for info in &display_infos {
        state
            .displays
            .entry(info.id)
            .and_modify(|display| {
                display.name = info.name.clone();
                display.frame = Rect::from_bounds(&info.frame);
                display.is_main = info.is_main;
            })
            .or_insert_with(|| {
                Display::new(
                    info.id,
                    info.name.clone(),
                    Rect::from_bounds(&info.frame),
                    info.is_main,
                )
            });
        if info.is_main && state.focused_display == 0 {
            state.focused_display = info.id;
        }
    }

    let current_ids: HashSet<_> = display_infos.iter().map(|d| d.id).collect();
    state.displays.retain(|id, _| current_ids.contains(id));

    let window_infos = ws.get_on_screen_windows();
    let rehide_moves = sync_with_window_infos(state, ws, &window_infos);
    sync_focused_window(state, ws);

    tracing::info!(
        "State initialized with {} displays, {} windows",
        state.displays.len(),
        state.windows.len()
    );
    for d in state.displays.values() {
        tracing::debug!("  Display {}: {:?}", d.id, d.frame);
    }
    for window in state.windows.values() {
        tracing::debug!(
            "  - [{}] {} ({}) on display {}",
            window.id,
            window.title,
            window.app_name,
            window.display_id
        );
    }

    rehide_moves
}

pub fn sync_focused_window<W: WindowSystem>(state: &mut State, ws: &W) -> (bool, Vec<WindowId>) {
    sync_focused_window_with_hint(state, ws, None)
}

pub fn sync_focused_window_with_hint<W: WindowSystem>(
    state: &mut State,
    ws: &W,
    pid_hint: Option<i32>,
) -> (bool, Vec<WindowId>) {
    if let Some(focused_info) = ws.get_focused_window() {
        let window_id = focused_info.window_id;

        // Window exists - just update focus
        if let Some(window) = state.windows.get(&window_id) {
            let display_id = window.display_id;
            state.set_focused(Some(window_id));
            if state.focused_display != display_id {
                tracing::info!(
                    "Focused display changed: {} -> {}",
                    state.focused_display,
                    display_id
                );
                state.focused_display = display_id;
            }
            return (false, vec![]);
        }

        // Window not in state - look up pid from system windows and sync
        let window_infos = ws.get_on_screen_windows();
        if let Some(info) = window_infos.iter().find(|w| w.window_id == window_id) {
            let pid = info.pid;
            tracing::info!(
                "Focused window {} not in state, syncing pid {}",
                window_id,
                pid
            );

            let (changed, new_ids, _) = sync_pid(state, ws, pid);

            // Set focus on the new window if it was added
            if let Some(window) = state.windows.get(&window_id) {
                let display_id = window.display_id;
                state.set_focused(Some(window_id));
                if state.focused_display != display_id {
                    state.focused_display = display_id;
                }
            }

            return (changed, new_ids);
        }
    }

    // Fallback: use pid_hint
    if let Some(pid) = pid_hint {
        let visible_tags = state.visible_tags();
        let pid_windows: Vec<_> = state
            .windows
            .values()
            .filter(|w| w.pid == pid && w.tags.intersects(visible_tags) && !w.is_hidden())
            .collect();

        if let Some(window) = pid_windows.first() {
            tracing::debug!(
                "Focus fallback: using window {} for pid {} (accessibility API unavailable)",
                window.id,
                pid
            );
            let window_id = window.id;
            let display_id = window.display_id;
            state.set_focused(Some(window_id));
            if state.focused_display != display_id {
                state.focused_display = display_id;
            }
            return (false, vec![]);
        }
    }

    state.set_focused(None);
    (false, vec![])
}

pub fn sync_pid<W: WindowSystem>(
    state: &mut State,
    ws: &W,
    pid: i32,
) -> (bool, Vec<WindowId>, Vec<WindowMove>) {
    let window_infos = ws.get_on_screen_windows();
    let pid_window_infos: Vec<_> = window_infos.iter().filter(|w| w.pid == pid).collect();

    let current_ids: HashSet<WindowId> = state
        .windows
        .values()
        .filter(|w| w.pid == pid)
        .map(|w| w.id)
        .collect();
    let new_ids: HashSet<WindowId> = pid_window_infos.iter().map(|w| w.window_id).collect();

    let mut changed = false;
    let mut added_window_ids = Vec::new();
    let mut rehide_moves = Vec::new();

    for id in current_ids.difference(&new_ids) {
        if let Some(window) = state.windows.remove(id) {
            tracing::info!(
                "Window removed: [{}] {} ({})",
                window.id,
                window.title,
                window.app_name
            );
            if state.focused == Some(*id) {
                state.focused = None;
            }
            changed = true;
        }
    }

    for id in new_ids.difference(&current_ids) {
        if let Some(info) = pid_window_infos.iter().find(|w| w.window_id == *id) {
            let display_id = find_display_for_bounds(state, &info.bounds);

            if let Some(window) = try_create_window(state, ws, info, display_id) {
                tracing::info!(
                    "Window added: [{}] {} ({}) on display {} [ax_id={:?}, subrole={:?}, level={}]",
                    window.id,
                    window.title,
                    window.app_name,
                    display_id,
                    window.ax_id,
                    window.subrole,
                    window.window_level
                );
                state.windows.insert(window.id, window);
                added_window_ids.push(*id);
                changed = true;
            }
        }
    }

    for id in current_ids.intersection(&new_ids) {
        if let Some(info) = pid_window_infos.iter().find(|w| w.window_id == *id) {
            let ext = ws.get_extended_attributes(info.window_id, info.pid, info.layer);
            let new_title = ext
                .title
                .clone()
                .unwrap_or_else(|| info.name.clone().unwrap_or_default());
            let new_frame = Rect::from_bounds(&info.bounds);
            let new_display_id = find_display_for_bounds(state, &info.bounds);

            // Compute hide position before mutable borrow
            let hide_pos = state
                .windows
                .get(id)
                .map(|w| compute_hide_position_for_display(state, w.display_id));

            if let Some(window) = state.windows.get_mut(id) {
                let title_changed = window.title != new_title;
                let frame_changed = window.frame.x != new_frame.x
                    || window.frame.y != new_frame.y
                    || window.frame.width != new_frame.width
                    || window.frame.height != new_frame.height;

                if title_changed || frame_changed {
                    tracing::debug!(
                        "Window updated: [{}] {} ({}) pos=({},{}) -> ({},{})",
                        window.id,
                        window.title,
                        window.app_name,
                        window.frame.x,
                        window.frame.y,
                        new_frame.x,
                        new_frame.y
                    );
                    window.title = new_title;

                    if let Some((hide_x, hide_y)) = hide_pos {
                        if let Some(mv) =
                            check_window_rehide(window, new_frame.x, new_frame.y, hide_x, hide_y)
                        {
                            rehide_moves.push(mv);
                        } else if !window.is_hidden() {
                            window.frame = new_frame;
                            window.display_id = new_display_id;
                        }
                    } else if !window.is_hidden() {
                        window.frame = new_frame;
                        window.display_id = new_display_id;
                    }
                }
            }
        }
    }

    (changed, added_window_ids, rehide_moves)
}

pub fn find_display_for_bounds(state: &State, bounds: &crate::macos::Bounds) -> DisplayId {
    let cx = bounds.x + bounds.width / 2.0;
    let cy = bounds.y + bounds.height / 2.0;

    for display in state.displays.values() {
        let dx = display.frame.x as f64;
        let dy = display.frame.y as f64;
        let dw = display.frame.width as f64;
        let dh = display.frame.height as f64;

        if cx >= dx && cx < dx + dw && cy >= dy && cy < dy + dh {
            return display.id;
        }
    }

    if state.focused_display != 0 {
        state.focused_display
    } else {
        state.displays.keys().next().copied().unwrap_or(0)
    }
}

pub fn try_create_window<W: WindowSystem>(
    state: &State,
    ws: &W,
    info: &crate::macos::WindowInfo,
    display_id: DisplayId,
) -> Option<Window> {
    let app_name = &info.owner_name;
    let app_id = info.bundle_id.as_deref();

    // Filter Control Center early - system UI that users never need to manage,
    // and it creates many transient windows that slow down processing
    if app_id == Some("com.apple.controlcenter") {
        return None;
    }

    let ext = ws.get_extended_attributes(info.window_id, info.pid, info.layer);

    let title = ext
        .title
        .clone()
        .unwrap_or_else(|| info.name.clone().unwrap_or_default());

    tracing::trace!(
        "Discovered window: [{}] pid={} app='{}' app_id={:?} title='{}' \
         ax_id={:?} subrole={:?} layer={} close={:?} fullscreen={:?} \
         minimize={:?} zoom={:?}",
        info.window_id,
        info.pid,
        app_name,
        app_id,
        title,
        ext.ax_id,
        ext.subrole,
        ext.window_level,
        ext.close_button,
        ext.fullscreen_button,
        ext.minimize_button,
        ext.zoom_button
    );

    if ext.window_level != 0 && !has_matching_non_ignore_rule(state, app_name, app_id, &title, &ext)
    {
        tracing::debug!(
            "Window skipped (non-normal layer without matching rule): [{}] {} ({}) level={}",
            info.window_id,
            title,
            app_name,
            ext.window_level
        );
        return None;
    }

    if should_ignore_window_extended(state, app_name, app_id, &title, &ext) {
        tracing::info!(
            "Window ignored by rule: [{}] {} ({}) [ax_id={:?}, subrole={:?}, level={}]",
            info.window_id,
            title,
            app_name,
            ext.ax_id,
            ext.subrole,
            ext.window_level
        );
        return None;
    }

    let initial_tag = state
        .displays
        .get(&display_id)
        .map(|d| d.visible_tags)
        .unwrap_or(state.default_tag);

    let mut window = Window::from_window_info(info, initial_tag, display_id);
    window.title = title;
    window.ax_id = ext.ax_id;
    window.subrole = ext.subrole;
    window.window_level = ext.window_level;
    window.close_button = ext.close_button;
    window.fullscreen_button = ext.fullscreen_button;
    window.minimize_button = ext.minimize_button;
    window.zoom_button = ext.zoom_button;

    Some(window)
}

pub fn sync_with_window_infos<W: WindowSystem>(
    state: &mut State,
    ws: &W,
    window_infos: &[crate::macos::WindowInfo],
) -> Vec<WindowMove> {
    let current_ids: HashSet<WindowId> = state.windows.keys().copied().collect();
    let new_ids: HashSet<WindowId> = window_infos.iter().map(|w| w.window_id).collect();

    for id in current_ids.difference(&new_ids) {
        remove_from_window_order(state, *id);
        state.windows.remove(id);
    }

    for info in window_infos {
        if !state.windows.contains_key(&info.window_id) {
            let display_id = find_display_for_bounds(state, &info.bounds);

            if let Some(window) = try_create_window(state, ws, info, display_id) {
                add_to_window_order(state, window.id, display_id);
                state.windows.insert(window.id, window);
            }
        }
    }

    for info in window_infos {
        let new_display_id = find_display_for_bounds(state, &info.bounds);
        if let Some(window) = state.windows.get_mut(&info.window_id) {
            let ext = ws.get_extended_attributes(info.window_id, info.pid, info.layer);
            let new_title = ext
                .title
                .clone()
                .unwrap_or_else(|| info.name.clone().unwrap_or_default());
            window.title = new_title;
            if !window.is_hidden() {
                window.frame = Rect::from_bounds(&info.bounds);
                window.display_id = new_display_id;
            }
        }
    }

    detect_rehide_moves(state, window_infos)
}
