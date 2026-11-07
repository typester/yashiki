use std::cell::RefCell;
use std::collections::HashSet;

use crate::core::{State, WindowId, WindowMove};
use crate::effect::Effect;
use crate::event_emitter::EventEmitter;
use crate::layout::LayoutEngineManager;
use crate::macos::{DisplayId, ObserverManager};
use crate::platform::{WindowManipulator, WindowSystem};

use super::effects::execute_effects;
use super::state_events::{capture_event_state, emit_state_change_events};

/// Result of a sync operation
pub struct SyncResult {
    /// Whether the state changed and retiling may be needed
    pub changed: bool,
}

/// Sync windows for a specific PID and process newly discovered windows.
/// Returns whether the state changed (which may require retiling).
pub fn sync_and_process_new_windows<W: WindowSystem, M: WindowManipulator>(
    state: &RefCell<State>,
    window_system: &W,
    layout_engine_manager: &RefCell<LayoutEngineManager>,
    manipulator: &M,
    event_emitter: &EventEmitter,
    observer_manager: &RefCell<ObserverManager>,
    pid: i32,
) -> SyncResult {
    // Ensure observer exists for this PID before syncing
    // This handles windows discovered via sync (e.g., when an app was running before yashiki started
    // or when AppLaunched event was missed)
    if !observer_manager.borrow().has_observer(pid) {
        tracing::info!("Adding observer for pid {} during sync", pid);
        if let Err(e) = observer_manager.borrow_mut().add_observer(pid) {
            tracing::warn!("Failed to add observer for pid {}: {}", pid, e);
        }
    }

    let (changed, new_window_ids, rehide_moves) = state.borrow_mut().sync_pid(window_system, pid);

    // Re-hide windows that macOS moved from hide position
    if !rehide_moves.is_empty() {
        manipulator.apply_window_moves(&rehide_moves);
    }

    // Apply rules to newly discovered windows and emit events
    process_new_windows(
        new_window_ids.clone(),
        state,
        layout_engine_manager,
        manipulator,
        event_emitter,
    );

    SyncResult { changed }
}

/// Re-check which displays are showing a native fullscreen Space. When that changes, resync windows, apply the
/// hide/show held back while a display was away, and retile. Returns whether anything changed.
pub fn refresh_fullscreen_spaces<W: WindowSystem, M: WindowManipulator>(
    state: &RefCell<State>,
    window_system: &W,
    layout_engine_manager: &RefCell<LayoutEngineManager>,
    manipulator: &M,
    event_emitter: &EventEmitter,
) -> bool {
    let display_ids: Vec<DisplayId> = state.borrow().displays.keys().copied().collect();
    let now = window_system.fullscreen_space_displays(&display_ids);
    let on_other_space_before: HashSet<WindowId> = state
        .borrow()
        .windows
        .values()
        .filter(|w| w.on_other_space)
        .map(|w| w.id)
        .collect();
    let transitions = state.borrow_mut().update_fullscreen_spaces(now);
    if transitions.entered.is_empty() && transitions.returned.is_empty() {
        return false;
    }
    for display_id in &transitions.entered {
        tracing::info!("Display {} is showing a fullscreen Space", display_id);
    }
    for display_id in &transitions.returned {
        tracing::info!("Display {} is back from a fullscreen Space", display_id);
    }

    let pre_state = capture_event_state(state);

    let (rehide_moves, new_window_ids) = state.borrow_mut().sync_all(window_system);
    if !rehide_moves.is_empty() {
        manipulator.apply_window_moves(&rehide_moves);
    }
    process_new_windows(
        new_window_ids,
        state,
        layout_engine_manager,
        manipulator,
        event_emitter,
    );

    let mut moves = Vec::new();
    for &display_id in &transitions.returned {
        let mut s = state.borrow_mut();
        moves.extend(follow_returned_focused_window(
            &mut s,
            display_id,
            &on_other_space_before,
        ));
        moves.extend(s.compute_layout_changes(display_id));
    }
    let _ = execute_effects(
        vec![Effect::ApplyWindowMoves(moves), Effect::Retile],
        state,
        layout_engine_manager,
        manipulator,
    );

    emit_state_change_events(event_emitter, state, &pre_state);
    true
}

/// If the focused window has just come back from another Space onto `display_id` and its tags are not shown there,
/// switch the display to its tag, as an external focus change does.
fn follow_returned_focused_window(
    state: &mut State,
    display_id: DisplayId,
    on_other_space_before: &HashSet<WindowId>,
) -> Vec<WindowMove> {
    let Some((window_id, tags)) = state
        .focused
        .filter(|id| on_other_space_before.contains(id))
        .and_then(|id| state.windows.get(&id))
        .filter(|w| !w.on_other_space && w.display_id == display_id)
        .map(|w| (w.id, w.tags))
    else {
        return vec![];
    };
    let shown = state
        .displays
        .get(&display_id)
        .is_some_and(|d| tags.intersects(d.visible_tags));
    let Some(tag) = tags.first_tag().filter(|_| !shown) else {
        return vec![];
    };
    tracing::info!(
        "Switching display {} to tag {} for window {} back from another Space",
        display_id,
        tag,
        window_id
    );
    state.view_tags_on_display(1 << (tag - 1), display_id)
}

/// Process newly discovered windows: apply rules and emit events.
pub fn process_new_windows<M: WindowManipulator>(
    new_window_ids: Vec<WindowId>,
    state: &RefCell<State>,
    layout_engine_manager: &RefCell<LayoutEngineManager>,
    manipulator: &M,
    event_emitter: &EventEmitter,
) {
    for window_id in new_window_ids {
        let effects = state.borrow_mut().apply_rules_to_new_window(window_id);
        if !effects.is_empty() {
            let _ = execute_effects(effects, state, layout_engine_manager, manipulator);
        }

        // Emit window created event
        {
            let state = state.borrow();
            if let Some(window) = state.windows.get(&window_id) {
                event_emitter.emit_window_created(window, state.focused);
            }
        }
    }
}

/// Sync focused window with hint and process newly discovered windows.
/// Returns whether the state changed (which may require retiling).
pub fn sync_focused_and_process<W: WindowSystem, M: WindowManipulator>(
    state: &RefCell<State>,
    window_system: &W,
    layout_engine_manager: &RefCell<LayoutEngineManager>,
    manipulator: &M,
    event_emitter: &EventEmitter,
    observer_manager: &RefCell<ObserverManager>,
    pid_hint: Option<i32>,
) -> SyncResult {
    // Ensure observer exists for this PID if provided
    if let Some(pid) = pid_hint {
        if !observer_manager.borrow().has_observer(pid) {
            tracing::info!("Adding observer for pid {} during focused sync", pid);
            if let Err(e) = observer_manager.borrow_mut().add_observer(pid) {
                tracing::warn!("Failed to add observer for pid {}: {}", pid, e);
            }
        }
    }

    let (changed, new_window_ids) = state
        .borrow_mut()
        .sync_focused_window_with_hint(window_system, pid_hint);

    // Apply rules to newly discovered windows and emit events
    process_new_windows(
        new_window_ids,
        state,
        layout_engine_manager,
        manipulator,
        event_emitter,
    );

    SyncResult { changed }
}

/// Sync all windows on a display and process newly discovered windows.
/// Used before tag-view commands to remove stale windows and apply rules to new ones.
pub fn sync_display_and_process_new_windows<W: WindowSystem, M: WindowManipulator>(
    state: &RefCell<State>,
    window_system: &W,
    layout_engine_manager: &RefCell<LayoutEngineManager>,
    manipulator: &M,
    event_emitter: &EventEmitter,
    observer_manager: &RefCell<ObserverManager>,
    display_id: DisplayId,
) -> SyncResult {
    // Collect PIDs on this display and ensure observers exist
    let pids: Vec<i32> = state
        .borrow()
        .windows
        .values()
        .filter(|w| w.display_id == display_id)
        .map(|w| w.pid)
        .collect();

    for pid in &pids {
        if !observer_manager.borrow().has_observer(*pid) {
            tracing::info!("Adding observer for pid {} during display sync", pid);
            if let Err(e) = observer_manager.borrow_mut().add_observer(*pid) {
                tracing::warn!("Failed to add observer for pid {}: {}", pid, e);
            }
        }
    }

    let (changed, new_window_ids, rehide_moves) = state
        .borrow_mut()
        .sync_windows_for_display(window_system, display_id);

    if !rehide_moves.is_empty() {
        manipulator.apply_window_moves(&rehide_moves);
    }

    process_new_windows(
        new_window_ids,
        state,
        layout_engine_manager,
        manipulator,
        event_emitter,
    );

    SyncResult { changed }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::ptr;
    use std::sync::atomic::AtomicPtr;
    use std::sync::mpsc as std_mpsc;
    use std::sync::Arc;

    use super::*;
    use crate::core::Tag;
    use crate::event::Event;
    use crate::event_emitter::EventEmitter;
    use crate::macos::space::SpacePresence;
    use crate::platform::mock::{
        create_test_display, create_test_window, MockWindowManipulator, MockWindowSystem,
    };

    fn create_test_observer_manager() -> RefCell<ObserverManager> {
        let (observer_event_tx, _observer_event_rx) = std_mpsc::channel::<Event>();
        let observer_source_ptr = Arc::new(AtomicPtr::new(ptr::null_mut()));
        RefCell::new(ObserverManager::new(observer_event_tx, observer_source_ptr))
    }

    #[test]
    fn test_sync_and_process_new_windows() {
        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 0.0, 0.0, 960.0, 1080.0,
            )]);

        let mut state = State::new();
        state.sync_all(&ws);
        let state = RefCell::new(state);

        let layout_engine_manager = RefCell::new(LayoutEngineManager::new());
        let manipulator = MockWindowManipulator::new();
        let (event_tx, _event_rx) = std_mpsc::channel();
        let event_emitter = EventEmitter::new(event_tx);
        let observer_manager = create_test_observer_manager();

        // Add a new window to the mock system
        let mut ws = ws;
        ws.add_window(create_test_window(
            101, 1000, "Safari", 100.0, 100.0, 800.0, 600.0,
        ));

        // Sync and process
        let result = sync_and_process_new_windows(
            &state,
            &ws,
            &layout_engine_manager,
            &manipulator,
            &event_emitter,
            &observer_manager,
            1000,
        );

        assert!(result.changed);
        assert_eq!(state.borrow().windows.len(), 2);
    }

    #[test]
    fn test_sync_removes_closed_windows() {
        let mut ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![
                create_test_window(100, 1000, "Safari", 0.0, 0.0, 960.0, 1080.0),
                create_test_window(101, 1000, "Safari", 100.0, 100.0, 800.0, 600.0),
            ]);

        let mut state = State::new();
        state.sync_all(&ws);
        let state = RefCell::new(state);

        let layout_engine_manager = RefCell::new(LayoutEngineManager::new());
        let manipulator = MockWindowManipulator::new();
        let (event_tx, _event_rx) = std_mpsc::channel();
        let event_emitter = EventEmitter::new(event_tx);
        let observer_manager = create_test_observer_manager();

        // Remove a window from the mock system
        ws.remove_window(101);

        // Sync and process
        let result = sync_and_process_new_windows(
            &state,
            &ws,
            &layout_engine_manager,
            &manipulator,
            &event_emitter,
            &observer_manager,
            1000,
        );

        assert!(result.changed);
        assert_eq!(state.borrow().windows.len(), 1);
    }

    fn refresh(state: &RefCell<State>, ws: &MockWindowSystem) -> bool {
        let layout_engine_manager = RefCell::new(LayoutEngineManager::new());
        let manipulator = MockWindowManipulator::new();
        let (event_tx, _event_rx) = std_mpsc::channel();
        let event_emitter = EventEmitter::new(event_tx);
        refresh_fullscreen_spaces(
            state,
            ws,
            &layout_engine_manager,
            &manipulator,
            &event_emitter,
        )
    }

    #[test]
    fn test_refresh_fullscreen_spaces_follows_returning_window_to_its_tag() {
        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![create_test_window(
                100, 1000, "Firefox", 0.0, 0.0, 960.0, 1080.0,
            )])
            .with_focused(Some(100));
        let mut state = State::new();
        state.sync_all(&ws);
        // Window 100 is on tag 1; the display was switched to tag 2 while the window sat on its fullscreen Space
        state.fullscreen_space_displays.insert(1);
        state.displays.get_mut(&1).unwrap().visible_tags = Tag::new(2);
        state.windows.get_mut(&100).unwrap().on_other_space = true;
        state.focused = Some(100);
        let state = RefCell::new(state);

        // The display is back on its desktop and the window is on screen again
        assert!(refresh(&state, &ws));

        let state = state.borrow();
        assert!(!state.is_on_fullscreen_space(1));
        assert!(!state.windows.get(&100).unwrap().on_other_space);
        assert_eq!(state.displays.get(&1).unwrap().visible_tags.mask(), 0b1);
    }

    #[test]
    fn test_refresh_fullscreen_spaces_keeps_desktop_windows_on_entering() {
        let mut ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![
                create_test_window(100, 1000, "Firefox", 0.0, 0.0, 960.0, 1080.0),
                create_test_window(101, 1001, "Terminal", 960.0, 0.0, 960.0, 1080.0),
            ]);
        let mut state = State::new();
        state.sync_all(&ws);
        let state = RefCell::new(state);

        // Display 1 switches to Firefox's fullscreen Space; Terminal stays behind on the desktop Space
        ws.remove_window(101);
        ws.fullscreen_spaces.insert(1);
        ws.space_presence = Some(HashMap::from([(101, SpacePresence::Other)]));
        assert!(refresh(&state, &ws));

        let state = state.borrow();
        assert!(state.is_on_fullscreen_space(1));
        assert!(state.windows.get(&101).unwrap().on_other_space);
    }

    #[test]
    fn test_refresh_fullscreen_spaces_without_change() {
        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)]);
        let mut state = State::new();
        state.sync_all(&ws);
        let state = RefCell::new(state);

        assert!(!refresh(&state, &ws));
    }
}
