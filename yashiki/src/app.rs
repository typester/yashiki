use crate::event::Command;
use crate::macos;
use anyhow::Result;
use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoop};
use std::sync::mpsc as std_mpsc;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct App {
    cmd_rx: std_mpsc::Receiver<Command>,
}

impl App {
    pub fn run() -> Result<()> {
        if !macos::is_trusted() {
            tracing::warn!("Accessibility permission not granted, requesting...");
            macos::is_trusted_with_prompt();
            anyhow::bail!("Please grant Accessibility permission and restart");
        }

        // Channel: tokio -> main thread (via dispatch)
        // We use std::sync::mpsc because dispatch callback needs 'static + Send
        let (cmd_tx, cmd_rx) = std_mpsc::channel::<Command>();

        // Channel: main thread -> tokio
        let (event_tx, event_rx) = mpsc::channel::<crate::event::Event>(256);

        // Spawn tokio runtime in separate thread
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                Self::run_async(cmd_tx, event_rx).await;
            });
        });

        let app = App { cmd_rx };
        app.run_main_loop(event_tx);

        Ok(())
    }

    async fn run_async(
        cmd_tx: std_mpsc::Sender<Command>,
        mut event_rx: mpsc::Receiver<crate::event::Event>,
    ) {
        tracing::info!("Tokio runtime started");

        // Test: send a command after 3 seconds
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            tracing::info!("Sending test command to move focused window");

            let position = core_graphics::geometry::CGPoint::new(200.0, 200.0);
            let cmd = Command::MoveFocusedWindow { position };

            dispatch::Queue::main().exec_async(move || {
                if let Err(e) = cmd_tx.send(cmd) {
                    tracing::error!("Failed to send command: {}", e);
                }
            });
        });

        // Process events from main thread
        while let Some(event) = event_rx.recv().await {
            tracing::info!("Received event: {:?}", event);
        }

        tracing::info!("Tokio runtime exiting");
    }

    fn run_main_loop(self, _event_tx: mpsc::Sender<crate::event::Event>) {
        tracing::info!("Starting main loop");

        // Set up a timer to check for commands periodically
        let cmd_rx = self.cmd_rx;

        let timer_context = Box::new(cmd_rx);
        let mut context = core_foundation::runloop::CFRunLoopTimerContext {
            version: 0,
            info: Box::into_raw(timer_context) as *mut _,
            retain: None,
            release: None,
            copyDescription: None,
        };

        extern "C" fn timer_callback(
            _timer: core_foundation::runloop::CFRunLoopTimerRef,
            info: *mut std::ffi::c_void,
        ) {
            let cmd_rx = unsafe { &*(info as *const std_mpsc::Receiver<Command>) };

            while let Ok(cmd) = cmd_rx.try_recv() {
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
        }

        let timer = unsafe {
            core_foundation::runloop::CFRunLoopTimer::new(
                core_foundation::date::CFAbsoluteTimeGetCurrent(),
                0.05, // 50ms interval
                0,
                0,
                timer_callback,
                &mut context,
            )
        };

        let run_loop = CFRunLoop::get_current();
        run_loop.add_timer(&timer, unsafe { kCFRunLoopDefaultMode });

        tracing::info!("Entering CFRunLoop");
        CFRunLoop::run_current();
        tracing::info!("CFRunLoop exited");
    }
}
