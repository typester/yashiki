use crate::core::{State, WindowMove};
use crate::effect::{CommandResult, Effect};
use crate::event::Event;
use crate::event_emitter::{create_snapshot, EventEmitter};
use crate::ipc::{EventBroadcaster, EventServer, IpcServer};
use crate::layout::LayoutEngineManager;
use crate::macos;
use crate::macos::{HotkeyManager, ObserverManager, WorkspaceEvent, WorkspaceWatcher};
use crate::pid;
use crate::platform::{MacOSWindowManipulator, MacOSWindowSystem, WindowManipulator};
use anyhow::Result;
use core_foundation::base::TCFType;
use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoop};
use core_foundation_sys::runloop::{
    CFRunLoopAddSource, CFRunLoopGetMain, CFRunLoopSourceContext, CFRunLoopSourceCreate,
    CFRunLoopSourceRef, CFRunLoopSourceSignal, CFRunLoopWakeUp,
};
use objc2_foundation::MainThreadMarker;
use std::cell::RefCell;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
use tokio::sync::mpsc;
use yashiki_ipc::{
    BindingInfo, Command, CursorWarpMode, OuterGap, OutputInfo, Response, RuleInfo, StateEvent,
    StateInfo, WindowInfo,
};

type IpcCommandWithResponse = (Command, mpsc::Sender<Response>);

type SnapshotRequest = tokio::sync::oneshot::Sender<StateEvent>;

struct IpcRelay {
    cmd_tx: std_mpsc::Sender<IpcCommandWithResponse>,
    server_tx: mpsc::Sender<IpcCommandWithResponse>,
    server_rx: mpsc::Receiver<IpcCommandWithResponse>,
    runloop_source: Arc<AtomicPtr<std::ffi::c_void>>,
}

struct EventStreaming {
    broadcaster: EventBroadcaster,
    event_server_rx: tokio::sync::broadcast::Receiver<StateEvent>,
    state_event_rx: std_mpsc::Receiver<StateEvent>,
}

struct SnapshotRelay {
    request_tx: mpsc::Sender<SnapshotRequest>,
    request_rx: mpsc::Receiver<SnapshotRequest>,
    main_tx: std_mpsc::Sender<SnapshotRequest>,
}

struct TokioChannels {
    ipc: IpcRelay,
    events: EventStreaming,
    snapshots: SnapshotRelay,
    event_rx: mpsc::Receiver<Event>,
}

struct MainChannels {
    ipc_cmd_rx: std_mpsc::Receiver<IpcCommandWithResponse>,
    observer_event_tx: std_mpsc::Sender<Event>,
    observer_event_rx: std_mpsc::Receiver<Event>,
    event_tx: mpsc::Sender<Event>,
    state_event_tx: std_mpsc::Sender<StateEvent>,
    snapshot_request_rx: std_mpsc::Receiver<SnapshotRequest>,
    ipc_source: Arc<AtomicPtr<std::ffi::c_void>>,
}

struct RunLoopContext {
    ipc_cmd_rx: std_mpsc::Receiver<IpcCommandWithResponse>,
    hotkey_cmd_rx: std_mpsc::Receiver<Command>,
    observer_event_rx: std_mpsc::Receiver<Event>,
    workspace_event_rx: std_mpsc::Receiver<WorkspaceEvent>,
    snapshot_request_rx: std_mpsc::Receiver<SnapshotRequest>,
    event_tx: mpsc::Sender<Event>,
    event_emitter: EventEmitter,
    observer_manager: RefCell<ObserverManager>,
    state: RefCell<State>,
    layout_engine_manager: RefCell<LayoutEngineManager>,
    hotkey_manager: RefCell<HotkeyManager>,
    window_system: MacOSWindowSystem,
    window_manipulator: MacOSWindowManipulator,
}

pub struct App {}

impl App {
    pub fn run() -> Result<()> {
        // Check if already running
        if let Some(existing_pid) = pid::check_already_running() {
            anyhow::bail!("yashiki is already running (pid: {})", existing_pid);
        }

        // Write PID file
        if let Err(e) = pid::write_pid() {
            tracing::warn!("Failed to write PID file: {}", e);
        }

        if !macos::is_trusted() {
            tracing::warn!("Accessibility permission not granted, requesting...");
            macos::is_trusted_with_prompt();
            pid::remove_pid();
            anyhow::bail!("Please grant Accessibility permission and restart");
        }

        let (tokio_channels, main_channels) = Self::create_channels();

        // Spawn tokio runtime in separate thread
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                Self::run_async(tokio_channels).await;
            });
        });

        let app = App {};
        app.run_main_loop(main_channels);

        // Clean up PID file on exit
        pid::remove_pid();
        Ok(())
    }

    fn create_channels() -> (TokioChannels, MainChannels) {
        // Channel: IPC commands (tokio -> main thread)
        let (ipc_cmd_tx, ipc_cmd_rx) = std_mpsc::channel::<IpcCommandWithResponse>();

        // Channel: observer -> main thread
        let (observer_event_tx, observer_event_rx) = std_mpsc::channel::<Event>();

        // Channel: main thread -> tokio
        let (event_tx, event_rx) = mpsc::channel::<Event>(256);

        // Channel for IPC server (tokio internal)
        let (ipc_server_tx, ipc_server_rx) = mpsc::channel::<IpcCommandWithResponse>(256);

        // Event broadcasting for state streaming
        let event_broadcaster = EventBroadcaster::new(256);
        let event_server_rx = event_broadcaster.subscribe();

        // Channel: state events (main thread -> tokio)
        let (state_event_tx, state_event_rx) = std_mpsc::channel::<StateEvent>();

        // Channel: snapshot requests (tokio -> main thread)
        let (snapshot_request_tx, snapshot_request_rx) = mpsc::channel::<SnapshotRequest>(16);
        let (snapshot_request_main_tx, snapshot_request_main_rx) =
            std_mpsc::channel::<SnapshotRequest>();

        // Shared pointer to CFRunLoopSource (will be set by main thread)
        let ipc_source = Arc::new(AtomicPtr::new(ptr::null_mut()));
        let ipc_source_clone = Arc::clone(&ipc_source);

        let tokio_channels = TokioChannels {
            ipc: IpcRelay {
                cmd_tx: ipc_cmd_tx,
                server_tx: ipc_server_tx,
                server_rx: ipc_server_rx,
                runloop_source: ipc_source_clone,
            },
            events: EventStreaming {
                broadcaster: event_broadcaster,
                event_server_rx,
                state_event_rx,
            },
            snapshots: SnapshotRelay {
                request_tx: snapshot_request_tx,
                request_rx: snapshot_request_rx,
                main_tx: snapshot_request_main_tx,
            },
            event_rx,
        };

        let main_channels = MainChannels {
            ipc_cmd_rx,
            observer_event_tx,
            observer_event_rx,
            event_tx,
            state_event_tx,
            snapshot_request_rx: snapshot_request_main_rx,
            ipc_source,
        };

        (tokio_channels, main_channels)
    }

    async fn run_async(channels: TokioChannels) {
        // Destructure for partial moves
        let TokioChannels {
            ipc,
            events,
            snapshots,
            mut event_rx,
        } = channels;
        let IpcRelay {
            cmd_tx: ipc_cmd_tx,
            server_tx: ipc_server_tx,
            server_rx: mut ipc_rx,
            runloop_source: ipc_source,
        } = ipc;
        let EventStreaming {
            broadcaster: event_broadcaster,
            event_server_rx,
            state_event_rx,
        } = events;
        let SnapshotRelay {
            request_tx: snapshot_request_tx,
            request_rx: mut snapshot_request_rx,
            main_tx: snapshot_request_main_tx,
        } = snapshots;

        tracing::info!("Tokio runtime started");

        // Start IPC server
        let ipc_server = IpcServer::new(ipc_server_tx);
        tokio::spawn(async move {
            if let Err(e) = ipc_server.run().await {
                tracing::error!("IPC server error: {}", e);
            }
        });

        // Start Event server
        let event_server = EventServer::new(event_server_rx, snapshot_request_tx);
        tokio::spawn(async move {
            if let Err(e) = event_server.run().await {
                tracing::error!("Event server error: {}", e);
            }
        });

        // Spawn task to forward state events from main thread to broadcast channel
        let broadcaster_clone = event_broadcaster.clone();
        std::thread::spawn(move || {
            while let Ok(event) = state_event_rx.recv() {
                broadcaster_clone.send(event);
            }
        });

        loop {
            tokio::select! {
                Some((cmd, resp_tx)) = ipc_rx.recv() => {
                    // Forward IPC commands to main thread
                    if ipc_cmd_tx.send((cmd, resp_tx)).is_err() {
                        tracing::error!("Failed to forward IPC command to main thread");
                        break;
                    }
                    // Wake up the main thread's RunLoop immediately
                    let source = ipc_source.load(Ordering::Acquire);
                    if !source.is_null() {
                        unsafe {
                            CFRunLoopSourceSignal(source as CFRunLoopSourceRef);
                            CFRunLoopWakeUp(CFRunLoopGetMain());
                        }
                    }
                }
                Some(event) = event_rx.recv() => {
                    tracing::debug!("Received event: {:?}", event);
                }
                Some(snapshot_req) = snapshot_request_rx.recv() => {
                    // Forward snapshot request to main thread
                    if snapshot_request_main_tx.send(snapshot_req).is_err() {
                        tracing::error!("Failed to forward snapshot request to main thread");
                    }
                }
                else => break,
            }
        }

        tracing::info!("Tokio runtime exiting");
    }

    fn run_main_loop(self, channels: MainChannels) {
        // Destructure channels
        let MainChannels {
            ipc_cmd_rx,
            observer_event_tx,
            observer_event_rx,
            event_tx,
            state_event_tx,
            snapshot_request_rx,
            ipc_source: ipc_source_ptr,
        } = channels;

        tracing::info!("Starting main loop");

        // Get MainThreadMarker - we're on the main thread
        let mtm = MainThreadMarker::new().expect("Must be called from main thread");

        // Start observer manager
        let mut observer_manager = ObserverManager::new(observer_event_tx);
        observer_manager.start();

        // Start workspace watcher for app launch/terminate notifications
        let (workspace_event_tx, workspace_event_rx) = std_mpsc::channel::<WorkspaceEvent>();
        let _workspace_watcher = WorkspaceWatcher::new(workspace_event_tx, mtm);

        // Initialize state with current windows
        let window_system = MacOSWindowSystem;
        let mut state = State::new();
        state.exec_path = build_initial_exec_path();
        state.sync_all(&window_system);

        // Create layout engine manager (lazy spawning)
        let mut layout_engine_manager = LayoutEngineManager::new();
        layout_engine_manager.set_exec_path(&state.exec_path);
        let layout_engine_manager = RefCell::new(layout_engine_manager);

        let state = RefCell::new(state);

        // Create window manipulator
        let window_manipulator = MacOSWindowManipulator;

        // Create event emitter
        let event_emitter = EventEmitter::new(state_event_tx);

        // Initial retile
        do_retile(&state, &layout_engine_manager, &window_manipulator);

        // Create hotkey manager
        let (hotkey_cmd_tx, hotkey_cmd_rx) = std_mpsc::channel::<Command>();
        let mut hotkey_manager = HotkeyManager::new(hotkey_cmd_tx);

        // Start hotkey tap (initially with no bindings, will be updated via IPC)
        if let Err(e) = hotkey_manager.start() {
            tracing::warn!("Failed to start hotkey tap: {}", e);
        }

        // Create shared context for timer and IPC source
        let context = Box::new(RunLoopContext {
            ipc_cmd_rx,
            hotkey_cmd_rx,
            observer_event_rx,
            workspace_event_rx,
            snapshot_request_rx,
            event_tx,
            event_emitter,
            observer_manager: RefCell::new(observer_manager),
            state,
            layout_engine_manager,
            hotkey_manager: RefCell::new(hotkey_manager),
            window_system,
            window_manipulator,
        });
        let context_ptr = Box::into_raw(context) as *mut std::ffi::c_void;

        // Create CFRunLoopSource for IPC commands (immediate processing)
        extern "C" fn ipc_source_callback(info: *const std::ffi::c_void) {
            let ctx = unsafe { &*(info as *const RunLoopContext) };

            // Process all pending IPC commands
            while let Ok((cmd, resp_tx)) = ctx.ipc_cmd_rx.try_recv() {
                tracing::debug!("Received IPC command: {:?}", cmd);

                // Capture state before command for event emission
                let pre_state = capture_event_state(&ctx.state);

                let response = handle_ipc_command(
                    &ctx.state,
                    &ctx.layout_engine_manager,
                    &ctx.hotkey_manager,
                    &ctx.window_manipulator,
                    &cmd,
                );
                let _ = resp_tx.blocking_send(response);

                // Emit events based on state changes
                emit_state_change_events(&ctx.event_emitter, &ctx.state, &pre_state);

                // Handle Quit command after sending response
                if matches!(cmd, Command::Quit) {
                    CFRunLoop::get_current().stop();
                }
            }
            // Note: ensure_tap is called in timer_callback to batch hotkey binding changes
        }

        let mut source_context = CFRunLoopSourceContext {
            version: 0,
            info: context_ptr,
            retain: None,
            release: None,
            copyDescription: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform: ipc_source_callback,
        };

        let ipc_source = unsafe { CFRunLoopSourceCreate(ptr::null(), 0, &mut source_context) };
        if ipc_source.is_null() {
            tracing::error!("Failed to create CFRunLoopSource for IPC");
        } else {
            // Register source with main RunLoop
            let run_loop = CFRunLoop::get_current();
            unsafe {
                CFRunLoopAddSource(
                    run_loop.as_concrete_TypeRef(),
                    ipc_source,
                    kCFRunLoopDefaultMode,
                );
            }
            // Share source pointer with tokio thread
            ipc_source_ptr.store(ipc_source as *mut std::ffi::c_void, Ordering::Release);
            tracing::info!("IPC CFRunLoopSource created and registered");
        }

        let mut timer_context = core_foundation::runloop::CFRunLoopTimerContext {
            version: 0,
            info: context_ptr,
            retain: None,
            release: None,
            copyDescription: None,
        };

        extern "C" fn timer_callback(
            _timer: core_foundation::runloop::CFRunLoopTimerRef,
            info: *mut std::ffi::c_void,
        ) {
            let ctx = unsafe { &*(info as *const RunLoopContext) };

            // Process snapshot requests
            while let Ok(resp_tx) = ctx.snapshot_request_rx.try_recv() {
                let snapshot = create_snapshot(&ctx.state.borrow());
                let _ = resp_tx.send(snapshot);
            }

            // Process hotkey commands (no response needed)
            while let Ok(cmd) = ctx.hotkey_cmd_rx.try_recv() {
                tracing::debug!("Received hotkey command: {:?}", cmd);

                // Capture state before command for event emission
                let pre_state = capture_event_state(&ctx.state);

                let _ = handle_ipc_command(
                    &ctx.state,
                    &ctx.layout_engine_manager,
                    &ctx.hotkey_manager,
                    &ctx.window_manipulator,
                    &cmd,
                );

                // Emit events based on state changes
                emit_state_change_events(&ctx.event_emitter, &ctx.state, &pre_state);
            }

            // Process workspace events (app launch/terminate/display change)
            while let Ok(event) = ctx.workspace_event_rx.try_recv() {
                match event {
                    WorkspaceEvent::AppLaunched { pid } => {
                        tracing::info!("App launched, adding observer for pid {}", pid);
                        if let Err(e) = ctx.observer_manager.borrow_mut().add_observer(pid) {
                            tracing::warn!("Failed to add observer for pid {}: {}", pid, e);
                        }
                    }
                    WorkspaceEvent::AppTerminated { pid } => {
                        tracing::info!("App terminated, removing observer for pid {}", pid);
                        ctx.observer_manager.borrow_mut().remove_observer(pid);

                        // Emit window destroyed events before removing windows
                        {
                            let state = ctx.state.borrow();
                            for window in state.windows.values() {
                                if window.pid == pid {
                                    ctx.event_emitter.emit_window_destroyed(window.id);
                                }
                            }
                        }

                        // Remove windows belonging to this PID from state
                        let (changed, _) = ctx.state.borrow_mut().sync_pid(&ctx.window_system, pid);
                        if changed {
                            do_retile(
                                &ctx.state,
                                &ctx.layout_engine_manager,
                                &ctx.window_manipulator,
                            );
                        }
                    }
                    WorkspaceEvent::DisplaysChanged => {
                        tracing::info!("Display configuration changed");
                        let result = ctx
                            .state
                            .borrow_mut()
                            .handle_display_change(&ctx.window_system);

                        // Emit display events
                        let focused_display = ctx.state.borrow().focused_display;
                        for display in &result.added {
                            ctx.event_emitter
                                .emit_display_added(display, focused_display);
                        }
                        for display_id in &result.removed {
                            ctx.event_emitter.emit_display_removed(*display_id);
                        }

                        // Apply window moves for orphaned windows
                        if !result.window_moves.is_empty() {
                            ctx.window_manipulator
                                .apply_window_moves(&result.window_moves);
                        }

                        // Retile affected displays
                        if !result.displays_to_retile.is_empty() {
                            for display_id in result.displays_to_retile {
                                do_retile_display(
                                    &ctx.state,
                                    &ctx.layout_engine_manager,
                                    &ctx.window_manipulator,
                                    display_id,
                                );
                            }
                        } else {
                            // If no specific displays, retile all
                            do_retile(
                                &ctx.state,
                                &ctx.layout_engine_manager,
                                &ctx.window_manipulator,
                            );
                        }
                    }
                }
            }

            // Process observer events and forward to tokio
            let mut needs_retile = false;
            while let Ok(event) = ctx.observer_event_rx.try_recv() {
                let is_focus_event = matches!(
                    event,
                    Event::FocusedWindowChanged | Event::ApplicationActivated { .. }
                );

                let (changed, new_window_ids) = ctx
                    .state
                    .borrow_mut()
                    .handle_event(&ctx.window_system, &event);

                if changed {
                    needs_retile = true;
                }

                // Apply rules to newly created windows and emit events
                for window_id in new_window_ids {
                    // Apply rules first (may change tags, display_id, is_floating)
                    let effects = ctx.state.borrow_mut().apply_rules_to_new_window(window_id);
                    if !effects.is_empty() {
                        let _ = execute_effects(
                            effects,
                            &ctx.state,
                            &ctx.layout_engine_manager,
                            &ctx.window_manipulator,
                        );
                    }

                    // Emit window created event after rules are applied
                    {
                        let state = ctx.state.borrow();
                        if let Some(window) = state.windows.get(&window_id) {
                            ctx.event_emitter.emit_window_created(window, state.focused);
                        }
                    }
                }

                // On external focus change, notify layout engine and switch tag if focused window is hidden
                if is_focus_event {
                    let focused_id = ctx.state.borrow().focused;
                    // Emit focus change event
                    ctx.event_emitter.emit_window_focused(focused_id);

                    if let Some(focused_id) = focused_id {
                        if notify_layout_focus(&ctx.state, &ctx.layout_engine_manager, focused_id) {
                            needs_retile = true;
                        }
                    }
                    let moves = switch_tag_for_focused_window(&ctx.state);
                    if let Some(moves) = moves {
                        ctx.window_manipulator.apply_window_moves(&moves);
                        needs_retile = true;
                    }
                }

                if ctx.event_tx.blocking_send(event).is_err() {
                    tracing::error!("Failed to forward event to tokio");
                }
            }
            if needs_retile {
                do_retile(
                    &ctx.state,
                    &ctx.layout_engine_manager,
                    &ctx.window_manipulator,
                );
            }

            // Apply pending hotkey binding changes (deferred tap recreation)
            if let Err(e) = ctx.hotkey_manager.borrow_mut().ensure_tap() {
                tracing::error!("Failed to update hotkey tap: {}", e);
            }
        }

        let timer = unsafe {
            core_foundation::runloop::CFRunLoopTimer::new(
                core_foundation::date::CFAbsoluteTimeGetCurrent(),
                0.05, // 50ms interval
                0,
                0,
                timer_callback,
                &mut timer_context,
            )
        };

        let run_loop = CFRunLoop::get_current();
        run_loop.add_timer(&timer, unsafe { kCFRunLoopDefaultMode });

        // Run init script in background thread
        std::thread::spawn(|| {
            run_init_script();
        });

        tracing::info!("Entering CFRunLoop");
        CFRunLoop::run_current();
        tracing::info!("CFRunLoop exited");
    }
}

fn build_initial_exec_path() -> String {
    let mut paths = Vec::new();

    // yashiki executable directory
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            paths.push(dir.to_string_lossy().to_string());
        }
    }

    // system PATH
    if let Ok(system_path) = std::env::var("PATH") {
        paths.push(system_path);
    }

    paths.join(":")
}

fn run_init_script() {
    let config_dir = match dirs::home_dir() {
        Some(dir) => dir.join(".config").join("yashiki"),
        None => {
            tracing::warn!("Could not determine home directory");
            return;
        }
    };

    let init_script = config_dir.join("init");
    if !init_script.exists() {
        tracing::debug!("No init script found at {:?}", init_script);
        return;
    }

    tracing::info!("Running init script: {:?}", init_script);

    // Small delay to ensure IPC server is ready
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Add yashiki's directory to PATH so init script can use `yashiki` command
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    let mut cmd = std::process::Command::new(&init_script);
    cmd.current_dir(&config_dir);

    if let Some(exe_dir) = &exe_dir {
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", exe_dir.display(), path);
        cmd.env("PATH", new_path);
        tracing::debug!("Added {:?} to PATH for init script", exe_dir);
    }

    let start = std::time::Instant::now();
    match cmd.status() {
        Ok(status) => {
            let elapsed = start.elapsed();
            if status.success() {
                tracing::info!("Init script completed in {:.2?}", elapsed);
                // Apply rules to existing windows after init script completes
                if let Ok(mut client) = crate::ipc::IpcClient::connect() {
                    match client.send(&Command::ApplyRules) {
                        Ok(_) => tracing::info!("Applied rules to existing windows"),
                        Err(e) => tracing::warn!("Failed to apply rules: {}", e),
                    }
                }
            } else {
                tracing::warn!(
                    "Init script exited with status: {} (took {:.2?})",
                    status,
                    elapsed
                );
            }
        }
        Err(e) => {
            tracing::error!("Failed to run init script: {}", e);
        }
    }
}

/// Pure function: processes a command and returns a response with effects.
/// This function does not perform any side effects - it only mutates state and computes effects.
fn process_command(
    state: &mut State,
    hotkey_manager: &mut HotkeyManager,
    cmd: &Command,
) -> CommandResult {
    match cmd {
        // Query commands - no effects
        Command::ListWindows => {
            let windows: Vec<WindowInfo> = state
                .windows
                .values()
                .map(|w| WindowInfo {
                    id: w.id,
                    pid: w.pid,
                    title: w.title.clone(),
                    app_name: w.app_name.clone(),
                    app_id: w.app_id.clone(),
                    tags: w.tags.mask(),
                    x: w.frame.x,
                    y: w.frame.y,
                    width: w.frame.width,
                    height: w.frame.height,
                    is_focused: state.focused == Some(w.id),
                    is_floating: w.is_floating,
                    is_fullscreen: w.is_fullscreen,
                })
                .collect();
            CommandResult::with_response(Response::Windows { windows })
        }
        Command::ListOutputs => {
            let outputs: Vec<OutputInfo> = state
                .displays
                .values()
                .map(|d| OutputInfo {
                    id: d.id,
                    name: d.name.clone(),
                    x: d.frame.x,
                    y: d.frame.y,
                    width: d.frame.width,
                    height: d.frame.height,
                    is_main: d.is_main,
                    visible_tags: d.visible_tags.mask(),
                    is_focused: state.focused_display == d.id,
                })
                .collect();
            CommandResult::with_response(Response::Outputs { outputs })
        }
        Command::GetState => CommandResult::with_response(Response::State {
            state: StateInfo {
                visible_tags: state.visible_tags().mask(),
                focused_window_id: state.focused,
                window_count: state.windows.len(),
                default_layout: state.default_layout.clone(),
                current_layout: state
                    .displays
                    .get(&state.focused_display)
                    .and_then(|d| d.current_layout.clone()),
            },
        }),
        Command::FocusedWindow => {
            CommandResult::with_response(Response::WindowId { id: state.focused })
        }
        Command::ListBindings => {
            let bindings: Vec<BindingInfo> = hotkey_manager
                .list_bindings()
                .into_iter()
                .map(|(key, cmd)| BindingInfo {
                    key,
                    action: format!("{:?}", cmd),
                })
                .collect();
            CommandResult::with_response(Response::Bindings { bindings })
        }

        // Tag operations - mutate state, return effects
        Command::TagView { tags, output } => {
            let display_id = match state.get_target_display(output.as_ref()) {
                Ok(id) => id,
                Err(e) => return CommandResult::error(e),
            };
            let moves = state.view_tags_on_display(*tags, display_id);
            CommandResult::ok_with_effects(vec![
                Effect::ApplyWindowMoves(moves),
                Effect::RetileDisplays(vec![display_id]),
                Effect::FocusVisibleWindowIfNeeded,
            ])
        }
        Command::TagToggle { tags, output } => {
            let display_id = match state.get_target_display(output.as_ref()) {
                Ok(id) => id,
                Err(e) => return CommandResult::error(e),
            };
            let moves = state.toggle_tags_on_display(*tags, display_id);
            CommandResult::ok_with_effects(vec![
                Effect::ApplyWindowMoves(moves),
                Effect::RetileDisplays(vec![display_id]),
                Effect::FocusVisibleWindowIfNeeded,
            ])
        }
        Command::TagViewLast => {
            let moves = state.view_tags_last();
            CommandResult::ok_with_effects(vec![
                Effect::ApplyWindowMoves(moves),
                Effect::Retile,
                Effect::FocusVisibleWindowIfNeeded,
            ])
        }
        Command::WindowMoveToTag { tags } => {
            let moves = state.move_focused_to_tags(*tags);
            CommandResult::ok_with_effects(vec![
                Effect::ApplyWindowMoves(moves),
                Effect::Retile,
                Effect::FocusVisibleWindowIfNeeded,
            ])
        }
        Command::WindowToggleTag { tags } => {
            let moves = state.toggle_focused_window_tags(*tags);
            CommandResult::ok_with_effects(vec![
                Effect::ApplyWindowMoves(moves),
                Effect::Retile,
                Effect::FocusVisibleWindowIfNeeded,
            ])
        }

        // Hotkey operations
        Command::Bind { key, action } => match hotkey_manager.bind(key, *action.clone()) {
            Ok(()) => CommandResult::ok(),
            Err(e) => CommandResult::error(e),
        },
        Command::Unbind { key } => match hotkey_manager.unbind(key) {
            Ok(()) => CommandResult::ok(),
            Err(e) => CommandResult::error(e),
        },

        // Focus operations
        Command::WindowFocus { direction } => {
            if let Some((window_id, pid)) = state.focus_window(*direction) {
                tracing::info!("Focusing window {} (pid {})", window_id, pid);
                CommandResult::ok_with_effects(vec![Effect::FocusWindow {
                    window_id,
                    pid,
                    is_output_change: false,
                }])
            } else {
                CommandResult::ok()
            }
        }
        Command::OutputFocus { direction } => {
            let result = state.focus_output(*direction);
            if let Some((window_id, pid)) = result {
                tracing::info!("Focusing output - window {} (pid {})", window_id, pid);
                CommandResult::ok_with_effects(vec![Effect::FocusWindow {
                    window_id,
                    pid,
                    is_output_change: true,
                }])
            } else {
                CommandResult::ok()
            }
        }

        // Fullscreen toggle
        Command::WindowToggleFullscreen => {
            if let Some((display_id, is_fullscreen, window_id, pid)) =
                state.toggle_focused_fullscreen()
            {
                if is_fullscreen {
                    // Going fullscreen - apply fullscreen geometry
                    CommandResult::ok_with_effects(vec![
                        Effect::ApplyFullscreen {
                            window_id,
                            pid,
                            display_id,
                        },
                        Effect::RetileDisplays(vec![display_id]),
                    ])
                } else {
                    // Exiting fullscreen - just retile (layout will recompute position)
                    CommandResult::ok_with_effects(vec![Effect::RetileDisplays(vec![display_id])])
                }
            } else {
                CommandResult::ok()
            }
        }

        // Float toggle
        Command::WindowToggleFloat => {
            if let Some((display_id, _is_floating, _window_id, _pid)) = state.toggle_focused_float()
            {
                // Just retile - window maintains current position when floating,
                // or gets positioned by layout when unfloating
                CommandResult::ok_with_effects(vec![Effect::RetileDisplays(vec![display_id])])
            } else {
                CommandResult::ok()
            }
        }

        // Window close
        Command::WindowClose => {
            if let Some(focused_id) = state.focused {
                if let Some(window) = state.windows.get(&focused_id) {
                    CommandResult::ok_with_effects(vec![Effect::CloseWindow {
                        window_id: focused_id,
                        pid: window.pid,
                    }])
                } else {
                    CommandResult::error("Focused window not found")
                }
            } else {
                CommandResult::error("No focused window")
            }
        }

        // Send to output - returns displays that need retiling
        Command::OutputSend { direction } => {
            let displays_to_retile = state.send_to_output(*direction);
            if let Some((source_display, target_display)) = displays_to_retile {
                // Get the window info for moving
                let mut effects = Vec::new();
                if let Some(focused_id) = state.focused {
                    if let Some(window) = state.windows.get(&focused_id) {
                        effects.push(Effect::MoveWindowToPosition {
                            window_id: focused_id,
                            pid: window.pid,
                            x: window.frame.x,
                            y: window.frame.y,
                        });
                    }
                }
                effects.push(Effect::RetileDisplays(vec![source_display, target_display]));
                CommandResult::ok_with_effects(effects)
            } else {
                CommandResult::ok()
            }
        }

        // Layout configuration
        Command::LayoutSetDefault { layout } => {
            state.set_default_layout(layout.clone());
            CommandResult::ok()
        }
        Command::LayoutSet {
            tags,
            output,
            layout,
        } => {
            let display_id = match state.get_target_display(output.as_ref()) {
                Ok(id) => Some(id),
                Err(e) => return CommandResult::error(e),
            };
            state.set_layout_on_display(*tags, display_id, layout.clone());
            // Only retile if setting for current tag (no tags specified)
            if tags.is_none() {
                if let Some(id) = display_id {
                    CommandResult::ok_with_effects(vec![Effect::RetileDisplays(vec![id])])
                } else {
                    CommandResult::ok_with_effects(vec![Effect::Retile])
                }
            } else {
                CommandResult::ok()
            }
        }
        Command::LayoutGet { tags, output } => {
            let display_id = match state.get_target_display(output.as_ref()) {
                Ok(id) => Some(id),
                Err(e) => return CommandResult::error(e),
            };
            let layout = state.get_layout_on_display(*tags, display_id).to_string();
            CommandResult::with_response(Response::Layout { layout })
        }

        // Layout commands - need layout engine interaction (handled as effects)
        Command::LayoutCommand { layout, cmd, args } => {
            let mut effects = vec![Effect::SendLayoutCommand {
                layout: layout.clone(),
                cmd: cmd.clone(),
                args: args.clone(),
            }];
            // Only retile if targeting current layout (layout is None)
            if layout.is_none() {
                effects.push(Effect::Retile);
            }
            CommandResult::ok_with_effects(effects)
        }
        Command::Retile { output } => {
            if let Some(ref spec) = output {
                let display_id = match state.get_target_display(Some(spec)) {
                    Ok(id) => id,
                    Err(e) => return CommandResult::error(e),
                };
                CommandResult::ok_with_effects(vec![Effect::RetileDisplays(vec![display_id])])
            } else {
                CommandResult::ok_with_effects(vec![Effect::Retile])
            }
        }

        // Exec path commands
        Command::GetExecPath => CommandResult::with_response(Response::ExecPath {
            path: state.exec_path.clone(),
        }),
        Command::SetExecPath { path } => {
            tracing::info!("Set exec path: {}", path);
            state.exec_path = path.clone();
            CommandResult::ok_with_effects(vec![Effect::UpdateLayoutExecPath {
                path: path.clone(),
            }])
        }
        Command::AddExecPath { path, append } => {
            let new_exec_path = if *append {
                if state.exec_path.is_empty() {
                    path.clone()
                } else {
                    format!("{}:{}", state.exec_path, path)
                }
            } else if state.exec_path.is_empty() {
                path.clone()
            } else {
                format!("{}:{}", path, state.exec_path)
            };
            tracing::info!("Add exec path: {} (append={})", path, append);
            state.exec_path = new_exec_path.clone();
            CommandResult::ok_with_effects(vec![Effect::UpdateLayoutExecPath {
                path: new_exec_path,
            }])
        }

        // Exec commands
        Command::Exec { command } => CommandResult::ok_with_effects(vec![Effect::ExecCommand {
            command: command.clone(),
            path: state.exec_path.clone(),
        }]),
        Command::ExecOrFocus { app_name, command } => {
            // Check if a window with the given app_name exists
            let existing_window = state
                .windows
                .values()
                .find(|w| w.app_name == *app_name)
                .map(|w| (w.id, w.pid, w.tags, w.display_id, w.is_hidden()));

            if let Some((window_id, pid, window_tags, window_display_id, is_hidden)) =
                existing_window
            {
                // Check if window is visible on its display
                let is_visible = state
                    .displays
                    .get(&window_display_id)
                    .map(|display| window_tags.intersects(display.visible_tags) && !is_hidden)
                    .unwrap_or(false);

                if is_visible {
                    tracing::info!(
                        "Focusing visible window for app '{}' (window_id={}, pid={})",
                        app_name,
                        window_id,
                        pid
                    );
                    CommandResult::ok_with_effects(vec![Effect::FocusWindow {
                        window_id,
                        pid,
                        is_output_change: false,
                    }])
                } else {
                    // Window is hidden, switch to its tag first
                    if let Some(tag) = window_tags.first_tag() {
                        tracing::info!(
                            "Switching to tag {} and focusing window for app '{}' (window_id={}, pid={})",
                            tag,
                            app_name,
                            window_id,
                            pid
                        );
                        let moves = state.view_tags(1 << (tag - 1));
                        CommandResult::ok_with_effects(vec![
                            Effect::ApplyWindowMoves(moves),
                            Effect::Retile,
                            Effect::FocusWindow {
                                window_id,
                                pid,
                                is_output_change: false,
                            },
                        ])
                    } else {
                        CommandResult::ok_with_effects(vec![Effect::FocusWindow {
                            window_id,
                            pid,
                            is_output_change: false,
                        }])
                    }
                }
            } else {
                tracing::info!(
                    "No existing window for app '{}', executing command",
                    app_name
                );
                CommandResult::ok_with_effects(vec![Effect::ExecCommand {
                    command: command.clone(),
                    path: state.exec_path.clone(),
                }])
            }
        }

        // Rules
        Command::RuleAdd { rule } => {
            state.add_rule(rule.clone());
            CommandResult::ok()
        }
        Command::RuleDel { matcher, action } => {
            if state.remove_rule(matcher, action) {
                CommandResult::ok()
            } else {
                CommandResult::error("Rule not found")
            }
        }
        Command::ListRules => {
            let rules: Vec<RuleInfo> = state
                .rules
                .iter()
                .map(|r| {
                    let action_str = match &r.action {
                        yashiki_ipc::RuleAction::Float => "float".to_string(),
                        yashiki_ipc::RuleAction::NoFloat => "no-float".to_string(),
                        yashiki_ipc::RuleAction::Tags { tags } => format!("tags {}", tags),
                        yashiki_ipc::RuleAction::Output { output } => match output {
                            yashiki_ipc::OutputSpecifier::Id(id) => format!("output {}", id),
                            yashiki_ipc::OutputSpecifier::Name(name) => {
                                format!("output {}", name)
                            }
                        },
                        yashiki_ipc::RuleAction::Position { x, y } => {
                            format!("position {} {}", x, y)
                        }
                        yashiki_ipc::RuleAction::Dimensions { width, height } => {
                            format!("dimensions {} {}", width, height)
                        }
                    };
                    RuleInfo {
                        app_name: r.matcher.app_name.as_ref().map(|p| p.pattern().to_string()),
                        app_id: r.matcher.app_id.as_ref().map(|p| p.pattern().to_string()),
                        title: r.matcher.title.as_ref().map(|p| p.pattern().to_string()),
                        action: action_str,
                    }
                })
                .collect();
            CommandResult::with_response(Response::Rules { rules })
        }
        Command::ApplyRules => {
            // Apply rules to all existing windows
            let (affected_displays, mut effects) = state.apply_rules_to_all_windows();

            // For each affected display, compute window hide/show moves
            let mut all_moves = Vec::new();
            for display_id in &affected_displays {
                let moves = state.compute_layout_changes(*display_id);
                all_moves.extend(moves);
            }

            // Prepend window moves to effects
            if !all_moves.is_empty() {
                effects.insert(0, Effect::ApplyWindowMoves(all_moves));
            }

            // Add retile for affected displays
            if !affected_displays.is_empty() {
                effects.push(Effect::RetileDisplays(affected_displays));
            }

            tracing::info!("Applied rules to all existing windows");
            CommandResult::ok_with_effects(effects)
        }

        // Cursor warp
        Command::SetCursorWarp { mode } => {
            tracing::info!("Set cursor warp mode: {:?}", mode);
            state.cursor_warp = *mode;
            CommandResult::ok()
        }
        Command::GetCursorWarp => CommandResult::with_response(Response::CursorWarp {
            mode: state.cursor_warp,
        }),

        // Outer gap
        Command::SetOuterGap { values } => match OuterGap::from_args(values) {
            Some(gap) => {
                tracing::info!("Set outer gap: {}", gap);
                state.outer_gap = gap;
                CommandResult::ok_with_effects(vec![Effect::Retile])
            }
            None => CommandResult::error("usage: set-outer-gap <all> | <v h> | <t r b l>"),
        },
        Command::GetOuterGap => CommandResult::with_response(Response::OuterGap {
            outer_gap: state.outer_gap,
        }),

        // Control
        Command::Quit => {
            tracing::info!("Quit command received");
            CommandResult::ok()
        }

        // Unhandled commands
        _ => {
            tracing::warn!("Unhandled command: {:?}", cmd);
            CommandResult::error("Command not yet implemented")
        }
    }
}

/// Execute side effects.
fn execute_effects<M: WindowManipulator>(
    effects: Vec<Effect>,
    state: &RefCell<State>,
    layout_engine_manager: &RefCell<LayoutEngineManager>,
    manipulator: &M,
) -> Result<(), String> {
    for effect in effects {
        match effect {
            Effect::ApplyWindowMoves(moves) => {
                manipulator.apply_window_moves(&moves);
            }
            Effect::FocusWindow {
                window_id,
                pid,
                is_output_change,
            } => {
                manipulator.focus_window(window_id, pid);

                // Warp cursor based on cursor_warp mode
                let cursor_warp_mode = state.borrow().cursor_warp;
                let should_warp = match cursor_warp_mode {
                    CursorWarpMode::Disabled => false,
                    CursorWarpMode::OnOutputChange => is_output_change,
                    CursorWarpMode::OnFocusChange => true,
                };

                if should_warp {
                    if let Some(window) = state.borrow().windows.get(&window_id) {
                        let (cx, cy) = window.center();
                        manipulator.warp_cursor(cx, cy);
                    }
                }

                if notify_layout_focus(state, layout_engine_manager, window_id) {
                    do_retile(state, layout_engine_manager, manipulator);
                }
            }
            Effect::MoveWindowToPosition {
                window_id,
                pid,
                x,
                y,
            } => {
                manipulator.move_window_to_position(window_id, pid, x, y);
            }
            Effect::SetWindowDimensions {
                window_id,
                pid,
                width,
                height,
            } => {
                manipulator.set_window_dimensions(window_id, pid, width, height);
            }
            Effect::CloseWindow { window_id, pid } => {
                manipulator.close_window(window_id, pid);
            }
            Effect::ApplyFullscreen {
                window_id,
                pid,
                display_id,
            } => {
                let state = state.borrow();
                let outer_gap = state.outer_gap;
                if let Some(display) = state.displays.get(&display_id) {
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
            Effect::Retile => {
                do_retile(state, layout_engine_manager, manipulator);
            }
            Effect::RetileDisplays(display_ids) => {
                for display_id in display_ids {
                    do_retile_display(state, layout_engine_manager, manipulator, display_id);
                }
            }
            Effect::SendLayoutCommand { layout, cmd, args } => {
                let layout_name = layout
                    .clone()
                    .unwrap_or_else(|| state.borrow().current_layout().to_string());
                let mut manager = layout_engine_manager.borrow_mut();
                if let Err(e) = manager.send_command(&layout_name, &cmd, &args) {
                    return Err(format!("Layout command failed: {}", e));
                }
            }
            Effect::ExecCommand { command, path } => {
                manipulator.exec_command(&command, &path)?;
            }
            Effect::UpdateLayoutExecPath { path } => {
                layout_engine_manager.borrow_mut().set_exec_path(&path);
            }
            Effect::FocusVisibleWindowIfNeeded => {
                focus_visible_window_if_needed(state, manipulator);
            }
        }
    }
    Ok(())
}

/// Main entry point for handling IPC commands.
/// This function orchestrates process_command and execute_effects.
fn handle_ipc_command<M: WindowManipulator>(
    state: &RefCell<State>,
    layout_engine_manager: &RefCell<LayoutEngineManager>,
    hotkey_manager: &RefCell<HotkeyManager>,
    manipulator: &M,
    cmd: &Command,
) -> Response {
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

fn do_retile<M: WindowManipulator>(
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

fn do_retile_display<M: WindowManipulator>(
    state: &RefCell<State>,
    layout_engine_manager: &RefCell<LayoutEngineManager>,
    manipulator: &M,
    display_id: crate::macos::DisplayId,
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
    display_id: crate::macos::DisplayId,
) {
    // First, handle any fullscreen windows on this display
    {
        let state = state.borrow();
        let outer_gap = state.outer_gap;
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
        let outer_gap = state.outer_gap;
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

fn focus_visible_window_if_needed<M: WindowManipulator>(state: &RefCell<State>, manipulator: &M) {
    let (window_to_focus, cursor_warp_mode) = {
        let state = state.borrow();
        let display_id = state.focused_display;
        let Some(display) = state.displays.get(&display_id) else {
            return;
        };

        // Get all visible windows on display (including fullscreen and floating)
        let all_visible: Vec<_> = state
            .windows
            .values()
            .filter(|w| {
                w.display_id == display_id
                    && w.tags.intersects(display.visible_tags)
                    && !w.is_hidden()
            })
            .collect();

        if all_visible.is_empty() {
            return;
        }

        // Check if current focus is on a visible window
        let focus_is_visible = state
            .focused
            .map(|id| all_visible.iter().any(|w| w.id == id))
            .unwrap_or(false);

        if focus_is_visible {
            return;
        }

        // Focus the first visible window (prefer tiled, then fullscreen, then floating)
        let window = all_visible
            .iter()
            .find(|w| w.is_tiled())
            .or_else(|| all_visible.iter().find(|w| w.is_fullscreen))
            .or_else(|| all_visible.first());

        match window {
            Some(w) => (Some((w.id, w.pid, w.center())), state.cursor_warp),
            None => return,
        }
    };

    if let Some((window_id, pid, (cx, cy))) = window_to_focus {
        tracing::info!("Focusing visible window {} after tag switch", window_id);
        manipulator.focus_window(window_id, pid);

        // Warp cursor if OnFocusChange mode (not OnOutputChange since this is not an output change)
        if cursor_warp_mode == CursorWarpMode::OnFocusChange {
            manipulator.warp_cursor(cx, cy);
        }
    }
}

fn switch_tag_for_focused_window(state: &RefCell<State>) -> Option<Vec<WindowMove>> {
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
fn notify_layout_focus(
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

/// Window properties tracked for change detection
#[derive(Clone, PartialEq)]
struct WindowProperties {
    tags: u32,
    display_id: u32,
    is_floating: bool,
    is_fullscreen: bool,
}

/// State captured before command execution for event comparison
struct PreEventState {
    /// Map of display_id to (visible_tags, current_layout)
    displays: std::collections::HashMap<u32, (u32, Option<String>)>,
    /// Map of window_id to tracked properties
    windows: std::collections::HashMap<u32, WindowProperties>,
    focused: Option<u32>,
    focused_display: u32,
}

/// Capture relevant state for event emission comparison
fn capture_event_state(state: &RefCell<State>) -> PreEventState {
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
fn emit_state_change_events(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::Effect;
    use crate::platform::mock::{create_test_display, create_test_window, MockWindowSystem};
    use yashiki_ipc::{Command, Direction, Response};

    fn setup_state() -> (State, HotkeyManager) {
        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![
                create_test_window(100, 1000, "Safari", 0.0, 0.0, 960.0, 1080.0),
                create_test_window(101, 1001, "Terminal", 960.0, 0.0, 960.0, 1080.0),
                create_test_window(102, 1002, "VSCode", 0.0, 0.0, 960.0, 540.0),
            ])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws);

        let (tx, _rx) = std_mpsc::channel();
        let hotkey_manager = HotkeyManager::new(tx);

        (state, hotkey_manager)
    }

    #[test]
    fn test_query_commands_have_no_effects() {
        let (mut state, mut hotkey_manager) = setup_state();

        // ListWindows
        let result = process_command(&mut state, &mut hotkey_manager, &Command::ListWindows);
        assert!(result.effects.is_empty());
        assert!(matches!(result.response, Response::Windows { .. }));

        // GetState
        let result = process_command(&mut state, &mut hotkey_manager, &Command::GetState);
        assert!(result.effects.is_empty());
        assert!(matches!(result.response, Response::State { .. }));

        // FocusedWindow
        let result = process_command(&mut state, &mut hotkey_manager, &Command::FocusedWindow);
        assert!(result.effects.is_empty());
        assert!(matches!(result.response, Response::WindowId { .. }));

        // ListBindings
        let result = process_command(&mut state, &mut hotkey_manager, &Command::ListBindings);
        assert!(result.effects.is_empty());
        assert!(matches!(result.response, Response::Bindings { .. }));
    }

    #[test]
    fn test_tag_view_produces_correct_effects() {
        let (mut state, mut hotkey_manager) = setup_state();

        let result = process_command(
            &mut state,
            &mut hotkey_manager,
            &Command::TagView {
                tags: 0b10,
                output: None,
            },
        );

        assert!(matches!(result.response, Response::Ok));
        assert_eq!(result.effects.len(), 3);

        // Should have ApplyWindowMoves, RetileDisplays, FocusVisibleWindowIfNeeded
        assert!(matches!(result.effects[0], Effect::ApplyWindowMoves(_)));
        assert!(matches!(result.effects[1], Effect::RetileDisplays(_)));
        assert!(matches!(
            result.effects[2],
            Effect::FocusVisibleWindowIfNeeded
        ));
    }

    #[test]
    fn test_window_focus_produces_focus_effect() {
        let (mut state, mut hotkey_manager) = setup_state();

        let result = process_command(
            &mut state,
            &mut hotkey_manager,
            &Command::WindowFocus {
                direction: Direction::Next,
            },
        );

        assert!(matches!(result.response, Response::Ok));
        assert_eq!(result.effects.len(), 1);

        match &result.effects[0] {
            Effect::FocusWindow {
                window_id,
                pid,
                is_output_change,
            } => {
                assert_eq!(*window_id, 101); // Next window after 100
                assert_eq!(*pid, 1001);
                assert!(!is_output_change);
            }
            _ => panic!("Expected FocusWindow effect"),
        }
    }

    #[test]
    fn test_exec_produces_exec_effect() {
        let (mut state, mut hotkey_manager) = setup_state();

        let result = process_command(
            &mut state,
            &mut hotkey_manager,
            &Command::Exec {
                command: "open -a Safari".to_string(),
            },
        );

        assert!(matches!(result.response, Response::Ok));
        assert_eq!(result.effects.len(), 1);

        match &result.effects[0] {
            Effect::ExecCommand { command, .. } => {
                assert_eq!(command, "open -a Safari");
            }
            _ => panic!("Expected ExecCommand effect"),
        }
    }

    #[test]
    fn test_exec_or_focus_existing_window_focuses() {
        let (mut state, mut hotkey_manager) = setup_state();

        let result = process_command(
            &mut state,
            &mut hotkey_manager,
            &Command::ExecOrFocus {
                app_name: "Safari".to_string(),
                command: "open -a Safari".to_string(),
            },
        );

        assert!(matches!(result.response, Response::Ok));
        assert_eq!(result.effects.len(), 1);

        // Should focus the existing Safari window, not execute command
        match &result.effects[0] {
            Effect::FocusWindow {
                window_id,
                pid,
                is_output_change,
            } => {
                assert_eq!(*window_id, 100); // Safari window
                assert_eq!(*pid, 1000);
                assert!(!is_output_change);
            }
            _ => panic!("Expected FocusWindow effect, got {:?}", result.effects[0]),
        }
    }

    #[test]
    fn test_exec_or_focus_new_app_executes() {
        let (mut state, mut hotkey_manager) = setup_state();

        let result = process_command(
            &mut state,
            &mut hotkey_manager,
            &Command::ExecOrFocus {
                app_name: "Slack".to_string(), // App not in our mock windows
                command: "open -a Slack".to_string(),
            },
        );

        assert!(matches!(result.response, Response::Ok));
        assert_eq!(result.effects.len(), 1);

        // Should execute command since Slack is not running
        match &result.effects[0] {
            Effect::ExecCommand { command, .. } => {
                assert_eq!(command, "open -a Slack");
            }
            _ => panic!("Expected ExecCommand effect, got {:?}", result.effects[0]),
        }
    }

    #[test]
    fn test_layout_command_produces_send_and_retile() {
        let (mut state, mut hotkey_manager) = setup_state();

        // Without layout option - should retile
        let result = process_command(
            &mut state,
            &mut hotkey_manager,
            &Command::LayoutCommand {
                layout: None,
                cmd: "set-main-ratio".to_string(),
                args: vec!["0.6".to_string()],
            },
        );

        assert!(matches!(result.response, Response::Ok));
        assert_eq!(result.effects.len(), 2);

        match &result.effects[0] {
            Effect::SendLayoutCommand { layout, cmd, args } => {
                assert_eq!(*layout, None);
                assert_eq!(cmd, "set-main-ratio");
                assert_eq!(args, &vec!["0.6".to_string()]);
            }
            _ => panic!("Expected SendLayoutCommand effect"),
        }
        assert!(matches!(result.effects[1], Effect::Retile));

        // With layout option - should not retile
        let result = process_command(
            &mut state,
            &mut hotkey_manager,
            &Command::LayoutCommand {
                layout: Some("tatami".to_string()),
                cmd: "set-outer-gap".to_string(),
                args: vec!["10".to_string()],
            },
        );

        assert!(matches!(result.response, Response::Ok));
        assert_eq!(result.effects.len(), 1);

        match &result.effects[0] {
            Effect::SendLayoutCommand { layout, cmd, args } => {
                assert_eq!(*layout, Some("tatami".to_string()));
                assert_eq!(cmd, "set-outer-gap");
                assert_eq!(args, &vec!["10".to_string()]);
            }
            _ => panic!("Expected SendLayoutCommand effect"),
        }
    }

    #[test]
    fn test_retile_produces_retile_effect() {
        let (mut state, mut hotkey_manager) = setup_state();

        let result = process_command(
            &mut state,
            &mut hotkey_manager,
            &Command::Retile { output: None },
        );

        assert!(matches!(result.response, Response::Ok));
        assert_eq!(result.effects.len(), 1);
        assert!(matches!(result.effects[0], Effect::Retile));
    }

    #[test]
    fn test_quit_has_no_effects() {
        let (mut state, mut hotkey_manager) = setup_state();

        let result = process_command(&mut state, &mut hotkey_manager, &Command::Quit);

        assert!(matches!(result.response, Response::Ok));
        assert!(result.effects.is_empty());
    }

    #[test]
    fn test_window_property_change_detection_tags() {
        use crate::event_emitter::EventEmitter;
        use std::cell::RefCell;
        use std::sync::mpsc as std_mpsc;
        use yashiki_ipc::StateEvent;

        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 0.0, 0.0, 800.0, 600.0,
            )])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws);

        let state_cell = RefCell::new(state);
        let (tx, rx) = std_mpsc::channel::<StateEvent>();
        let event_emitter = EventEmitter::new(tx);

        // Capture initial state
        let pre = capture_event_state(&state_cell);

        // Modify window tags
        state_cell.borrow_mut().windows.get_mut(&100).unwrap().tags =
            crate::core::Tag::from_mask(0b10);

        // Emit events
        emit_state_change_events(&event_emitter, &state_cell, &pre);

        // Verify WindowUpdated event was emitted
        let events: Vec<_> = rx.try_iter().collect();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], StateEvent::WindowUpdated { .. }));
    }

    #[test]
    fn test_window_property_change_detection_floating() {
        use crate::event_emitter::EventEmitter;
        use std::cell::RefCell;
        use std::sync::mpsc as std_mpsc;
        use yashiki_ipc::StateEvent;

        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 0.0, 0.0, 800.0, 600.0,
            )])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws);

        let state_cell = RefCell::new(state);
        let (tx, rx) = std_mpsc::channel::<StateEvent>();
        let event_emitter = EventEmitter::new(tx);

        // Capture initial state
        let pre = capture_event_state(&state_cell);

        // Modify window is_floating
        state_cell
            .borrow_mut()
            .windows
            .get_mut(&100)
            .unwrap()
            .is_floating = true;

        // Emit events
        emit_state_change_events(&event_emitter, &state_cell, &pre);

        // Verify WindowUpdated event was emitted
        let events: Vec<_> = rx.try_iter().collect();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], StateEvent::WindowUpdated { .. }));
    }

    #[test]
    fn test_window_property_no_change_no_event() {
        use crate::event_emitter::EventEmitter;
        use std::cell::RefCell;
        use std::sync::mpsc as std_mpsc;
        use yashiki_ipc::StateEvent;

        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 0.0, 0.0, 800.0, 600.0,
            )])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws);

        let state_cell = RefCell::new(state);
        let (tx, rx) = std_mpsc::channel::<StateEvent>();
        let event_emitter = EventEmitter::new(tx);

        // Capture initial state
        let pre = capture_event_state(&state_cell);

        // No modifications to state

        // Emit events
        emit_state_change_events(&event_emitter, &state_cell, &pre);

        // Verify no events were emitted
        let events: Vec<_> = rx.try_iter().collect();
        assert!(events.is_empty());
    }

    #[test]
    fn test_emit_focus_change_detection() {
        use crate::event_emitter::EventEmitter;
        use std::cell::RefCell;
        use std::sync::mpsc as std_mpsc;
        use yashiki_ipc::StateEvent;

        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![
                create_test_window(100, 1000, "Safari", 0.0, 0.0, 800.0, 600.0),
                create_test_window(101, 1001, "Terminal", 800.0, 0.0, 800.0, 600.0),
            ])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws);

        let state_cell = RefCell::new(state);
        let (tx, rx) = std_mpsc::channel::<StateEvent>();
        let event_emitter = EventEmitter::new(tx);

        // Capture initial state (focused = 100)
        let pre = capture_event_state(&state_cell);

        // Change focused window
        state_cell.borrow_mut().focused = Some(101);

        // Emit events
        emit_state_change_events(&event_emitter, &state_cell, &pre);

        // Verify WindowFocused event was emitted
        let events: Vec<_> = rx.try_iter().collect();
        assert_eq!(events.len(), 1);
        match &events[0] {
            StateEvent::WindowFocused { window_id } => {
                assert_eq!(*window_id, Some(101));
            }
            _ => panic!("Expected WindowFocused event, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_emit_display_focus_change_detection() {
        use crate::event_emitter::EventEmitter;
        use std::cell::RefCell;
        use std::sync::mpsc as std_mpsc;
        use yashiki_ipc::StateEvent;

        let ws = MockWindowSystem::new()
            .with_displays(vec![
                create_test_display(1, 0.0, 0.0, 1920.0, 1080.0),
                create_test_display(2, 1920.0, 0.0, 1920.0, 1080.0),
            ])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 0.0, 0.0, 800.0, 600.0,
            )])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws);

        let state_cell = RefCell::new(state);
        let (tx, rx) = std_mpsc::channel::<StateEvent>();
        let event_emitter = EventEmitter::new(tx);

        // Capture initial state (focused_display = 1)
        let pre = capture_event_state(&state_cell);
        assert_eq!(pre.focused_display, 1);

        // Change focused display
        state_cell.borrow_mut().focused_display = 2;

        // Emit events
        emit_state_change_events(&event_emitter, &state_cell, &pre);

        // Verify DisplayFocused event was emitted
        let events: Vec<_> = rx.try_iter().collect();
        assert_eq!(events.len(), 1);
        match &events[0] {
            StateEvent::DisplayFocused { display_id } => {
                assert_eq!(*display_id, 2);
            }
            _ => panic!("Expected DisplayFocused event, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_emit_tags_changed_detection() {
        use crate::event_emitter::EventEmitter;
        use std::cell::RefCell;
        use std::sync::mpsc as std_mpsc;
        use yashiki_ipc::StateEvent;

        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 0.0, 0.0, 800.0, 600.0,
            )])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws);

        let state_cell = RefCell::new(state);
        let (tx, rx) = std_mpsc::channel::<StateEvent>();
        let event_emitter = EventEmitter::new(tx);

        // Capture initial state (visible_tags = 1 on display 1)
        let pre = capture_event_state(&state_cell);

        // Change visible tags on display
        state_cell
            .borrow_mut()
            .displays
            .get_mut(&1)
            .unwrap()
            .visible_tags = crate::core::Tag::from_mask(0b10); // Tag 2

        // Emit events
        emit_state_change_events(&event_emitter, &state_cell, &pre);

        // Verify TagsChanged event was emitted
        let events: Vec<_> = rx.try_iter().collect();
        assert_eq!(events.len(), 1);
        match &events[0] {
            StateEvent::TagsChanged {
                display_id,
                visible_tags,
                previous_tags,
            } => {
                assert_eq!(*display_id, 1);
                assert_eq!(*visible_tags, 0b10);
                assert_eq!(*previous_tags, 1);
            }
            _ => panic!("Expected TagsChanged event, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_emit_layout_changed_detection() {
        use crate::event_emitter::EventEmitter;
        use std::cell::RefCell;
        use std::sync::mpsc as std_mpsc;
        use yashiki_ipc::StateEvent;

        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 0.0, 0.0, 800.0, 600.0,
            )])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws);

        let state_cell = RefCell::new(state);
        let (tx, rx) = std_mpsc::channel::<StateEvent>();
        let event_emitter = EventEmitter::new(tx);

        // Capture initial state (current_layout = None on display 1)
        let pre = capture_event_state(&state_cell);

        // Change layout on display
        state_cell
            .borrow_mut()
            .displays
            .get_mut(&1)
            .unwrap()
            .current_layout = Some("byobu".to_string());

        // Emit events
        emit_state_change_events(&event_emitter, &state_cell, &pre);

        // Verify LayoutChanged event was emitted
        let events: Vec<_> = rx.try_iter().collect();
        assert_eq!(events.len(), 1);
        match &events[0] {
            StateEvent::LayoutChanged { display_id, layout } => {
                assert_eq!(*display_id, 1);
                assert_eq!(layout, "byobu");
            }
            _ => panic!("Expected LayoutChanged event, got {:?}", events[0]),
        }
    }
}
