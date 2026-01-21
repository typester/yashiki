use std::cell::RefCell;
use std::collections::HashMap;

use crate::core::State;
use crate::event_emitter::EventEmitter;

/// Window properties tracked for change detection
#[derive(Clone, PartialEq)]
pub struct WindowProperties {
    tags: u32,
    display_id: u32,
    is_floating: bool,
    is_fullscreen: bool,
}

/// State captured before command execution for event comparison
pub struct PreEventState {
    /// Map of display_id to (visible_tags, current_layout)
    pub displays: HashMap<u32, (u32, Option<String>)>,
    /// Map of window_id to tracked properties
    pub windows: HashMap<u32, WindowProperties>,
    pub focused: Option<u32>,
    pub focused_display: u32,
}

/// Capture relevant state for event emission comparison
pub fn capture_event_state(state: &RefCell<State>) -> PreEventState {
    let state = state.borrow();
    let displays = state
        .displays
        .iter()
        .map(|(id, d)| (*id, (d.visible_tags.mask(), d.current_layout.clone())))
        .collect();

    let windows = state
        .windows
        .iter()
        .map(|(id, w)| {
            (
                *id,
                WindowProperties {
                    tags: w.tags.mask(),
                    display_id: w.display_id,
                    is_floating: w.is_floating,
                    is_fullscreen: w.is_fullscreen,
                },
            )
        })
        .collect();

    PreEventState {
        displays,
        windows,
        focused: state.focused,
        focused_display: state.focused_display,
    }
}

/// Emit events based on state changes
pub fn emit_state_change_events(
    event_emitter: &EventEmitter,
    state: &RefCell<State>,
    pre: &PreEventState,
) {
    let state = state.borrow();

    // Check for focus changes
    if state.focused != pre.focused {
        event_emitter.emit_window_focused(state.focused);
    }

    // Check for display focus changes
    if state.focused_display != pre.focused_display {
        event_emitter.emit_display_focused(state.focused_display);
    }

    // Check for tag and layout changes on each display
    for (display_id, display) in &state.displays {
        if let Some((pre_tags, pre_layout)) = pre.displays.get(display_id) {
            let current_tags = display.visible_tags.mask();

            // Emit tags changed event
            if current_tags != *pre_tags {
                event_emitter.emit_tags_changed(*display_id, current_tags, *pre_tags);
            }

            // Emit layout changed event
            if display.current_layout != *pre_layout {
                if let Some(ref layout) = display.current_layout {
                    event_emitter.emit_layout_changed(*display_id, layout);
                }
            }
        }
    }

    // Check for removed windows
    for window_id in pre.windows.keys() {
        if !state.windows.contains_key(window_id) {
            event_emitter.emit_window_destroyed(*window_id);
        }
    }

    // Check for window property changes
    for (window_id, window) in &state.windows {
        if let Some(pre_props) = pre.windows.get(window_id) {
            let current_props = WindowProperties {
                tags: window.tags.mask(),
                display_id: window.display_id,
                is_floating: window.is_floating,
                is_fullscreen: window.is_fullscreen,
            };

            // Emit window updated event if any tracked property changed
            if current_props != *pre_props {
                event_emitter.emit_window_updated(window, state.focused);
            }
        }
    }
}
