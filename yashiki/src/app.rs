use crate::core::{Display, State, WindowMove};
use crate::event::Event;
use crate::ipc::IpcServer;
use crate::layout::LayoutEngine;
use crate::macos;
use crate::macos::{
    activate_application, AXUIElement, HotkeyManager, ObserverManager, WorkspaceEvent,
    WorkspaceWatcher,
};
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
}

pub struct App {}

impl App {
    pub fn run() -> Result<()> {
        if !macos::is_trusted() {
            tracing::warn!("Accessibility permission not granted, requesting...");
            macos::is_trusted_with_prompt();
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
        let mut state = State::new();
        state.sync_all();

        // Spawn layout engine
        let layout_engine = match LayoutEngine::spawn("tatami") {
            Ok(engine) => Some(engine),
            Err(e) => {
                tracing::warn!("Failed to spawn layout engine: {}", e);
                None
            }
        };

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
            state: RefCell::new(state),
            layout_engine: RefCell::new(layout_engine),
            hotkey_manager: RefCell::new(hotkey_manager),
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
                    }
                }
            }

            // Process observer events and forward to tokio
            let mut needs_retile = false;
            while let Ok(event) = ctx.observer_event_rx.try_recv() {
                if ctx.state.borrow_mut().handle_event(&event) {
                    needs_retile = true;
                }
                if ctx.event_tx.blocking_send(event).is_err() {
                    tracing::error!("Failed to forward event to tokio");
                }
            }
            if needs_retile {
                do_retile(&ctx.state.borrow(), &mut ctx.layout_engine.borrow_mut());
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
        Command::ViewTag { tag } => {
            let moves = state.borrow_mut().view_tag(*tag);
            apply_window_moves(&moves);
            Response::Ok
        }
        Command::ToggleViewTag { tag } => {
            let moves = state.borrow_mut().toggle_view_tag(*tag);
            apply_window_moves(&moves);
            Response::Ok
        }
        Command::MoveToTag { tag } => {
            let moves = state.borrow_mut().move_focused_to_tag(*tag);
            apply_window_moves(&moves);
            Response::Ok
        }
        Command::ToggleWindowTag { tag } => {
            let moves = state.borrow_mut().toggle_focused_window_tag(*tag);
            apply_window_moves(&moves);
            Response::Ok
        }
        Command::LayoutCommand { cmd, args } => {
            let mut engine = layout_engine.borrow_mut();
            if let Some(ref mut engine) = *engine {
                match engine.send_command(cmd, args) {
                    Ok(()) => Response::Ok,
                    Err(e) => Response::Error {
                        message: format!("Layout command failed: {}", e),
                    },
                }
            } else {
                Response::Error {
                    message: "No layout engine available".to_string(),
                }
            }
        }
        Command::Retile => {
            do_retile(&state.borrow(), &mut layout_engine.borrow_mut());
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
                do_retile_display(
                    &state.borrow(),
                    &mut layout_engine.borrow_mut(),
                    source_display,
                );
                do_retile_display(
                    &state.borrow(),
                    &mut layout_engine.borrow_mut(),
                    target_display,
                );
            }
            Response::Ok
        }
        Command::Quit => {
            tracing::info!("Quit command received");
            Response::Ok
        }
        _ => {
            tracing::warn!("Unhandled command: {:?}", cmd);
            Response::Error {
                message: "Command not yet implemented".to_string(),
            }
        }
    }
}

fn do_retile(state: &State, layout_engine: &mut Option<LayoutEngine>) {
    let Some(ref mut engine) = layout_engine else {
        return;
    };

    // Retile each display independently
    for (display_id, display) in &state.displays {
        retile_single_display(state, engine, *display_id, display);
    }
}

fn do_retile_display(
    state: &State,
    layout_engine: &mut Option<LayoutEngine>,
    display_id: crate::macos::DisplayId,
) {
    let Some(ref mut engine) = layout_engine else {
        return;
    };
    let Some(display) = state.displays.get(&display_id) else {
        return;
    };
    retile_single_display(state, engine, display_id, display);
}

fn retile_single_display(
    state: &State,
    engine: &mut LayoutEngine,
    display_id: crate::macos::DisplayId,
    display: &Display,
) {
    let visible_windows = state.visible_windows_on_display(display_id);

    if visible_windows.is_empty() {
        return;
    }

    let window_ids: Vec<u32> = visible_windows.iter().map(|w| w.id).collect();
    let width = display.frame.width;
    let height = display.frame.height;

    match engine.request_layout(width, height, &window_ids) {
        Ok(geometries) => {
            apply_layout_on_display(state, display, &geometries);
        }
        Err(e) => {
            tracing::error!("Layout request failed for display {}: {}", display_id, e);
        }
    }
}

fn move_window_to_position(pid: i32, window_id: u32, state: &State, x: i32, y: i32) {
    let window = match state.windows.get(&window_id) {
        Some(w) => w,
        None => return,
    };

    let app = AXUIElement::application(pid);
    let ax_windows = match app.windows() {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!("Failed to get windows for pid {}: {}", pid, e);
            return;
        }
    };

    // Find matching AX window by current position
    for ax_win in &ax_windows {
        let pos = match ax_win.position() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let size = match ax_win.size() {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Use stored frame (before moving)
        let old_x = window.frame.x as f64 - (x as f64 - window.frame.x as f64);
        let old_y = window.frame.y as f64 - (y as f64 - window.frame.y as f64);

        let pos_match = (pos.x - old_x).abs() < 10.0 && (pos.y - old_y).abs() < 10.0;
        let size_match = (size.width - window.frame.width as f64).abs() < 10.0
            && (size.height - window.frame.height as f64).abs() < 10.0;

        if pos_match || size_match {
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

    // Fallback: move first window
    if let Some(ax_win) = ax_windows.first() {
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
            tracing::info!("Moved window {} to ({}, {}) (fallback)", window_id, x, y);
        }
    }
}

fn focus_window_by_id(state: &State, window_id: u32, pid: i32) {
    // Activate the application first
    activate_application(pid);

    // Get the target window's position/size for matching
    let window = match state.windows.get(&window_id) {
        Some(w) => w,
        None => return,
    };

    // Get AX windows and find the matching one
    let app = AXUIElement::application(pid);
    let ax_windows = match app.windows() {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!("Failed to get windows for pid {}: {}", pid, e);
            return;
        }
    };

    for ax_win in &ax_windows {
        let pos = match ax_win.position() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let size = match ax_win.size() {
            Ok(s) => s,
            Err(_) => continue,
        };

        let pos_match = (pos.x - window.frame.x as f64).abs() < 10.0
            && (pos.y - window.frame.y as f64).abs() < 10.0;
        let size_match = (size.width - window.frame.width as f64).abs() < 10.0
            && (size.height - window.frame.height as f64).abs() < 10.0;

        if pos_match && size_match {
            if let Err(e) = ax_win.raise() {
                tracing::warn!("Failed to raise window {}: {}", window_id, e);
            } else {
                tracing::debug!("Raised window {} (pid {})", window_id, pid);
            }
            return;
        }
    }

    tracing::warn!(
        "Could not find matching AX window for id {} (pid {})",
        window_id,
        pid
    );
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
        let windows = match app.windows() {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("Failed to get windows for pid {}: {}", pid, e);
                continue;
            }
        };

        for m in pid_moves {
            // Find the window to move - for now, just move all windows of this app
            // In the future, we should match by position/size
            for win in &windows {
                let pos = CGPoint::new(m.x as f64, m.y as f64);
                if let Err(e) = win.set_position(pos) {
                    tracing::warn!(
                        "Failed to move window (pid={}, target=({}, {})): {}",
                        m.pid,
                        m.x,
                        m.y,
                        e
                    );
                } else {
                    tracing::debug!("Moved window (pid={}) to ({}, {})", m.pid, m.x, m.y);
                    break;
                }
            }
        }
    }
}

fn apply_layout_on_display(state: &State, disp: &Display, geometries: &[WindowGeometry]) {
    use std::collections::HashMap;

    // Display offset
    let offset_x = disp.frame.x;
    let offset_y = disp.frame.y;

    // Build a map of window_id -> (pid, geometry)
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
            // Find matching AX window by current position/size
            if let Some(window) = state.windows.get(&window_id) {
                for ax_win in &ax_windows {
                    // Match by approximate position
                    let pos = match ax_win.position() {
                        Ok(p) => p,
                        Err(_) => continue,
                    };
                    let size = match ax_win.size() {
                        Ok(s) => s,
                        Err(_) => continue,
                    };

                    let pos_match = (pos.x - window.frame.x as f64).abs() < 10.0
                        && (pos.y - window.frame.y as f64).abs() < 10.0;
                    let size_match = (size.width - window.frame.width as f64).abs() < 10.0
                        && (size.height - window.frame.height as f64).abs() < 10.0;

                    if pos_match && size_match {
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
                            disp.id,
                            new_x,
                            new_y,
                            geom.width,
                            geom.height
                        );
                        break;
                    }
                }
            }
        }
    }
}
