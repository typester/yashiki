use std::cell::RefCell;

use crate::core::State;
use crate::event_emitter::EventEmitter;
use crate::layout::LayoutEngineManager;
use crate::macos::{DisplayId, HotkeyManager, ObserverManager};
use crate::platform::{WindowManipulator, WindowSystem};
use yashiki_ipc::{Command, Response};

use super::command::{list_all_windows, process_command};
use super::effects::execute_effects;
use super::state_events::{capture_event_state, emit_state_change_events};
use super::sync_helper::sync_display_and_process_new_windows;

/// Unified command dispatcher for IPC and hotkey commands.
/// Handles the common pattern: capture state -> process command -> execute effects -> emit events.
#[allow(clippy::too_many_arguments)]
pub fn dispatch_command<S: WindowSystem, M: WindowManipulator>(
    cmd: &Command,
    state: &RefCell<State>,
    layout_engine_manager: &RefCell<LayoutEngineManager>,
    hotkey_manager: &RefCell<HotkeyManager>,
    window_system: &S,
    manipulator: &M,
    event_emitter: &EventEmitter,
    observer_manager: &RefCell<ObserverManager>,
) -> Response {
    // Capture state before command for event emission
    let pre_state = capture_event_state(state);

    // Process command and execute effects
    let response = handle_ipc_command(
        state,
        layout_engine_manager,
        hotkey_manager,
        window_system,
        manipulator,
        event_emitter,
        observer_manager,
        cmd,
    );

    // Emit events based on state changes
    emit_state_change_events(event_emitter, state, &pre_state);

    response
}

/// Returns target display_id for tag-view commands, None for other commands.
fn get_tag_view_display(cmd: &Command, state: &State) -> Option<DisplayId> {
    match cmd {
        Command::TagView { output, .. } | Command::TagToggle { output, .. } => {
            state.get_target_display(output.as_ref()).ok()
        }
        Command::TagViewLast => Some(state.focused_display),
        _ => None,
    }
}

/// This function orchestrates process_command and execute_effects.
#[allow(clippy::too_many_arguments)]
fn handle_ipc_command<S: WindowSystem, M: WindowManipulator>(
    state: &RefCell<State>,
    layout_engine_manager: &RefCell<LayoutEngineManager>,
    hotkey_manager: &RefCell<HotkeyManager>,
    window_system: &S,
    manipulator: &M,
    event_emitter: &EventEmitter,
    observer_manager: &RefCell<ObserverManager>,
    cmd: &Command,
) -> Response {
    // Handle ListWindows with all=true specially (requires system query)
    if let Command::ListWindows { all: true, debug } = cmd {
        return list_all_windows(state, window_system, *debug);
    }

    // Handle tag-view commands with pre-sync to remove stale windows
    // Get display_id in a separate scope to avoid borrow conflict
    let tag_view_display = get_tag_view_display(cmd, &state.borrow());
    if let Some(display_id) = tag_view_display {
        // Note: SyncResult.changed is ignored because tag-view commands always produce
        // RetileDisplays effect, so retile will happen regardless of sync changes
        let _ = sync_display_and_process_new_windows(
            state,
            window_system,
            layout_engine_manager,
            manipulator,
            event_emitter,
            observer_manager,
            display_id,
        );
    }

    let result = process_command(
        &mut state.borrow_mut(),
        &mut hotkey_manager.borrow_mut(),
        cmd,
    );

    if let Err(e) = execute_effects(result.effects, state, layout_engine_manager, manipulator) {
        return Response::Error { message: e };
    }

    result.response
}

#[cfg(test)]
mod tests {
    use std::ptr;
    use std::sync::atomic::AtomicPtr;
    use std::sync::mpsc as std_mpsc;
    use std::sync::Arc;

    use super::*;
    use crate::event::Event;
    use crate::platform::mock::{
        create_test_display, create_test_window, MockWindowManipulator, MockWindowSystem,
    };

    fn setup_test_context() -> (
        RefCell<State>,
        RefCell<LayoutEngineManager>,
        RefCell<HotkeyManager>,
        MockWindowSystem,
        MockWindowManipulator,
        EventEmitter,
        RefCell<ObserverManager>,
    ) {
        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![
                create_test_window(100, 1000, "Safari", 0.0, 0.0, 960.0, 1080.0),
                create_test_window(101, 1001, "Terminal", 960.0, 0.0, 960.0, 1080.0),
            ])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws);
        let state = RefCell::new(state);

        let layout_engine_manager = RefCell::new(LayoutEngineManager::new());

        let (tx, _rx) = std_mpsc::channel();
        let dummy_source = Arc::new(AtomicPtr::new(std::ptr::null_mut()));
        let hotkey_manager = RefCell::new(HotkeyManager::new(tx, dummy_source));

        let (event_tx, _event_rx) = std_mpsc::channel();
        let event_emitter = EventEmitter::new(event_tx);

        let manipulator = MockWindowManipulator::new();

        let (observer_event_tx, _observer_event_rx) = std_mpsc::channel::<Event>();
        let observer_source_ptr = Arc::new(AtomicPtr::new(ptr::null_mut()));
        let observer_manager =
            RefCell::new(ObserverManager::new(observer_event_tx, observer_source_ptr));

        (
            state,
            layout_engine_manager,
            hotkey_manager,
            ws,
            manipulator,
            event_emitter,
            observer_manager,
        )
    }

    #[test]
    fn test_dispatch_command_list_windows() {
        let (
            state,
            layout_manager,
            hotkey_manager,
            ws,
            manipulator,
            event_emitter,
            observer_manager,
        ) = setup_test_context();

        let response = dispatch_command(
            &Command::ListWindows {
                all: false,
                debug: false,
            },
            &state,
            &layout_manager,
            &hotkey_manager,
            &ws,
            &manipulator,
            &event_emitter,
            &observer_manager,
        );

        assert!(matches!(response, Response::Windows { .. }));
    }

    #[test]
    fn test_dispatch_command_get_state() {
        let (
            state,
            layout_manager,
            hotkey_manager,
            ws,
            manipulator,
            event_emitter,
            observer_manager,
        ) = setup_test_context();

        let response = dispatch_command(
            &Command::GetState,
            &state,
            &layout_manager,
            &hotkey_manager,
            &ws,
            &manipulator,
            &event_emitter,
            &observer_manager,
        );

        assert!(matches!(response, Response::State { .. }));
    }

    #[test]
    fn test_dispatch_command_tag_view() {
        let (
            state,
            layout_manager,
            hotkey_manager,
            ws,
            manipulator,
            event_emitter,
            observer_manager,
        ) = setup_test_context();

        let response = dispatch_command(
            &Command::TagView {
                tags: 0b10,
                output: None,
            },
            &state,
            &layout_manager,
            &hotkey_manager,
            &ws,
            &manipulator,
            &event_emitter,
            &observer_manager,
        );

        assert!(matches!(response, Response::Ok));
        assert_eq!(state.borrow().visible_tags().mask(), 0b10);
    }
}
