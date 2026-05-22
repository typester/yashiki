use std::cell::RefCell;

use crate::core::{State, WindowMove};
use crate::layout::LayoutEngineManager;
use crate::platform::WindowManipulator;
use yashiki_ipc::CursorWarpMode;

pub fn focus_visible_window_if_needed<M: WindowManipulator>(
    state: &RefCell<State>,
    manipulator: &M,
) {
    let (window_to_focus, cursor_warp_mode) = {
        let state = state.borrow();
        let display_id = state.focused_display;
        if !state.displays.contains_key(&display_id) {
            return;
        }

        let all_focusable = state.focusable_windows_on_display(display_id);
        if all_focusable.is_empty() {
            return;
        }

        // Skip if the current focused window is already focusable.
        let focus_is_visible = state
            .focused
            .map(|id| all_focusable.iter().any(|w| w.id == id))
            .unwrap_or(false);
        if focus_is_visible {
            return;
        }

        // Pick last-focused on this display, or the first focusable as fallback.
        let Some(w) = state.pick_focus_target(display_id) else {
            return;
        };
        (
            Some((w.id, w.pid, w.display_id, w.center())),
            state.config.cursor_warp,
        )
    };

    if let Some((window_id, pid, display_id, (cx, cy))) = window_to_focus {
        tracing::info!("Focusing visible window {} after tag switch", window_id);

        // Set focus intent BEFORE focusing to suppress spurious macOS focus changes
        state
            .borrow_mut()
            .set_focus_intent(window_id, pid, display_id);

        manipulator.focus_window(window_id, pid);

        // Update internal state immediately after focusing
        // This ensures emit_state_change_events will detect the focus change
        state.borrow_mut().set_focused(Some(window_id));

        // Warp cursor if OnFocusChange mode (not OnOutputChange since this is not an output change)
        if cursor_warp_mode == CursorWarpMode::OnFocusChange {
            manipulator.warp_cursor(cx, cy);
        }
    }
}

pub fn switch_tag_for_focused_window(state: &RefCell<State>) -> Option<Vec<WindowMove>> {
    let (focused_id, window_tags, window_display_id, is_hidden) = {
        let s = state.borrow();
        let focused_id = s.focused?;
        let window = s.windows.get(&focused_id)?;
        (
            focused_id,
            window.tags,
            window.display_id,
            window.is_hidden(),
        )
    };

    // Check for cross-display spurious focus redirect
    // This happens when yashiki focuses a window on one display and macOS redirects
    // focus to a different window of the same app on another display
    if state
        .borrow()
        .should_suppress_cross_display_tag_switch(focused_id)
    {
        tracing::info!(
            "Skipping tag switch for window {} - detected cross-display focus redirect",
            focused_id
        );
        let mut s = state.borrow_mut();
        if let Some(intent) = &s.focus_intent {
            let intended_display = intent.display_id;
            if s.focused_display != intended_display {
                tracing::debug!(
                    "Reverting focused_display: {} -> {}",
                    s.focused_display,
                    intended_display
                );
                s.focused_display = intended_display;
            }
        }
        return None;
    }

    // Check if window is visible on its display's current visible tags
    let is_visible = {
        let s = state.borrow();
        if let Some(display) = s.displays.get(&window_display_id) {
            window_tags.intersects(display.visible_tags) && !is_hidden
        } else {
            false
        }
    };

    if is_visible {
        return None;
    }

    // Window is hidden, switch to its tag
    let tag = window_tags.first_tag()?;
    tracing::info!(
        "Switching to tag {} for focused window {} (external focus change)",
        tag,
        focused_id
    );

    let moves = state.borrow_mut().view_tags(1 << (tag - 1));
    // Note: Retiling is handled by the caller after applying moves
    Some(moves)
}

/// Notify layout engine of focus change.
/// Returns true if the layout engine requests a retile.
pub fn notify_layout_focus(
    state: &RefCell<State>,
    layout_engine_manager: &RefCell<LayoutEngineManager>,
    window_id: u32,
) -> bool {
    let layout_name = state.borrow().current_layout().to_string();
    let mut manager = layout_engine_manager.borrow_mut();
    match manager.send_command(&layout_name, "focus-changed", &[window_id.to_string()]) {
        Ok(needs_retile) => needs_retile,
        Err(e) => {
            tracing::warn!("Failed to notify layout engine of focus change: {}", e);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Tag;
    use crate::platform::mock::{
        create_test_display, create_test_window, MockWindowManipulator, MockWindowSystem,
    };
    use std::thread;
    use std::time::Duration;

    fn setup_three_windows() -> (RefCell<State>, MockWindowManipulator) {
        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![
                create_test_window(100, 1000, "A", 0.0, 0.0, 960.0, 1080.0),
                create_test_window(101, 1001, "B", 960.0, 0.0, 480.0, 1080.0),
                create_test_window(102, 1002, "C", 1440.0, 0.0, 480.0, 1080.0),
            ])
            .with_focused(Some(100));
        let mut state = State::new();
        state.sync_all(&ws);
        (RefCell::new(state), MockWindowManipulator::new())
    }

    #[test]
    fn test_restore_focus_uses_last_focused_per_tag() {
        // Focus 101 on tag 1, switch away, switch back — 101 must be restored.
        let (state, manipulator) = setup_three_windows();

        // Record on tag 1: focus 101 (already on tag 1 by default)
        state.borrow_mut().set_focused(Some(101));

        // Switch away from tag 1 (clear focus to force fallback path)
        state
            .borrow_mut()
            .displays
            .get_mut(&1)
            .unwrap()
            .visible_tags = Tag::from_mask(0b0010);
        // No window has tag 2 yet → focus_visible_window_if_needed returns early
        // (all_visible empty), so no change. Move one window to tag 2 to make it non-empty.
        state.borrow_mut().windows.get_mut(&100).unwrap().tags = Tag::from_mask(0b0010);
        state.borrow_mut().set_focused(Some(100));
        // Now switch back to tag 1
        state
            .borrow_mut()
            .displays
            .get_mut(&1)
            .unwrap()
            .visible_tags = Tag::from_mask(0b0001);
        // Move 100 back so it doesn't show on tag 1
        state.borrow_mut().windows.get_mut(&100).unwrap().tags = Tag::from_mask(0b0010);
        // Current focused is 100, which is NOT on tag 1 visible → restoration logic engages
        focus_visible_window_if_needed(&state, &manipulator);

        assert_eq!(state.borrow().focused, Some(101));
    }

    #[test]
    fn test_restore_focus_multi_bit_visible_picks_latest_timestamp() {
        // Focus distinct windows on tag 1 and tag 2 with a measurable time gap.
        // When both tags are visible together, the latest-timestamp window wins.
        let (state, manipulator) = setup_three_windows();

        // Set up: 101 on tag 1, 102 on tag 2
        state.borrow_mut().windows.get_mut(&101).unwrap().tags = Tag::from_mask(0b0001);
        state.borrow_mut().windows.get_mut(&102).unwrap().tags = Tag::from_mask(0b0010);
        // Window 100 won't be visible on tags 1+2 only (it's on tag 1 by default, leave it)
        state.borrow_mut().windows.get_mut(&100).unwrap().tags = Tag::from_mask(0b1000);

        // Focus 101 on tag 1
        state
            .borrow_mut()
            .displays
            .get_mut(&1)
            .unwrap()
            .visible_tags = Tag::from_mask(0b0001);
        state.borrow_mut().set_focused(Some(101));
        // Ensure a measurable timestamp gap
        thread::sleep(Duration::from_millis(2));
        // Focus 102 on tag 2
        state
            .borrow_mut()
            .displays
            .get_mut(&1)
            .unwrap()
            .visible_tags = Tag::from_mask(0b0010);
        state.borrow_mut().set_focused(Some(102));

        // Now view tags 1+2 together
        state
            .borrow_mut()
            .displays
            .get_mut(&1)
            .unwrap()
            .visible_tags = Tag::from_mask(0b0011);
        // Force a refocus by clearing current focus into something non-visible
        // (move 102 to tag 1 will still be visible; instead set focused to None)
        // Actually simpler: keep focused = 102 which is still visible → early return.
        // To force the restoration branch, simulate focused window becoming non-visible
        // by setting focused to a non-existent id.
        state.borrow_mut().set_focused(None);
        state.borrow_mut().focused = Some(9999);

        focus_visible_window_if_needed(&state, &manipulator);

        // 102 has the latest timestamp → should be picked
        assert_eq!(state.borrow().focused, Some(102));
    }

    #[test]
    fn test_restore_focus_validates_window_still_has_tag() {
        // After the recorded window moves to a different tag, the stale entry
        // must be skipped at validation time and the fallback path used.
        let (state, manipulator) = setup_three_windows();

        // Record 101 on tag 1
        state.borrow_mut().set_focused(Some(101));
        // Move 101 away from tag 1
        state.borrow_mut().windows.get_mut(&101).unwrap().tags = Tag::from_mask(0b0010);
        // Force refocus path
        state.borrow_mut().focused = Some(9999);

        focus_visible_window_if_needed(&state, &manipulator);

        let focused = state.borrow().focused;
        // 101 should not be chosen (no longer on tag 1)
        assert_ne!(focused, Some(101));
        // Fallback should pick a tile window on tag 1: 100 or 102
        assert!(focused == Some(100) || focused == Some(102));
    }

    #[test]
    fn test_restore_focus_falls_back_to_tag_orders_first() {
        // With last_focused_per_tag empty, the fallback must pick the first
        // window in tag_orders (deterministic, not HashMap iteration order).
        let (state, manipulator) = setup_three_windows();
        // Override tag_orders so 102 is first (different from WindowID ascending)
        state
            .borrow_mut()
            .displays
            .get_mut(&1)
            .unwrap()
            .tag_orders
            .insert(1, vec![102, 100, 101]);
        // Clear any focus records
        state
            .borrow_mut()
            .displays
            .get_mut(&1)
            .unwrap()
            .last_focused_per_tag
            .clear();
        // Force the refocus branch
        state.borrow_mut().focused = Some(9999);

        focus_visible_window_if_needed(&state, &manipulator);

        // tag_orders[1] is [102, 100, 101] → fallback picks 102
        assert_eq!(state.borrow().focused, Some(102));
    }

    #[test]
    fn test_restore_focus_fallback_skips_fullscreen_preference() {
        // Empty last_focused_per_tag + a fullscreen window with tag_orders
        // putting 100 first: the old implementation preferred fullscreen, the
        // new one picks the first entry in tag_orders (100).
        let (state, manipulator) = setup_three_windows();
        state
            .borrow_mut()
            .displays
            .get_mut(&1)
            .unwrap()
            .tag_orders
            .insert(1, vec![100, 101, 102]);
        state
            .borrow_mut()
            .windows
            .get_mut(&102)
            .unwrap()
            .is_fullscreen = true;
        state
            .borrow_mut()
            .displays
            .get_mut(&1)
            .unwrap()
            .last_focused_per_tag
            .clear();
        state.borrow_mut().focused = Some(9999);

        focus_visible_window_if_needed(&state, &manipulator);

        // No more fullscreen preference: tag_orders[1] first = 100
        assert_eq!(state.borrow().focused, Some(100));
    }
}
