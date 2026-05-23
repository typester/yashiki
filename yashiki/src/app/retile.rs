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
    let fullscreen_data = {
        let state = state.borrow();
        let outer_gap = state.config.outer_gap;
        let Some(display) = state.displays.get(&display_id) else {
            return;
        };
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

        if fullscreen_windows.is_empty() {
            None
        } else {
            let frame_x = display.frame.x + outer_gap.left as i32;
            let frame_y = display.frame.y + outer_gap.top as i32;
            let frame_width = display.frame.width.saturating_sub(outer_gap.horizontal());
            let frame_height = display.frame.height.saturating_sub(outer_gap.vertical());
            Some((
                fullscreen_windows,
                frame_x,
                frame_y,
                frame_width,
                frame_height,
            ))
        }
    };

    if let Some((fullscreen_windows, frame_x, frame_y, frame_width, frame_height)) = fullscreen_data
    {
        // Apply fullscreen with outer gap
        for (window_id, pid) in &fullscreen_windows {
            manipulator.set_window_frame(
                *window_id,
                *pid,
                frame_x,
                frame_y,
                frame_width,
                frame_height,
            );
        }

        // Update State window frames for fullscreen windows
        let mut state_mut = state.borrow_mut();
        for (window_id, _) in fullscreen_windows {
            if let Some(window) = state_mut.windows.get_mut(&window_id) {
                let frame = crate::core::Rect {
                    x: frame_x,
                    y: frame_y,
                    width: frame_width,
                    height: frame_height,
                };
                window.update_frame_if_visible(frame);
            }
            state_mut.record_frame_write(window_id);
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

            // Update State window frames to match applied layout
            {
                let mut state = state.borrow_mut();
                for geom in &adjusted_geometries {
                    if let Some(window) = state.windows.get_mut(&geom.id) {
                        let frame = crate::core::Rect {
                            x: display_frame.x + geom.x,
                            y: display_frame.y + geom.y,
                            width: geom.width,
                            height: geom.height,
                        };
                        window.update_frame_if_visible(frame);
                    }
                    state.record_frame_write(geom.id);
                }
            }
        }
        Err(e) => {
            tracing::error!("Layout request failed for display {}: {}", display_id, e);
        }
    }
}
