use std::cell::RefCell;

use crate::core::State;
use crate::layout::LayoutEngineManager;
use crate::macos::DisplayId;
use crate::platform::WindowManipulator;

pub fn do_retile<M: WindowManipulator>(
    state: &RefCell<State>,
    layout_engine_manager: &RefCell<LayoutEngineManager>,
    manipulator: &M,
) {
    // Collect display IDs first to avoid borrow issues
    let display_ids: Vec<_> = state.borrow().displays.keys().copied().collect();

    for display_id in display_ids {
        retile_single_display(state, layout_engine_manager, manipulator, display_id);
    }
}

pub fn do_retile_display<M: WindowManipulator>(
    state: &RefCell<State>,
    layout_engine_manager: &RefCell<LayoutEngineManager>,
    manipulator: &M,
    display_id: DisplayId,
) {
    if !state.borrow().displays.contains_key(&display_id) {
        return;
    }
    retile_single_display(state, layout_engine_manager, manipulator, display_id);
}

fn retile_single_display<M: WindowManipulator>(
    state: &RefCell<State>,
    layout_engine_manager: &RefCell<LayoutEngineManager>,
    manipulator: &M,
    display_id: DisplayId,
) {
    // First, handle any fullscreen windows on this display
    {
        let state = state.borrow();
        let outer_gap = state.config.outer_gap;
        if let Some(display) = state.displays.get(&display_id) {
            let fullscreen_windows: Vec<_> = state
                .windows
                .values()
                .filter(|w| {
                    w.display_id == display_id
                        && w.is_fullscreen
                        && w.tags.intersects(display.visible_tags)
                        && !w.is_hidden()
                })
                .map(|w| (w.id, w.pid))
                .collect();

            // Apply fullscreen with outer gap
            for (window_id, pid) in fullscreen_windows {
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
    }

    // Get layout parameters with immutable borrow
    let (window_ids, usable_width, usable_height, display_frame, layout_name, outer_gap) = {
        let state = state.borrow();
        let Some(display) = state.displays.get(&display_id) else {
            return;
        };
        let visible_windows = state.visible_windows_on_display(display_id);
        if visible_windows.is_empty() {
            return;
        }
        let window_ids: Vec<u32> = visible_windows.iter().map(|w| w.id).collect();
        let layout_name = state.current_layout_for_display(display_id).to_string();
        let outer_gap = state.config.outer_gap;
        // Subtract outer gap from dimensions before sending to layout engine
        let usable_width = display.frame.width.saturating_sub(outer_gap.horizontal());
        let usable_height = display.frame.height.saturating_sub(outer_gap.vertical());
        (
            window_ids,
            usable_width,
            usable_height,
            display.frame,
            layout_name,
            outer_gap,
        )
    };

    let mut manager = layout_engine_manager.borrow_mut();
    match manager.request_layout(&layout_name, usable_width, usable_height, &window_ids) {
        Ok(geometries) => {
            // Update window_order based on geometries order from layout engine
            {
                let mut state = state.borrow_mut();
                if let Some(display) = state.displays.get_mut(&display_id) {
                    display.window_order = geometries.iter().map(|g| g.id).collect();
                }
            }
            // Add outer gap offset to geometries before applying
            let adjusted_geometries: Vec<_> = geometries
                .into_iter()
                .map(|mut g| {
                    g.x += outer_gap.left as i32;
                    g.y += outer_gap.top as i32;
                    g
                })
                .collect();
            // Apply layout using manipulator
            manipulator.apply_layout(display_id, &display_frame, &adjusted_geometries);
        }
        Err(e) => {
            tracing::error!("Layout request failed for display {}: {}", display_id, e);
        }
    }
}
