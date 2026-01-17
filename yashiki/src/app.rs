use crate::core::{Rect, State, WindowMove};
use crate::event::Event;
use crate::ipc::IpcServer;
use crate::layout::LayoutEngine;
use crate::macos;
use crate::macos::{
    activate_application, AXUIElement, HotkeyManager, ObserverManager, WorkspaceEvent,
    WorkspaceWatcher,
};
use crate::pid;
use crate::platform::MacOSWindowSystem;
use anyhow::Result;
use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoop};
use core_graphics::geometry::{CGPoint, CGSize};
use objc2_foundation::MainThreadMarker;
use std::cell::RefCell;
use std::sync::mpsc as std_mpsc;
use tokio::sync::mpsc;
use yashiki_ipc::{BindingInfo, Command, Response, StateInfo, WindowGeometry, WindowInfo};

type IpcCommandWithResponse = (Command, mpsc::Sender<Response>);

struct RunLoopContext {
    ipc_cmd_rx: std_mpsc::Receiver<IpcCommandWithResponse>,
    hotkey_cmd_rx: std_mpsc::Receiver<Command>,
    observer_event_rx: std_mpsc::Receiver<Event>,
    workspace_event_rx: std_mpsc::Receiver<WorkspaceEvent>,
    event_tx: mpsc::Sender<Event>,
    observer_manager: RefCell<ObserverManager>,
    state: RefCell<State>,
    layout_engine: RefCell<Option<LayoutEngine>>,
    hotkey_manager: RefCell<HotkeyManager>,
    window_system: MacOSWindowSystem,
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

        // Channel: IPC commands (tokio -> main thread)
        let (ipc_cmd_tx, ipc_cmd_rx) = std_mpsc::channel::<IpcCommandWithResponse>();

        // Channel: observer -> main thread
        let (observer_event_tx, observer_event_rx) = std_mpsc::channel::<Event>();

        // Channel: main thread -> tokio
        let (event_tx, event_rx) = mpsc::channel::<Event>(256);

        // Channel for IPC server (tokio internal)
        let (ipc_tx, ipc_rx) = mpsc::channel::<IpcCommandWithResponse>(256);

        // Spawn tokio runtime in separate thread
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                Self::run_async(ipc_cmd_tx, ipc_tx, ipc_rx, event_rx).await;
            });
        });

        let app = App {};
        app.run_main_loop(ipc_cmd_rx, observer_event_tx, observer_event_rx, event_tx);

        // Clean up PID file on exit
        pid::remove_pid();
        Ok(())
    }

    async fn run_async(
        ipc_cmd_tx: std_mpsc::Sender<IpcCommandWithResponse>,
        ipc_server_tx: mpsc::Sender<IpcCommandWithResponse>,
        mut ipc_rx: mpsc::Receiver<IpcCommandWithResponse>,
        mut event_rx: mpsc::Receiver<Event>,
    ) {
        tracing::info!("Tokio runtime started");

        // Start IPC server
        let ipc_server = IpcServer::new(ipc_server_tx);
        tokio::spawn(async move {
            if let Err(e) = ipc_server.run().await {
                tracing::error!("IPC server error: {}", e);
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
                }
                Some(event) = event_rx.recv() => {
                    tracing::debug!("Received event: {:?}", event);
                }
                else => break,
            }
        }

        tracing::info!("Tokio runtime exiting");
    }

    fn run_main_loop(
        self,
        ipc_cmd_rx: std_mpsc::Receiver<IpcCommandWithResponse>,
        observer_event_tx: std_mpsc::Sender<Event>,
        observer_event_rx: std_mpsc::Receiver<Event>,
        event_tx: mpsc::Sender<Event>,
    ) {
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
        let window_system = MacOSWindowSystem::default();
        let mut state = State::new();
        state.sync_all(&window_system);
        let state = RefCell::new(state);

        // Spawn layout engine
        let mut layout_engine = match LayoutEngine::spawn("tatami") {
            Ok(engine) => Some(engine),
            Err(e) => {
                tracing::warn!("Failed to spawn layout engine: {}", e);
                None
            }
        };

        // Initial retile
        do_retile(&state, &mut layout_engine);

        // Create hotkey manager
        let (hotkey_cmd_tx, hotkey_cmd_rx) = std_mpsc::channel::<Command>();
        let mut hotkey_manager = HotkeyManager::new(hotkey_cmd_tx);

        // Start hotkey tap (initially with no bindings, will be updated via IPC)
        if let Err(e) = hotkey_manager.start() {
            tracing::warn!("Failed to start hotkey tap: {}", e);
        }

        // Set up a timer to check for commands and events periodically
        let context = Box::new(RunLoopContext {
            ipc_cmd_rx,
            hotkey_cmd_rx,
            observer_event_rx,
            workspace_event_rx,
            event_tx,
            observer_manager: RefCell::new(observer_manager),
            state,
            layout_engine: RefCell::new(layout_engine),
            hotkey_manager: RefCell::new(hotkey_manager),
            window_system,
        });
        let mut timer_context = core_foundation::runloop::CFRunLoopTimerContext {
            version: 0,
            info: Box::into_raw(context) as *mut _,
            retain: None,
            release: None,
            copyDescription: None,
        };

        extern "C" fn timer_callback(
            _timer: core_foundation::runloop::CFRunLoopTimerRef,
            info: *mut std::ffi::c_void,
        ) {
            let ctx = unsafe { &*(info as *const RunLoopContext) };

            // Process IPC commands
            while let Ok((cmd, resp_tx)) = ctx.ipc_cmd_rx.try_recv() {
                tracing::debug!("Received IPC command: {:?}", cmd);
                let response =
                    handle_ipc_command(&ctx.state, &ctx.layout_engine, &ctx.hotkey_manager, &cmd);
                let _ = resp_tx.blocking_send(response);

                // Handle Quit command after sending response
                if matches!(cmd, Command::Quit) {
                    CFRunLoop::get_current().stop();
                }
            }

            // Process hotkey commands (no response needed)
            while let Ok(cmd) = ctx.hotkey_cmd_rx.try_recv() {
                tracing::debug!("Received hotkey command: {:?}", cmd);
                let _ =
                    handle_ipc_command(&ctx.state, &ctx.layout_engine, &ctx.hotkey_manager, &cmd);
            }

            // Process workspace events (app launch/terminate)
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
                        // Remove windows belonging to this PID from state
                        if ctx.state.borrow_mut().sync_pid(&ctx.window_system, pid) {
                            do_retile(&ctx.state, &mut ctx.layout_engine.borrow_mut());
                        }
                    }
                }
            }

            // Process observer events and forward to tokio
            let mut needs_retile = false;
            while let Ok(event) = ctx.observer_event_rx.try_recv() {
                let is_focus_event = matches!(
                    event,
                    Event::FocusedWindowChanged { .. } | Event::ApplicationActivated { .. }
                );

                if ctx
                    .state
                    .borrow_mut()
                    .handle_event(&ctx.window_system, &event)
                {
                    needs_retile = true;
                }

                // On external focus change, switch tag if focused window is hidden
                if is_focus_event {
                    if let Some(moves) = switch_tag_for_focused_window(
                        &ctx.state,
                        &mut ctx.layout_engine.borrow_mut(),
                    ) {
                        apply_window_moves(&moves);
                        needs_retile = true;
                    }
                }

                if ctx.event_tx.blocking_send(event).is_err() {
                    tracing::error!("Failed to forward event to tokio");
                }
            }
            if needs_retile {
                do_retile(&ctx.state, &mut ctx.layout_engine.borrow_mut());
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

    match std::process::Command::new(&init_script)
        .current_dir(&config_dir)
        .status()
    {
        Ok(status) => {
            if status.success() {
                tracing::info!("Init script completed successfully");
            } else {
                tracing::warn!("Init script exited with status: {}", status);
            }
        }
        Err(e) => {
            tracing::error!("Failed to run init script: {}", e);
        }
    }
}

fn handle_ipc_command(
    state: &RefCell<State>,
    layout_engine: &RefCell<Option<LayoutEngine>>,
    hotkey_manager: &RefCell<HotkeyManager>,
    cmd: &Command,
) -> Response {
    match cmd {
        Command::ListWindows => {
            let state = state.borrow();
            let windows: Vec<WindowInfo> = state
                .windows
                .values()
                .map(|w| WindowInfo {
                    id: w.id,
                    pid: w.pid,
                    title: w.title.clone(),
                    app_name: w.app_name.clone(),
                    tags: w.tags.mask(),
                    x: w.frame.x,
                    y: w.frame.y,
                    width: w.frame.width,
                    height: w.frame.height,
                    is_focused: state.focused == Some(w.id),
                })
                .collect();
            Response::Windows { windows }
        }
        Command::GetState => {
            let state = state.borrow();
            Response::State {
                state: StateInfo {
                    visible_tags: state.visible_tags().mask(),
                    focused_window_id: state.focused,
                    window_count: state.windows.len(),
                },
            }
        }
        Command::FocusedWindow => Response::WindowId {
            id: state.borrow().focused,
        },
        Command::ViewTag { tag } => {
            let moves = state.borrow_mut().view_tag(*tag);
            apply_window_moves(&moves);
            // Retile after switching tag to layout shown windows
            do_retile(&state, &mut layout_engine.borrow_mut());
            // Focus a visible window if none is focused
            focus_visible_window_if_needed(&state);
            Response::Ok
        }
        Command::ToggleViewTag { tag } => {
            let moves = state.borrow_mut().toggle_view_tag(*tag);
            apply_window_moves(&moves);
            // Retile after toggling tag
            do_retile(&state, &mut layout_engine.borrow_mut());
            // Focus a visible window if none is focused
            focus_visible_window_if_needed(&state);
            Response::Ok
        }
        Command::ViewTagLast => {
            let moves = state.borrow_mut().view_tag_last();
            apply_window_moves(&moves);
            do_retile(&state, &mut layout_engine.borrow_mut());
            focus_visible_window_if_needed(&state);
            Response::Ok
        }
        Command::MoveToTag { tag } => {
            let moves = state.borrow_mut().move_focused_to_tag(*tag);
            apply_window_moves(&moves);
            // Retile after moving window to tag
            do_retile(&state, &mut layout_engine.borrow_mut());
            // Focus a visible window since the moved window may now be hidden
            focus_visible_window_if_needed(&state);
            Response::Ok
        }
        Command::ToggleWindowTag { tag } => {
            let moves = state.borrow_mut().toggle_focused_window_tag(*tag);
            apply_window_moves(&moves);
            // Retile after toggling window tag
            do_retile(&state, &mut layout_engine.borrow_mut());
            // Focus a visible window if needed
            focus_visible_window_if_needed(&state);
            Response::Ok
        }
        Command::LayoutCommand { cmd, args } => {
            let result = {
                let mut engine = layout_engine.borrow_mut();
                if let Some(ref mut engine) = *engine {
                    match engine.send_command(cmd, args) {
                        Ok(()) => Ok(()),
                        Err(e) => Err(format!("Layout command failed: {}", e)),
                    }
                } else {
                    Err("No layout engine available".to_string())
                }
            };
            match result {
                Ok(()) => {
                    // Retile to apply layout changes
                    do_retile(&state, &mut layout_engine.borrow_mut());
                    Response::Ok
                }
                Err(message) => Response::Error { message },
            }
        }
        Command::Retile => {
            do_retile(&state, &mut layout_engine.borrow_mut());
            Response::Ok
        }
        Command::Bind { key, action } => {
            let mut manager = hotkey_manager.borrow_mut();
            match manager.bind(key, *action.clone()) {
                Ok(()) => Response::Ok,
                Err(e) => Response::Error { message: e },
            }
        }
        Command::Unbind { key } => {
            let mut manager = hotkey_manager.borrow_mut();
            match manager.unbind(key) {
                Ok(()) => Response::Ok,
                Err(e) => Response::Error { message: e },
            }
        }
        Command::ListBindings => {
            let manager = hotkey_manager.borrow();
            let bindings: Vec<BindingInfo> = manager
                .list_bindings()
                .into_iter()
                .map(|(key, cmd)| BindingInfo {
                    key,
                    action: format!("{:?}", cmd),
                })
                .collect();
            Response::Bindings { bindings }
        }
        Command::FocusWindow { direction } => {
            let state = state.borrow();
            if let Some((window_id, pid)) = state.focus_window(*direction) {
                tracing::info!("Focusing window {} (pid {})", window_id, pid);
                focus_window_by_id(&state, window_id, pid);
            }
            Response::Ok
        }
        Command::Zoom => {
            let focused_id = state.borrow().focused;
            if let Some(window_id) = focused_id {
                let result = {
                    let mut engine = layout_engine.borrow_mut();
                    if let Some(ref mut engine) = *engine {
                        engine.send_command("zoom", &[window_id.to_string()])
                    } else {
                        Err(anyhow::anyhow!("No layout engine"))
                    }
                };
                match result {
                    Ok(()) => {
                        do_retile(&state, &mut layout_engine.borrow_mut());
                        Response::Ok
                    }
                    Err(e) => Response::Error {
                        message: e.to_string(),
                    },
                }
            } else {
                Response::Error {
                    message: "No focused window".to_string(),
                }
            }
        }
        Command::FocusOutput { direction } => {
            let result = state.borrow_mut().focus_output(*direction);
            if let Some((window_id, pid)) = result {
                tracing::info!("Focusing output - window {} (pid {})", window_id, pid);
                focus_window_by_id(&state.borrow(), window_id, pid);
            }
            Response::Ok
        }
        Command::SendToOutput { direction } => {
            let displays_to_retile = state.borrow_mut().send_to_output(*direction);
            if let Some((source_display, target_display)) = displays_to_retile {
                // Move the focused window physically
                {
                    let s = state.borrow();
                    if let Some(focused_id) = s.focused {
                        if let Some(window) = s.windows.get(&focused_id) {
                            move_window_to_position(
                                window.pid,
                                focused_id,
                                &s,
                                window.frame.x,
                                window.frame.y,
                            );
                        }
                    }
                }
                // Retile both displays
                do_retile_display(state, &mut layout_engine.borrow_mut(), source_display);
                do_retile_display(state, &mut layout_engine.borrow_mut(), target_display);
            }
            Response::Ok
        }
        Command::Quit => {
            tracing::info!("Quit command received");
            Response::Ok
        }
        Command::Exec { command } => match macos::exec_command(command) {
            Ok(()) => Response::Ok,
            Err(message) => Response::Error { message },
        },
        Command::ExecOrFocus { app_name, command } => {
            // Check if a window with the given app_name exists
            let existing_window = {
                let s = state.borrow();
                s.windows
                    .values()
                    .find(|w| w.app_name == *app_name)
                    .map(|w| (w.id, w.pid, w.tags, w.display_id, w.is_hidden()))
            };

            if let Some((window_id, pid, window_tags, window_display_id, is_hidden)) =
                existing_window
            {
                // Check if window is visible on its display
                let is_visible = {
                    let s = state.borrow();
                    if let Some(display) = s.displays.get(&window_display_id) {
                        window_tags.intersects(display.visible_tags) && !is_hidden
                    } else {
                        false
                    }
                };

                if is_visible {
                    tracing::info!(
                        "Focusing visible window for app '{}' (window_id={}, pid={})",
                        app_name,
                        window_id,
                        pid
                    );
                    focus_window_by_id(&state.borrow(), window_id, pid);
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
                        let moves = state.borrow_mut().view_tag(tag);
                        apply_window_moves(&moves);
                        do_retile(&state, &mut layout_engine.borrow_mut());
                    }
                    focus_window_by_id(&state.borrow(), window_id, pid);
                }
                Response::Ok
            } else {
                tracing::info!(
                    "No existing window for app '{}', executing command",
                    app_name
                );
                match macos::exec_command(command) {
                    Ok(()) => Response::Ok,
                    Err(message) => Response::Error { message },
                }
            }
        }
        _ => {
            tracing::warn!("Unhandled command: {:?}", cmd);
            Response::Error {
                message: "Command not yet implemented".to_string(),
            }
        }
    }
}

fn do_retile(state: &RefCell<State>, layout_engine: &mut Option<LayoutEngine>) {
    let Some(ref mut engine) = layout_engine else {
        return;
    };

    // Collect display IDs first to avoid borrow issues
    let display_ids: Vec<_> = state.borrow().displays.keys().copied().collect();

    for display_id in display_ids {
        retile_single_display(state, engine, display_id);
    }
}

fn do_retile_display(
    state: &RefCell<State>,
    layout_engine: &mut Option<LayoutEngine>,
    display_id: crate::macos::DisplayId,
) {
    let Some(ref mut engine) = layout_engine else {
        return;
    };
    if !state.borrow().displays.contains_key(&display_id) {
        return;
    }
    retile_single_display(state, engine, display_id);
}

fn retile_single_display(
    state: &RefCell<State>,
    engine: &mut LayoutEngine,
    display_id: crate::macos::DisplayId,
) {
    // Get layout parameters with immutable borrow
    let (window_ids, width, height, display_frame) = {
        let state = state.borrow();
        let Some(display) = state.displays.get(&display_id) else {
            return;
        };
        let visible_windows = state.visible_windows_on_display(display_id);
        if visible_windows.is_empty() {
            return;
        }
        let window_ids: Vec<u32> = visible_windows.iter().map(|w| w.id).collect();
        (
            window_ids,
            display.frame.width,
            display.frame.height,
            display.frame,
        )
    };

    match engine.request_layout(width, height, &window_ids) {
        Ok(geometries) => {
            // Update window_order based on geometries order from layout engine
            {
                let mut state = state.borrow_mut();
                if let Some(display) = state.displays.get_mut(&display_id) {
                    display.window_order = geometries.iter().map(|g| g.id).collect();
                }
            }
            // Apply layout
            let state = state.borrow();
            apply_layout_on_display(&state, display_id, &display_frame, &geometries);
        }
        Err(e) => {
            tracing::error!("Layout request failed for display {}: {}", display_id, e);
        }
    }
}

fn move_window_to_position(pid: i32, window_id: u32, _state: &State, x: i32, y: i32) {
    let app = AXUIElement::application(pid);
    let ax_windows = match app.windows() {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!("Failed to get windows for pid {}: {}", pid, e);
            return;
        }
    };

    // Find matching AX window by window_id
    for ax_win in &ax_windows {
        if let Some(wid) = ax_win.window_id() {
            if wid == window_id {
                let new_pos = CGPoint::new(x as f64, y as f64);
                if let Err(e) = ax_win.set_position(new_pos) {
                    tracing::warn!(
                        "Failed to move window {} to ({}, {}): {}",
                        window_id,
                        x,
                        y,
                        e
                    );
                } else {
                    tracing::info!("Moved window {} to ({}, {})", window_id, x, y);
                }
                return;
            }
        }
    }

    tracing::warn!(
        "Could not find AX window for id {} (pid {})",
        window_id,
        pid
    );
}

fn focus_window_by_id(_state: &State, window_id: u32, pid: i32) {
    // Activate the application first
    activate_application(pid);

    // Get AX windows and find the matching one by window_id
    let app = AXUIElement::application(pid);
    let ax_windows = match app.windows() {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!("Failed to get windows for pid {}: {}", pid, e);
            return;
        }
    };

    for ax_win in &ax_windows {
        if let Some(wid) = ax_win.window_id() {
            if wid == window_id {
                if let Err(e) = ax_win.raise() {
                    tracing::warn!("Failed to raise window {}: {}", window_id, e);
                } else {
                    tracing::debug!("Raised window {} (pid {})", window_id, pid);
                }
                return;
            }
        }
    }

    tracing::warn!(
        "Could not find AX window for id {} (pid {})",
        window_id,
        pid
    );
}

fn focus_visible_window_if_needed(state: &RefCell<State>) {
    let state = state.borrow();
    let visible_windows = state.visible_windows_on_display(state.focused_display);

    if visible_windows.is_empty() {
        return;
    }

    // Check if current focus is on a visible window
    let focus_is_visible = state
        .focused
        .map(|id| visible_windows.iter().any(|w| w.id == id))
        .unwrap_or(false);

    if !focus_is_visible {
        // Focus the first visible window
        if let Some(window) = visible_windows.first() {
            tracing::info!(
                "Focusing visible window {} ({}) after tag switch",
                window.id,
                window.app_name
            );
            focus_window_by_id(&state, window.id, window.pid);
        }
    }
}

fn apply_window_moves(moves: &[WindowMove]) {
    // Group moves by PID to minimize AX calls
    use std::collections::HashMap;
    let mut by_pid: HashMap<i32, Vec<&WindowMove>> = HashMap::new();
    for m in moves {
        by_pid.entry(m.pid).or_default().push(m);
    }

    for (pid, pid_moves) in by_pid {
        let app = AXUIElement::application(pid);
        let ax_windows = match app.windows() {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("Failed to get windows for pid {}: {}", pid, e);
                continue;
            }
        };

        for m in pid_moves {
            // Find matching AX window by window_id
            let mut found = false;
            for ax_win in &ax_windows {
                if let Some(wid) = ax_win.window_id() {
                    if wid == m.window_id {
                        let new_pos = CGPoint::new(m.new_x as f64, m.new_y as f64);
                        if let Err(e) = ax_win.set_position(new_pos) {
                            tracing::warn!(
                                "Failed to move window (id={}, pid={}, to=({}, {})): {}",
                                m.window_id,
                                m.pid,
                                m.new_x,
                                m.new_y,
                                e
                            );
                        } else {
                            tracing::debug!(
                                "Moved window (id={}, pid={}) to ({}, {})",
                                m.window_id,
                                m.pid,
                                m.new_x,
                                m.new_y
                            );
                        }
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                tracing::warn!(
                    "Could not find AX window for id {} (pid {})",
                    m.window_id,
                    m.pid
                );
            }
        }
    }
}

fn apply_layout_on_display(
    state: &State,
    display_id: crate::macos::DisplayId,
    frame: &Rect,
    geometries: &[WindowGeometry],
) {
    use std::collections::HashMap;

    // Display offset
    let offset_x = frame.x;
    let offset_y = frame.y;

    // Build a map of pid -> [(window_id, geometry)]
    let mut by_pid: HashMap<i32, Vec<(u32, &WindowGeometry)>> = HashMap::new();
    for geom in geometries {
        if let Some(window) = state.windows.get(&geom.id) {
            by_pid.entry(window.pid).or_default().push((geom.id, geom));
        }
    }

    for (pid, windows) in by_pid {
        let app = AXUIElement::application(pid);
        let ax_windows = match app.windows() {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("Failed to get windows for pid {}: {}", pid, e);
                continue;
            }
        };

        for (window_id, geom) in windows {
            // Find matching AX window by window_id
            let mut found = false;
            for ax_win in &ax_windows {
                if let Some(wid) = ax_win.window_id() {
                    if wid == window_id {
                        // Apply new geometry with display offset
                        let new_x = geom.x as i32 + offset_x;
                        let new_y = geom.y as i32 + offset_y;
                        let new_pos = CGPoint::new(new_x as f64, new_y as f64);
                        let new_size = CGSize::new(geom.width as f64, geom.height as f64);

                        if let Err(e) = ax_win.set_position(new_pos) {
                            tracing::warn!(
                                "Failed to set position for window {}: {}",
                                window_id,
                                e
                            );
                        }
                        if let Err(e) = ax_win.set_size(new_size) {
                            tracing::warn!("Failed to set size for window {}: {}", window_id, e);
                        }

                        tracing::debug!(
                            "Applied layout to window {} (pid={}) on display {}: ({}, {}) {}x{}",
                            window_id,
                            pid,
                            display_id,
                            new_x,
                            new_y,
                            geom.width,
                            geom.height
                        );
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                tracing::warn!(
                    "Could not find AX window for id {} (pid {}) when applying layout",
                    window_id,
                    pid
                );
            }
        }
    }
}

fn switch_tag_for_focused_window(
    state: &RefCell<State>,
    layout_engine: &mut Option<LayoutEngine>,
) -> Option<Vec<WindowMove>> {
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

    let moves = state.borrow_mut().view_tag(tag);
    if !moves.is_empty() {
        // Need to retile after tag switch
        do_retile(state, layout_engine);
    }
    Some(moves)
}
