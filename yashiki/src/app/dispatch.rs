use std::cell::RefCell;

use crate::core::State;
use crate::event_emitter::EventEmitter;
use crate::layout::LayoutEngineManager;
use crate::macos::HotkeyManager;
use crate::platform::{WindowManipulator, WindowSystem};
use yashiki_ipc::{Command, Response};

use super::command::{list_all_windows, process_command};
use super::effects::execute_effects;
use super::state_events::{capture_event_state, emit_state_change_events};

/// Unified command dispatcher for IPC and hotkey commands.
/// Handles the common pattern: capture state -> process command -> execute effects -> emit events.
pub fn dispatch_command<S: WindowSystem, M: WindowManipulator>(
    cmd: &Command,
    state: &RefCell<State>,
    layout_engine_manager: &RefCell<LayoutEngineManager>,
    hotkey_manager: &RefCell<HotkeyManager>,
    window_system: &S,
    manipulator: &M,
    event_emitter: &EventEmitter,
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
        cmd,
    );

    // Emit events based on state changes
    emit_state_change_events(event_emitter, state, &pre_state);

    response
}

/// This function orchestrates process_command and execute_effects.
fn handle_ipc_command<S: WindowSystem, M: WindowManipulator>(
    state: &RefCell<State>,
    layout_engine_manager: &RefCell<LayoutEngineManager>,
    hotkey_manager: &RefCell<HotkeyManager>,
    window_system: &S,
    manipulator: &M,
    cmd: &Command,
) -> Response {
    // Handle ListWindows with all=true specially (requires system query)
    if let Command::ListWindows { all: true, debug } = cmd {
        return list_all_windows(state, window_system, *debug);
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
    use super::*;
    use crate::platform::mock::{
        create_test_display, create_test_window, MockWindowManipulator, MockWindowSystem,
    };
    use std::sync::atomic::AtomicPtr;
    use std::sync::mpsc as std_mpsc;
    use std::sync::Arc;

    fn setup_test_context() -> (
        RefCell<State>,
        RefCell<LayoutEngineManager>,
        RefCell<HotkeyManager>,
        MockWindowSystem,
        MockWindowManipulator,
        EventEmitter,
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

        (
            state,
            layout_engine_manager,
            hotkey_manager,
            ws,
            manipulator,
            event_emitter,
        )
    }

    #[test]
    fn test_dispatch_command_list_windows() {
        let (state, layout_manager, hotkey_manager, ws, manipulator, event_emitter) =
            setup_test_context();

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
        );

        assert!(matches!(response, Response::Windows { .. }));
    }

    #[test]
    fn test_dispatch_command_get_state() {
        let (state, layout_manager, hotkey_manager, ws, manipulator, event_emitter) =
            setup_test_context();

        let response = dispatch_command(
            &Command::GetState,
            &state,
            &layout_manager,
            &hotkey_manager,
            &ws,
            &manipulator,
            &event_emitter,
        );

        assert!(matches!(response, Response::State { .. }));
    }

    #[test]
    fn test_dispatch_command_tag_view() {
        let (state, layout_manager, hotkey_manager, ws, manipulator, event_emitter) =
            setup_test_context();

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
        );

        assert!(matches!(response, Response::Ok));
        assert_eq!(state.borrow().visible_tags().mask(), 0b10);
    }
}
