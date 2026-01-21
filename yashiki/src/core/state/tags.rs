use super::super::{Tag, WindowId};
use crate::macos::DisplayId;

use super::super::state::{State, WindowMove};
use super::layout::compute_layout_changes_for_display;

pub fn view_tags(state: &mut State, tags: u32) -> Vec<WindowMove> {
    view_tags_on_display(state, tags, state.focused_display)
}

pub fn view_tags_on_display(
    state: &mut State,
    tags: u32,
    display_id: DisplayId,
) -> Vec<WindowMove> {
    let new_visible = Tag::from_mask(tags);
    let first_tag = new_visible.first_tag().unwrap_or(1);
    let new_layout = state.resolve_layout_for_tag(first_tag as u8).to_string();
    let Some(disp) = state.displays.get_mut(&display_id) else {
        return vec![];
    };
    if disp.visible_tags == new_visible {
        return vec![];
    }
    tracing::info!(
        "View tags on display {}: {} -> {}, layout: {:?} -> {}",
        display_id,
        disp.visible_tags.mask(),
        new_visible.mask(),
        disp.current_layout,
        new_layout
    );
    disp.previous_visible_tags = disp.visible_tags;
    disp.visible_tags = new_visible;
    disp.previous_layout = disp.current_layout.take();
    disp.current_layout = Some(new_layout);
    compute_layout_changes_for_display(state, display_id)
}

pub fn toggle_tags_on_display(
    state: &mut State,
    tags: u32,
    display_id: DisplayId,
) -> Vec<WindowMove> {
    let Some(disp) = state.displays.get_mut(&display_id) else {
        return vec![];
    };
    let tag = Tag::from_mask(tags);
    let new_visible = disp.visible_tags.toggle(tag);
    if new_visible.mask() == 0 {
        return vec![];
    }
    tracing::info!(
        "Toggle tags on display {}: {} -> {}",
        display_id,
        disp.visible_tags.mask(),
        new_visible.mask()
    );
    disp.previous_visible_tags = disp.visible_tags;
    disp.visible_tags = new_visible;
    compute_layout_changes_for_display(state, display_id)
}

pub fn view_tags_last(state: &mut State) -> Vec<WindowMove> {
    let Some(disp) = state.displays.get_mut(&state.focused_display) else {
        return vec![];
    };
    if disp.visible_tags == disp.previous_visible_tags {
        return vec![];
    }
    tracing::info!(
        "View tags last on display {}: {} -> {}, layout: {:?} -> {:?}",
        state.focused_display,
        disp.visible_tags.mask(),
        disp.previous_visible_tags.mask(),
        disp.current_layout,
        disp.previous_layout
    );
    std::mem::swap(&mut disp.visible_tags, &mut disp.previous_visible_tags);
    std::mem::swap(&mut disp.current_layout, &mut disp.previous_layout);
    compute_layout_changes_for_display(state, state.focused_display)
}

pub fn move_focused_to_tags(state: &mut State, tags: u32) -> Vec<WindowMove> {
    let Some(focused_id) = state.focused else {
        return vec![];
    };
    let new_tags = Tag::from_mask(tags);
    let display_id = if let Some(window) = state.windows.get_mut(&focused_id) {
        tracing::info!("Move window {} to tags {}", window.id, new_tags.mask());
        window.tags = new_tags;
        window.display_id
    } else {
        return vec![];
    };
    compute_layout_changes_for_display(state, display_id)
}

pub fn toggle_focused_window_tags(state: &mut State, tags: u32) -> Vec<WindowMove> {
    let Some(focused_id) = state.focused else {
        return vec![];
    };
    let tag = Tag::from_mask(tags);
    let display_id = if let Some(window) = state.windows.get_mut(&focused_id) {
        let new_tags = window.tags.toggle(tag);
        if new_tags.mask() == 0 {
            return vec![];
        }
        tracing::info!(
            "Toggle window {} tags: {} -> {}",
            window.id,
            window.tags.mask(),
            new_tags.mask()
        );
        window.tags = new_tags;
        window.display_id
    } else {
        return vec![];
    };
    compute_layout_changes_for_display(state, display_id)
}

pub fn toggle_focused_fullscreen(state: &mut State) -> Option<(DisplayId, bool, WindowId, i32)> {
    let focused_id = state.focused?;
    let window = state.windows.get_mut(&focused_id)?;

    window.is_fullscreen = !window.is_fullscreen;
    tracing::info!(
        "Toggle fullscreen for window {}: {}",
        window.id,
        window.is_fullscreen
    );

    Some((
        window.display_id,
        window.is_fullscreen,
        window.id,
        window.pid,
    ))
}

pub fn toggle_focused_float(state: &mut State) -> Option<(DisplayId, bool, WindowId, i32)> {
    let focused_id = state.focused?;
    let window = state.windows.get_mut(&focused_id)?;

    window.is_floating = !window.is_floating;
    tracing::info!(
        "Toggle floating for window {}: {}",
        window.id,
        window.is_floating
    );

    Some((window.display_id, window.is_floating, window.id, window.pid))
}
