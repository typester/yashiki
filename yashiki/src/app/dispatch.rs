use std::cell::RefCell;

use crate::core::State;
use crate::event_emitter::EventEmitter;
use crate::layout::LayoutEngineManager;
use crate::macos::{DisplayId, HotkeyManager, ObserverManager};
use crate::platform::{WindowManipulator, WindowSystem};
use yashiki_ipc::{Command, OutputSpecifier, Response};

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
    let pre_mode = hotkey_manager.borrow().current_mode();

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

    // Emit mode change event if mode changed
    let post_mode = hotkey_manager.borrow().current_mode();
    if pre_mode != post_mode {
        event_emitter.emit_mode_changed(&post_mode);
    }

    response
}

/// How a command is gated while a display is showing a native fullscreen Space.
pub(crate) enum SpaceGate<'a> {
    /// Acts on the focused display.
    FocusedDisplay,
    /// Acts on the display given by `--output`, or the focused one.
    Output(Option<&'a OutputSpecifier>),
    /// Not blocked over IPC; only its hotkey passes through on a fullscreen Space.
    HotkeyOnly,
}

/// Classify commands that act on windows, tags or layout. No wildcard arm, so new commands must be classified.
pub(crate) fn space_gate(cmd: &Command) -> Option<SpaceGate<'_>> {
    match cmd {
        Command::WindowFocus { .. }
        | Command::WindowSwap { .. }
        | Command::WindowClose
        | Command::WindowToggleFloat
        | Command::WindowToggleFullscreen
        | Command::WindowMoveToTag { .. }
        | Command::WindowToggleTag { .. }
        | Command::TagViewLast
        | Command::OutputFocus { .. }
        | Command::OutputSend { .. } => Some(SpaceGate::FocusedDisplay),
        Command::TagView { output, .. } | Command::TagToggle { output, .. } => {
            Some(SpaceGate::Output(output.as_ref()))
        }
        Command::LayoutCommand { .. } | Command::LayoutSet { .. } | Command::Retile { .. } => {
            Some(SpaceGate::HotkeyOnly)
        }
        Command::LayoutSetDefault { .. }
        | Command::LayoutGet { .. }
        | Command::DeclareMode { .. }
        | Command::EnterMode { .. }
        | Command::GetMode
        | Command::Bind { .. }
        | Command::Unbind { .. }
        | Command::ListBindings { .. }
        | Command::ListWindows { .. }
        | Command::ListOutputs
        | Command::GetState
        | Command::FocusedWindow
        | Command::Exec { .. }
        | Command::ExecOrFocus { .. }
        | Command::GetExecPath
        | Command::SetExecPath { .. }
        | Command::AddExecPath { .. }
        | Command::RuleAdd { .. }
        | Command::RuleDel { .. }
        | Command::ListRules
        | Command::ApplyRules
        | Command::SetCursorWarp { .. }
        | Command::GetCursorWarp
        | Command::SetAutoRaise { .. }
        | Command::GetAutoRaise
        | Command::SetMenuBarStatus { .. }
        | Command::GetMenuBarStatus
        | Command::SetOuterGap { .. }
        | Command::GetOuterGap
        | Command::SetLogLevel { .. }
        | Command::GetLogLevel
        | Command::Quit => None,
    }
}

/// The display that blocks the command because it is showing a native fullscreen Space, if any.
fn fullscreen_space_block(cmd: &Command, state: &State) -> Option<DisplayId> {
    let candidates: Vec<DisplayId> = match space_gate(cmd)? {
        SpaceGate::FocusedDisplay => {
            let focused_window_display = state
                .focused
                .and_then(|id| state.windows.get(&id))
                .map(|w| w.display_id);
            std::iter::once(state.effective_focused_display())
                .chain(focused_window_display)
                .collect()
        }
        SpaceGate::Output(spec) => state.get_target_display(spec).ok().into_iter().collect(),
        SpaceGate::HotkeyOnly => return None,
    };
    candidates
        .into_iter()
        .find(|&id| state.is_on_fullscreen_space(id))
}

/// Returns target display_id for tag-view commands, None for other commands.
fn get_tag_view_display(cmd: &Command, state: &State) -> Option<DisplayId> {
    match cmd {
        Command::TagView { output, .. } | Command::TagToggle { output, .. } => {
            state.get_target_display(output.as_ref()).ok()
        }
        Command::TagViewLast => Some(state.effective_focused_display()),
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

    if let Some(display_id) = fullscreen_space_block(cmd, &state.borrow()) {
        tracing::info!(
            "Ignoring {:?}: display {} is showing a fullscreen Space",
            cmd,
            display_id
        );
        return Response::Error {
            message: format!("Display {} is showing a fullscreen Space", display_id),
        };
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
    use yashiki_ipc::Direction;

    type TestContext = (
        RefCell<State>,
        RefCell<LayoutEngineManager>,
        RefCell<HotkeyManager>,
        MockWindowSystem,
        MockWindowManipulator,
        EventEmitter,
        RefCell<ObserverManager>,
    );

    fn setup_test_context() -> TestContext {
        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![
                create_test_window(100, 1000, "Safari", 0.0, 0.0, 960.0, 1080.0),
                create_test_window(101, 1001, "Terminal", 960.0, 0.0, 960.0, 1080.0),
            ])
            .with_focused(Some(100));
        setup_test_context_with(ws)
    }

    fn setup_test_context_with(ws: MockWindowSystem) -> TestContext {
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

    fn dispatch(ctx: &TestContext, cmd: Command) -> Response {
        let (
            state,
            layout_manager,
            hotkey_manager,
            ws,
            manipulator,
            event_emitter,
            observer_manager,
        ) = ctx;
        dispatch_command(
            &cmd,
            state,
            layout_manager,
            hotkey_manager,
            ws,
            manipulator,
            event_emitter,
            observer_manager,
        )
    }

    #[test]
    fn test_dispatch_ignores_command_for_display_on_fullscreen_space() {
        let ctx = setup_test_context();
        ctx.0.borrow_mut().fullscreen_space_displays.insert(1);

        let response = dispatch(
            &ctx,
            Command::TagView {
                tags: 0b10,
                output: None,
            },
        );

        assert!(matches!(response, Response::Error { .. }));
        assert_eq!(ctx.0.borrow().visible_tags().mask(), 0b1);
    }

    #[test]
    fn test_dispatch_allows_tag_view_on_other_display() {
        let ws = MockWindowSystem::new()
            .with_displays(vec![
                create_test_display(1, 0.0, 0.0, 1920.0, 1080.0),
                create_test_display(2, 1920.0, 0.0, 1920.0, 1080.0),
            ])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 0.0, 0.0, 960.0, 1080.0,
            )])
            .with_focused(Some(100));
        let ctx = setup_test_context_with(ws);
        ctx.0.borrow_mut().fullscreen_space_displays.insert(1);

        let response = dispatch(
            &ctx,
            Command::TagView {
                tags: 0b10,
                output: Some(OutputSpecifier::Id(2)),
            },
        );

        assert!(matches!(response, Response::Ok));
        assert_eq!(
            ctx.0.borrow().displays.get(&2).unwrap().visible_tags.mask(),
            0b10
        );
    }

    #[test]
    fn test_dispatch_does_not_block_config_or_exec_on_fullscreen_space() {
        let ctx = setup_test_context();
        ctx.0.borrow_mut().fullscreen_space_displays.insert(1);

        let layout_set = Command::LayoutSet {
            tags: Some(0b100),
            output: None,
            layout: "byobu".to_string(),
        };
        assert!(matches!(dispatch(&ctx, layout_set), Response::Ok));

        let exec = Command::Exec {
            command: "true".to_string(),
            track: false,
        };
        assert!(matches!(dispatch(&ctx, exec), Response::Ok));
    }

    #[test]
    fn test_space_gate_classification() {
        assert!(matches!(
            space_gate(&Command::WindowFocus {
                direction: Direction::Next
            }),
            Some(SpaceGate::FocusedDisplay)
        ));
        assert!(matches!(
            space_gate(&Command::TagView {
                tags: 1,
                output: None
            }),
            Some(SpaceGate::Output(None))
        ));
        assert!(matches!(
            space_gate(&Command::LayoutCommand {
                layout: None,
                cmd: "zoom".to_string(),
                args: vec![],
            }),
            Some(SpaceGate::HotkeyOnly)
        ));
        assert!(space_gate(&Command::Exec {
            command: "true".to_string(),
            track: false,
        })
        .is_none());
    }
}
