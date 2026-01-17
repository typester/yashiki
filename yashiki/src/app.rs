use crate::event::{Command, Event};
use crate::macos;
use crate::macos::{ObserverManager, WorkspaceEvent, WorkspaceWatcher};
use anyhow::Result;
use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoop};
use objc2_foundation::MainThreadMarker;
use std::cell::RefCell;
use std::sync::mpsc as std_mpsc;
use tokio::sync::mpsc;

struct RunLoopContext {
    cmd_rx: std_mpsc::Receiver<Command>,
    observer_event_rx: std_mpsc::Receiver<Event>,
    workspace_event_rx: std_mpsc::Receiver<WorkspaceEvent>,
    event_tx: mpsc::Sender<Event>,
    observer_manager: RefCell<ObserverManager>,
}

pub struct App {}

impl App {
    pub fn run() -> Result<()> {
        if !macos::is_trusted() {
            tracing::warn!("Accessibility permission not granted, requesting...");
            macos::is_trusted_with_prompt();
            anyhow::bail!("Please grant Accessibility permission and restart");
        }

        // Channel: tokio -> main thread (via dispatch)
        let (cmd_tx, cmd_rx) = std_mpsc::channel::<Command>();

        // Channel: observer -> main thread
        let (observer_event_tx, observer_event_rx) = std_mpsc::channel::<Event>();

        // Channel: main thread -> tokio
        let (event_tx, event_rx) = mpsc::channel::<Event>(256);

        // Spawn tokio runtime in separate thread
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                Self::run_async(cmd_tx, event_rx).await;
            });
        });

        let app = App {};
        app.run_main_loop(cmd_rx, observer_event_tx, observer_event_rx, event_tx);

        Ok(())
    }

    async fn run_async(_cmd_tx: std_mpsc::Sender<Command>, mut event_rx: mpsc::Receiver<Event>) {
        tracing::info!("Tokio runtime started");

        // Process events from main thread
        while let Some(event) = event_rx.recv().await {
            tracing::info!("Received event: {:?}", event);
        }

        tracing::info!("Tokio runtime exiting");
    }

    fn run_main_loop(
        self,
        cmd_rx: std_mpsc::Receiver<Command>,
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

        // Set up a timer to check for commands and events periodically
        let context = Box::new(RunLoopContext {
            cmd_rx,
            observer_event_rx,
            workspace_event_rx,
            event_tx,
            observer_manager: RefCell::new(observer_manager),
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

            // Process commands from tokio
            while let Ok(cmd) = ctx.cmd_rx.try_recv() {
                tracing::debug!("Received command: {:?}", cmd);
                match cmd {
                    Command::MoveFocusedWindow { position } => match macos::get_focused_window() {
                        Ok(win) => {
                            let title = win.title().unwrap_or_default();
                            tracing::info!(
                                "Moving window {:?} to ({}, {})",
                                title,
                                position.x,
                                position.y
                            );
                            if let Err(e) = win.set_position(position) {
                                tracing::error!("Failed to move window: {}", e);
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to get focused window: {}", e);
                        }
                    },
                    Command::Quit => {
                        CFRunLoop::get_current().stop();
                    }
                    _ => {}
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

            // Forward observer events to tokio
            while let Ok(event) = ctx.observer_event_rx.try_recv() {
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
