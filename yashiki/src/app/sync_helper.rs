use std::cell::RefCell;

use crate::core::{State, WindowId};
use crate::event_emitter::EventEmitter;
use crate::layout::LayoutEngineManager;
use crate::macos::ObserverManager;
use crate::platform::{WindowManipulator, WindowSystem};

use super::effects::execute_effects;

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

#[cfg(test)]
mod tests {
    use std::ptr;
    use std::sync::atomic::AtomicPtr;
    use std::sync::mpsc as std_mpsc;
    use std::sync::Arc;

    use super::*;
    use crate::event::Event;
    use crate::event_emitter::EventEmitter;
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
}
