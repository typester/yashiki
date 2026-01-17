use crate::core::{State, WindowMove};
use crate::event::Event;
use crate::ipc::IpcServer;
use crate::macos;
use crate::macos::{AXUIElement, ObserverManager, WorkspaceEvent, WorkspaceWatcher};
use anyhow::Result;
use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoop};
use core_graphics::geometry::CGPoint;
use objc2_foundation::MainThreadMarker;
use std::cell::RefCell;
use std::sync::mpsc as std_mpsc;
use tokio::sync::mpsc;
use yashiki_ipc::{Command, Response, StateInfo, WindowInfo};

type IpcCommandWithResponse = (Command, mpsc::Sender<Response>);

struct RunLoopContext {
    ipc_cmd_rx: std_mpsc::Receiver<IpcCommandWithResponse>,
    observer_event_rx: std_mpsc::Receiver<Event>,
    workspace_event_rx: std_mpsc::Receiver<WorkspaceEvent>,
    event_tx: mpsc::Sender<Event>,
    observer_manager: RefCell<ObserverManager>,
    state: RefCell<State>,
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

        // Set up a timer to check for commands and events periodically
        let context = Box::new(RunLoopContext {
            ipc_cmd_rx,
            observer_event_rx,
            workspace_event_rx,
            event_tx,
            observer_manager: RefCell::new(observer_manager),
            state: RefCell::new(state),
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
                let response = handle_ipc_command(&ctx.state, &cmd);
                let _ = resp_tx.blocking_send(response);

                // Handle Quit command after sending response
                if matches!(cmd, Command::Quit) {
                    CFRunLoop::get_current().stop();
                }
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
            while let Ok(event) = ctx.observer_event_rx.try_recv() {
                ctx.state.borrow_mut().handle_event(&event);
                if ctx.event_tx.blocking_send(event).is_err() {
                    tracing::error!("Failed to forward event to tokio");
                }
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

        tracing::info!("Entering CFRunLoop");
        CFRunLoop::run_current();
        tracing::info!("CFRunLoop exited");
    }
}

fn handle_ipc_command(state: &RefCell<State>, cmd: &Command) -> Response {
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
                    visible_tags: state.visible_tags.mask(),
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
