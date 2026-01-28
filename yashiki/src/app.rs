use std::cell::RefCell;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;

use anyhow::Result;
use core_foundation::base::TCFType;
use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoop};
use core_foundation_sys::runloop::{
    CFRunLoopAddSource, CFRunLoopGetMain, CFRunLoopSourceContext, CFRunLoopSourceCreate,
};
use objc2::rc::Retained;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSEvent, NSEventType};
use objc2_foundation::MainThreadMarker;
use tokio::sync::mpsc;

mod channels;
mod command;
mod dispatch;
mod effects;
mod focus;
mod retile;
mod state_events;
mod sync_helper;

use channels::{create_channels, run_async, IpcCommandWithResponse, MainChannels, SnapshotRequest};
use dispatch::dispatch_command;
use focus::{notify_layout_focus, switch_tag_for_focused_window};
use retile::{do_retile, do_retile_display};
use sync_helper::{process_new_windows, sync_and_process_new_windows, sync_focused_and_process};

use crate::core::State;
use crate::event::Event;
use crate::event_emitter::{create_snapshot, EventEmitter};
use crate::layout::LayoutEngineManager;
use crate::macos;
use crate::macos::{
    DisplayReconfigEvent, HotkeyManager, MousePosition, MouseTracker, ObserverManager,
    WorkspaceEvent, WorkspaceWatcher,
};
use crate::pid;
use crate::platform::{MacOSWindowManipulator, MacOSWindowSystem, WindowManipulator};
use yashiki_ipc::Command;

struct RunLoopContext {
    ipc_cmd_rx: std_mpsc::Receiver<IpcCommandWithResponse>,
    hotkey_cmd_rx: std_mpsc::Receiver<Command>,
    mouse_event_rx: std_mpsc::Receiver<MousePosition>,
    observer_event_rx: std_mpsc::Receiver<Event>,
    workspace_event_rx: std_mpsc::Receiver<WorkspaceEvent>,
    snapshot_request_rx: std_mpsc::Receiver<SnapshotRequest>,
    display_reconfig_rx: std_mpsc::Receiver<DisplayReconfigEvent>,
    event_tx: mpsc::Sender<Event>,
    event_emitter: EventEmitter,
    observer_manager: RefCell<ObserverManager>,
    state: RefCell<State>,
    layout_engine_manager: RefCell<LayoutEngineManager>,
    hotkey_manager: RefCell<HotkeyManager>,
    mouse_tracker: RefCell<MouseTracker>,
    window_system: MacOSWindowSystem,
    window_manipulator: MacOSWindowManipulator,
    ns_app: Retained<NSApplication>,
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

        let (tokio_channels, main_channels) = create_channels();

        // Spawn tokio runtime in separate thread
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                run_async(tokio_channels).await;
            });
        });

        let app = App {};
        app.run_main_loop(main_channels);

        // Clean up PID file on exit
        pid::remove_pid();
        Ok(())
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
            display_reconfig_tx,
            display_reconfig_rx,
            ipc_source: ipc_source_ptr,
        } = channels;

        tracing::info!("Starting main loop");

        // Get MainThreadMarker - we're on the main thread
        let mtm = MainThreadMarker::new().expect("Must be called from main thread");

        // Initialize NSApplication (required for CGDisplayRegisterReconfigurationCallback)
        let ns_app = NSApplication::sharedApplication(mtm);
        ns_app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

        // Create source pointers early (will be set after CFRunLoopSource creation)
        let observer_source_ptr = Arc::new(AtomicPtr::new(ptr::null_mut()));
        let workspace_source_ptr = Arc::new(AtomicPtr::new(ptr::null_mut()));
        let display_source_ptr = Arc::new(AtomicPtr::new(ptr::null_mut()));

        // Start observer manager (with source_ptr for event-driven signaling)
        let mut observer_manager =
            ObserverManager::new(observer_event_tx, observer_source_ptr.clone());
        observer_manager.start();

        // Start workspace watcher for app launch/terminate notifications
        let (workspace_event_tx, workspace_event_rx) = std_mpsc::channel::<WorkspaceEvent>();
        let _workspace_watcher =
            WorkspaceWatcher::new(workspace_event_tx, workspace_source_ptr.clone(), mtm);

        // Initialize state with current windows
        let window_system = MacOSWindowSystem;
        let mut state = State::new();
        state.config.exec_path = build_initial_exec_path();
        // Initial sync has no hidden windows, so rehide_moves is always empty
        // Note: new_window_ids are not processed here - rules aren't loaded yet,
        // ApplyRules command is sent after init script runs
        let (_, _) = state.sync_all(&window_system);

        // Create layout engine manager (lazy spawning)
        let mut layout_engine_manager = LayoutEngineManager::new();
        layout_engine_manager.set_exec_path(&state.config.exec_path);
        let layout_engine_manager = RefCell::new(layout_engine_manager);

        let state = RefCell::new(state);

        // Create window manipulator
        let window_manipulator = MacOSWindowManipulator;

        // Create event emitter
        let event_emitter = EventEmitter::new(state_event_tx);

        // Initial retile
        do_retile(&state, &layout_engine_manager, &window_manipulator);

        // Create shared pointer for hotkey CFRunLoopSource
        let hotkey_source_ptr = Arc::new(AtomicPtr::new(ptr::null_mut()));
        let hotkey_source_clone = Arc::clone(&hotkey_source_ptr);

        // Create hotkey manager
        let (hotkey_cmd_tx, hotkey_cmd_rx) = std_mpsc::channel::<Command>();
        let mut hotkey_manager = HotkeyManager::new(hotkey_cmd_tx, hotkey_source_clone);

        // Start hotkey tap (initially with no bindings, will be updated via IPC)
        if let Err(e) = hotkey_manager.start() {
            tracing::warn!("Failed to start hotkey tap: {}", e);
        }

        // Create shared pointer for mouse CFRunLoopSource
        let mouse_source_ptr = Arc::new(AtomicPtr::new(ptr::null_mut()));
        let mouse_source_clone = Arc::clone(&mouse_source_ptr);

        // Create mouse tracker (initially stopped, will be started via IPC)
        let (mouse_event_tx, mouse_event_rx) = std_mpsc::channel::<MousePosition>();
        let mouse_tracker = MouseTracker::new(mouse_event_tx, mouse_source_clone);

        // Create shared context for IPC/hotkey/display sources
        let context = Box::new(RunLoopContext {
            ipc_cmd_rx,
            hotkey_cmd_rx,
            mouse_event_rx,
            observer_event_rx,
            workspace_event_rx,
            snapshot_request_rx,
            display_reconfig_rx,
            event_tx,
            event_emitter,
            observer_manager: RefCell::new(observer_manager),
            state,
            layout_engine_manager,
            hotkey_manager: RefCell::new(hotkey_manager),
            mouse_tracker: RefCell::new(mouse_tracker),
            window_system,
            window_manipulator,
            ns_app: ns_app.clone(),
        });
        let context_ptr = Box::into_raw(context) as *mut std::ffi::c_void;

        // Create CFRunLoopSource for IPC commands (immediate processing)
        extern "C" fn ipc_source_callback(info: *const std::ffi::c_void) {
            let ctx = unsafe { &*(info as *const RunLoopContext) };

            // Process snapshot requests
            while let Ok(resp_tx) = ctx.snapshot_request_rx.try_recv() {
                let snapshot = create_snapshot(&ctx.state.borrow());
                let _ = resp_tx.send(snapshot);
            }

            // Process all pending IPC commands
            while let Ok((cmd, resp_tx)) = ctx.ipc_cmd_rx.try_recv() {
                tracing::debug!("Received IPC command: {:?}", cmd);

                let response = dispatch_command(
                    &cmd,
                    &ctx.state,
                    &ctx.layout_engine_manager,
                    &ctx.hotkey_manager,
                    &ctx.window_system,
                    &ctx.window_manipulator,
                    &ctx.event_emitter,
                    &ctx.observer_manager,
                );
                let _ = resp_tx.blocking_send(response);

                // Handle Quit command after sending response
                if matches!(cmd, Command::Quit) {
                    // Terminate all tracked processes
                    for process in ctx.state.borrow().tracked_processes.iter() {
                        ctx.window_manipulator.terminate_process(process.pid);
                    }
                    // Stop NSApplication and post a dummy event to exit run() immediately
                    ctx.ns_app.stop(None);
                    // Post dummy event to wake up NSApp.run()
                    if let Some(event) = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
                        NSEventType::ApplicationDefined,
                        objc2_foundation::NSPoint::new(0.0, 0.0),
                        objc2_app_kit::NSEventModifierFlags::empty(),
                        0.0,
                        0,
                        None,
                        0,
                        0,
                        0,
                    ) {
                        ctx.ns_app.postEvent_atStart(&event, true);
                    }
                }
            }

            // Apply pending hotkey binding changes
            if let Err(e) = ctx.hotkey_manager.borrow_mut().ensure_tap() {
                tracing::error!("Failed to update hotkey tap: {}", e);
            }

            // Sync mouse tracker state with auto-raise config
            {
                use yashiki_ipc::AutoRaiseMode;
                let mode = ctx.state.borrow().config.auto_raise_mode;
                let mut tracker = ctx.mouse_tracker.borrow_mut();
                match mode {
                    AutoRaiseMode::Enabled => {
                        if !tracker.is_running() {
                            if let Err(e) = tracker.start() {
                                tracing::error!("Failed to start mouse tracker: {}", e);
                            }
                        }
                    }
                    AutoRaiseMode::Disabled => {
                        if tracker.is_running() {
                            tracker.stop();
                        }
                    }
                }
            }
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

        // Create CFRunLoopSource for hotkey commands (immediate processing)
        extern "C" fn hotkey_source_callback(info: *const std::ffi::c_void) {
            let ctx = unsafe { &*(info as *const RunLoopContext) };

            // Process all pending hotkey commands
            while let Ok(cmd) = ctx.hotkey_cmd_rx.try_recv() {
                tracing::debug!("Received hotkey command: {:?}", cmd);

                let _ = dispatch_command(
                    &cmd,
                    &ctx.state,
                    &ctx.layout_engine_manager,
                    &ctx.hotkey_manager,
                    &ctx.window_system,
                    &ctx.window_manipulator,
                    &ctx.event_emitter,
                    &ctx.observer_manager,
                );
            }
        }

        let mut hotkey_source_context = CFRunLoopSourceContext {
            version: 0,
            info: context_ptr,
            retain: None,
            release: None,
            copyDescription: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform: hotkey_source_callback,
        };

        let hotkey_source =
            unsafe { CFRunLoopSourceCreate(ptr::null(), 0, &mut hotkey_source_context) };
        if hotkey_source.is_null() {
            tracing::error!("Failed to create CFRunLoopSource for hotkey");
        } else {
            // Register source with main RunLoop
            let run_loop = CFRunLoop::get_current();
            unsafe {
                CFRunLoopAddSource(
                    run_loop.as_concrete_TypeRef(),
                    hotkey_source,
                    kCFRunLoopDefaultMode,
                );
            }
            // Store source pointer for hotkey tap to signal
            hotkey_source_ptr.store(hotkey_source as *mut std::ffi::c_void, Ordering::Release);
            tracing::info!("Hotkey CFRunLoopSource created and registered");
        }

        // Create CFRunLoopSource for mouse events (auto-raise)
        extern "C" fn mouse_source_callback(info: *const std::ffi::c_void) {
            let ctx = unsafe { &*(info as *const RunLoopContext) };

            use std::time::Instant;
            use yashiki_ipc::AutoRaiseMode;

            // Process all pending mouse events
            while let Ok(pos) = ctx.mouse_event_rx.try_recv() {
                // Check if auto-raise is enabled
                let mode = ctx.state.borrow().config.auto_raise_mode;
                if mode == AutoRaiseMode::Disabled {
                    continue;
                }

                let delay_ms = ctx.state.borrow().config.auto_raise_delay_ms;

                // Find window at cursor position
                let window_info = ctx.state.borrow().find_window_at_point(pos.x, pos.y);

                match window_info {
                    Some((window_id, pid)) => {
                        let mut state = ctx.state.borrow_mut();
                        let auto_raise = &mut state.auto_raise_state;

                        if auto_raise.last_hovered == Some(window_id) {
                            // Same window - check if delay has elapsed
                            if let Some(start) = auto_raise.hover_start {
                                if start.elapsed().as_millis() >= delay_ms as u128 {
                                    // Delay elapsed - check if already focused
                                    if state.focused != Some(window_id) {
                                        // Set focus intent before focusing
                                        state.set_focus_intent(window_id, pid);
                                        drop(state); // Release borrow before manipulator call
                                        tracing::debug!(
                                            "Auto-raise: focusing window {} at ({}, {})",
                                            window_id,
                                            pos.x,
                                            pos.y
                                        );
                                        ctx.window_manipulator.focus_window(window_id, pid);
                                        ctx.state.borrow_mut().set_focused(Some(window_id));
                                        ctx.event_emitter.emit_window_focused(Some(window_id));
                                        // Clear hover state after focusing
                                        ctx.state.borrow_mut().auto_raise_state.hover_start = None;
                                    }
                                }
                            }
                        } else {
                            // Different window - record new hover
                            auto_raise.last_hovered = Some(window_id);
                            auto_raise.hover_start = Some(Instant::now());
                        }
                    }
                    None => {
                        // No window under cursor - clear hover state
                        let mut state = ctx.state.borrow_mut();
                        state.auto_raise_state.last_hovered = None;
                        state.auto_raise_state.hover_start = None;
                    }
                }
            }
        }

        let mut mouse_source_context = CFRunLoopSourceContext {
            version: 0,
            info: context_ptr,
            retain: None,
            release: None,
            copyDescription: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform: mouse_source_callback,
        };

        let mouse_source =
            unsafe { CFRunLoopSourceCreate(ptr::null(), 0, &mut mouse_source_context) };
        if mouse_source.is_null() {
            tracing::error!("Failed to create CFRunLoopSource for mouse");
        } else {
            // Register source with main RunLoop
            let run_loop = CFRunLoop::get_current();
            unsafe {
                CFRunLoopAddSource(
                    run_loop.as_concrete_TypeRef(),
                    mouse_source,
                    kCFRunLoopDefaultMode,
                );
            }
            // Store source pointer for mouse tracker to signal
            mouse_source_ptr.store(mouse_source as *mut std::ffi::c_void, Ordering::Release);
            tracing::info!("Mouse CFRunLoopSource created and registered");
        }

        // Create CFRunLoopSource for display reconfiguration events
        extern "C" fn display_source_callback(info: *const std::ffi::c_void) {
            let ctx = unsafe { &*(info as *const RunLoopContext) };

            // Process all pending display reconfig events
            while let Ok(event) = ctx.display_reconfig_rx.try_recv() {
                tracing::info!(
                    "Display reconfiguration: display_id={}, flags={:#x}",
                    event.display_id,
                    event.flags
                );

                // Handle display change
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

                // Emit DisplayUpdated events for frame changes
                {
                    let state = ctx.state.borrow();
                    for disp in state.displays.values() {
                        ctx.event_emitter
                            .emit_display_updated(disp, focused_display);
                    }
                }

                // Apply window moves for orphaned windows
                if !result.window_moves.is_empty() {
                    ctx.window_manipulator
                        .apply_window_moves(&result.window_moves);
                }

                // Apply rules to newly discovered windows
                process_new_windows(
                    result.new_window_ids,
                    &ctx.state,
                    &ctx.layout_engine_manager,
                    &ctx.window_manipulator,
                    &ctx.event_emitter,
                );

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
                    do_retile(
                        &ctx.state,
                        &ctx.layout_engine_manager,
                        &ctx.window_manipulator,
                    );
                }
            }
        }

        let mut display_source_context = CFRunLoopSourceContext {
            version: 0,
            info: context_ptr,
            retain: None,
            release: None,
            copyDescription: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform: display_source_callback,
        };

        let display_source =
            unsafe { CFRunLoopSourceCreate(ptr::null(), 0, &mut display_source_context) };
        if display_source.is_null() {
            tracing::error!("Failed to create CFRunLoopSource for display");
        } else {
            // Register source with main RunLoop
            let run_loop = unsafe {
                core_foundation::runloop::CFRunLoop::wrap_under_get_rule(CFRunLoopGetMain())
            };
            unsafe {
                CFRunLoopAddSource(
                    run_loop.as_concrete_TypeRef(),
                    display_source,
                    kCFRunLoopDefaultMode,
                );
            }
            display_source_ptr.store(display_source as *mut std::ffi::c_void, Ordering::Release);
            tracing::info!("Display CFRunLoopSource created and registered");

            // Register display callback (now that source_ptr is set)
            if let Err(e) =
                macos::register_display_callback(display_reconfig_tx, display_source_ptr.clone())
            {
                tracing::warn!("Failed to register display callback: {}", e);
            }
        }

        // Create CFRunLoopSource for workspace events (app launch/terminate)
        extern "C" fn workspace_source_callback(info: *const std::ffi::c_void) {
            let ctx = unsafe { &*(info as *const RunLoopContext) };

            // Process workspace events (app launch/terminate)
            while let Ok(event) = ctx.workspace_event_rx.try_recv() {
                match event {
                    WorkspaceEvent::AppLaunched { pid } => {
                        tracing::info!("App launched, adding observer for pid {}", pid);
                        if let Err(e) = ctx.observer_manager.borrow_mut().add_observer(pid) {
                            tracing::warn!("Failed to add observer for pid {}: {}", pid, e);
                        }

                        // Sync windows for this pid immediately after adding observer
                        let result = sync_and_process_new_windows(
                            &ctx.state,
                            &ctx.window_system,
                            &ctx.layout_engine_manager,
                            &ctx.window_manipulator,
                            &ctx.event_emitter,
                            &ctx.observer_manager,
                            pid,
                        );

                        if result.changed {
                            do_retile(
                                &ctx.state,
                                &ctx.layout_engine_manager,
                                &ctx.window_manipulator,
                            );
                        }

                        // Sync focused window in case we missed the ApplicationActivated event
                        // (can happen if the app was activated before the observer was ready)
                        let focused_result = sync_focused_and_process(
                            &ctx.state,
                            &ctx.window_system,
                            &ctx.layout_engine_manager,
                            &ctx.window_manipulator,
                            &ctx.event_emitter,
                            &ctx.observer_manager,
                            Some(pid),
                        );

                        if focused_result.changed {
                            do_retile(
                                &ctx.state,
                                &ctx.layout_engine_manager,
                                &ctx.window_manipulator,
                            );
                        }

                        ctx.event_emitter
                            .emit_window_focused(ctx.state.borrow().focused);
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

                        // Directly remove windows - no AX API check needed since
                        // process termination is confirmed by NSWorkspace notification
                        let changed = ctx.state.borrow_mut().remove_windows_for_pid(pid);
                        if changed {
                            do_retile(
                                &ctx.state,
                                &ctx.layout_engine_manager,
                                &ctx.window_manipulator,
                            );
                        }
                    }
                    WorkspaceEvent::AppActivated { pid } => {
                        // Only sync if we don't have an observer OR we don't have windows
                        // This avoids redundant work when the app is already being tracked
                        let needs_sync = !ctx.observer_manager.borrow().has_observer(pid)
                            || !ctx.state.borrow().has_windows_for_pid(pid);

                        if needs_sync {
                            tracing::info!("App activated (needs sync), pid {}", pid);

                            // sync_and_process_new_windows handles observer registration internally
                            let result = sync_and_process_new_windows(
                                &ctx.state,
                                &ctx.window_system,
                                &ctx.layout_engine_manager,
                                &ctx.window_manipulator,
                                &ctx.event_emitter,
                                &ctx.observer_manager,
                                pid,
                            );

                            if result.changed {
                                do_retile(
                                    &ctx.state,
                                    &ctx.layout_engine_manager,
                                    &ctx.window_manipulator,
                                );
                            }

                            // Sync focused window
                            let focused_result = sync_focused_and_process(
                                &ctx.state,
                                &ctx.window_system,
                                &ctx.layout_engine_manager,
                                &ctx.window_manipulator,
                                &ctx.event_emitter,
                                &ctx.observer_manager,
                                Some(pid),
                            );

                            if focused_result.changed {
                                do_retile(
                                    &ctx.state,
                                    &ctx.layout_engine_manager,
                                    &ctx.window_manipulator,
                                );
                            }

                            ctx.event_emitter
                                .emit_window_focused(ctx.state.borrow().focused);
                        } else {
                            tracing::debug!("App activated (already tracked), pid {}", pid);
                        }
                    }
                    WorkspaceEvent::DisplaysChanged => {
                        // Handled by display_source_callback via CGDisplayRegisterReconfigurationCallback
                        tracing::debug!(
                            "DisplaysChanged event received (handled by display callback)"
                        );
                    }
                }
            }
        }

        let mut workspace_source_context = CFRunLoopSourceContext {
            version: 0,
            info: context_ptr,
            retain: None,
            release: None,
            copyDescription: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform: workspace_source_callback,
        };

        let workspace_source =
            unsafe { CFRunLoopSourceCreate(ptr::null(), 0, &mut workspace_source_context) };
        if workspace_source.is_null() {
            tracing::error!("Failed to create CFRunLoopSource for workspace");
        } else {
            let run_loop = unsafe {
                core_foundation::runloop::CFRunLoop::wrap_under_get_rule(CFRunLoopGetMain())
            };
            unsafe {
                CFRunLoopAddSource(
                    run_loop.as_concrete_TypeRef(),
                    workspace_source,
                    kCFRunLoopDefaultMode,
                );
            }
            workspace_source_ptr
                .store(workspace_source as *mut std::ffi::c_void, Ordering::Release);
            tracing::info!("Workspace CFRunLoopSource created and registered");
        }

        // Create CFRunLoopSource for observer events
        extern "C" fn observer_source_callback(info: *const std::ffi::c_void) {
            let ctx = unsafe { &*(info as *const RunLoopContext) };

            // Process observer events and forward to tokio
            let mut needs_retile = false;
            while let Ok(event) = ctx.observer_event_rx.try_recv() {
                let is_focus_event = matches!(
                    event,
                    Event::FocusedWindowChanged | Event::ApplicationActivated { .. }
                );

                // Capture previous focused window before handle_event updates it
                let prev_focused = if is_focus_event {
                    Some(ctx.state.borrow().focused)
                } else {
                    None
                };

                // For ApplicationActivated, sync windows if none exist for this pid.
                // This handles cases where AppLaunched event was missed.
                if let Event::ApplicationActivated { pid } = &event {
                    if !ctx.state.borrow().has_windows_for_pid(*pid) {
                        // Ensure observer exists
                        if !ctx.observer_manager.borrow().has_observer(*pid) {
                            tracing::info!(
                                "Adding missing observer for activated app pid {}",
                                *pid
                            );
                            let _ = ctx.observer_manager.borrow_mut().add_observer(*pid);
                        }

                        // Sync windows for this pid
                        tracing::info!("Syncing windows for activated app pid {}", *pid);
                        let result = sync_and_process_new_windows(
                            &ctx.state,
                            &ctx.window_system,
                            &ctx.layout_engine_manager,
                            &ctx.window_manipulator,
                            &ctx.event_emitter,
                            &ctx.observer_manager,
                            *pid,
                        );

                        if result.changed {
                            needs_retile = true;
                        }
                    }
                }

                let (changed, new_window_ids, rehide_moves) = ctx
                    .state
                    .borrow_mut()
                    .handle_event(&ctx.window_system, &event);

                // Re-hide windows that macOS moved from hide position
                if !rehide_moves.is_empty() {
                    ctx.window_manipulator.apply_window_moves(&rehide_moves);
                }

                if changed {
                    needs_retile = true;
                }

                // Apply rules to newly created windows and emit events
                process_new_windows(
                    new_window_ids,
                    &ctx.state,
                    &ctx.layout_engine_manager,
                    &ctx.window_manipulator,
                    &ctx.event_emitter,
                );

                // On external focus change, notify layout engine and switch tag if focused window is hidden
                if is_focus_event {
                    let focused_id = ctx.state.borrow().focused;

                    // Check if this is a spurious focus change caused by macOS
                    // (focus jumped to hidden window of same app we just focused)
                    if let Some(focused_id) = focused_id {
                        // Get intended_id and pid before mutable borrow
                        let refocus_info = ctx
                            .state
                            .borrow()
                            .check_spurious_focus_change(focused_id)
                            .and_then(|intended_id| {
                                ctx.state
                                    .borrow()
                                    .windows
                                    .get(&intended_id)
                                    .map(|w| (intended_id, w.pid))
                            });

                        if let Some((intended_id, pid)) = refocus_info {
                            tracing::info!(
                                "Suppressing spurious focus change, refocusing window {}",
                                intended_id
                            );
                            ctx.window_manipulator.focus_window(intended_id, pid);
                            ctx.state.borrow_mut().set_focused(Some(intended_id));
                            // Skip further focus handling - don't switch tags
                            continue;
                        }
                    }

                    // Emit focus change event
                    ctx.event_emitter.emit_window_focused(focused_id);

                    if let Some(focused_id) = focused_id {
                        if notify_layout_focus(&ctx.state, &ctx.layout_engine_manager, focused_id) {
                            needs_retile = true;
                        }
                    }

                    // Only switch tag if focus changed from one window to another.
                    // This prevents unwanted tag switch when:
                    // 1. Accessory apps like Raycast are activated (prev_focused is None)
                    // 2. App terminates and macOS auto-activates another app (prev was None)
                    let focus_changed = match prev_focused {
                        Some(Some(prev_id)) => Some(prev_id) != focused_id,
                        _ => false,
                    };
                    if focus_changed {
                        let moves = switch_tag_for_focused_window(&ctx.state);
                        if let Some(moves) = moves {
                            ctx.window_manipulator.apply_window_moves(&moves);
                            needs_retile = true;
                        }
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
        }

        let mut observer_source_context = CFRunLoopSourceContext {
            version: 0,
            info: context_ptr,
            retain: None,
            release: None,
            copyDescription: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform: observer_source_callback,
        };

        let observer_source =
            unsafe { CFRunLoopSourceCreate(ptr::null(), 0, &mut observer_source_context) };
        if observer_source.is_null() {
            tracing::error!("Failed to create CFRunLoopSource for observer");
        } else {
            let run_loop = unsafe {
                core_foundation::runloop::CFRunLoop::wrap_under_get_rule(CFRunLoopGetMain())
            };
            unsafe {
                CFRunLoopAddSource(
                    run_loop.as_concrete_TypeRef(),
                    observer_source,
                    kCFRunLoopDefaultMode,
                );
            }
            observer_source_ptr.store(observer_source as *mut std::ffi::c_void, Ordering::Release);
            tracing::info!("Observer CFRunLoopSource created and registered");
        }

        // Run init script in background thread
        std::thread::spawn(|| {
            run_init_script();
        });

        tracing::info!("Entering NSApp run loop");
        ns_app.run();
        tracing::info!("NSApp run loop exited");
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
            send_apply_rules(true);
            return;
        }
    };

    let init_script = config_dir.join("init");
    if !init_script.exists() {
        tracing::debug!("No init script found at {:?}", init_script);
        send_apply_rules(true);
        return;
    }

    tracing::info!("Running init script: {:?}", init_script);

    std::thread::sleep(std::time::Duration::from_millis(100));

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

    send_apply_rules(false);
}

fn send_apply_rules(needs_delay: bool) {
    if needs_delay {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    if let Ok(mut client) = crate::ipc::IpcClient::connect() {
        match client.send(&Command::ApplyRules) {
            Ok(_) => tracing::info!("Applied rules to existing windows"),
            Err(e) => tracing::warn!("Failed to apply rules: {}", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::command::process_command;
    use crate::app::state_events::{capture_event_state, emit_state_change_events};
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
        let dummy_source = Arc::new(AtomicPtr::new(std::ptr::null_mut()));
        let hotkey_manager = HotkeyManager::new(tx, dummy_source);

        (state, hotkey_manager)
    }

    #[test]
    fn test_query_commands_have_no_effects() {
        let (mut state, mut hotkey_manager) = setup_state();

        // ListWindows
        let result = process_command(
            &mut state,
            &mut hotkey_manager,
            &Command::ListWindows {
                all: false,
                debug: false,
            },
        );
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
    fn test_list_windows_with_debug_includes_debug_fields() {
        let (mut state, mut hotkey_manager) = setup_state();

        let result = process_command(
            &mut state,
            &mut hotkey_manager,
            &Command::ListWindows {
                all: false,
                debug: true,
            },
        );

        assert!(result.effects.is_empty());

        if let Response::Windows { windows } = result.response {
            assert!(!windows.is_empty());
            // Debug fields should be populated when debug=true
            let first_window = &windows[0];
            // window_level should be Some when debug=true
            assert!(first_window.window_level.is_some());
            // close_button should be Some when debug=true
            assert!(first_window.close_button.is_some());
            // Status should be None for non-all mode
            assert!(first_window.status.is_none());
        } else {
            panic!("Expected Response::Windows");
        }
    }

    #[test]
    fn test_list_windows_all_true_returns_empty_marker() {
        let (mut state, mut hotkey_manager) = setup_state();

        // When all=true, process_command returns an empty marker
        // (the actual implementation happens in handle_ipc_command with system access)
        let result = process_command(
            &mut state,
            &mut hotkey_manager,
            &Command::ListWindows {
                all: true,
                debug: false,
            },
        );

        assert!(result.effects.is_empty());

        if let Response::Windows { windows } = result.response {
            // Empty marker - actual implementation is in handle_ipc_command
            assert!(windows.is_empty());
        } else {
            panic!("Expected Response::Windows");
        }
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
                track: false,
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
    fn test_exec_tracked_produces_exec_tracked_effect() {
        let (mut state, mut hotkey_manager) = setup_state();

        let result = process_command(
            &mut state,
            &mut hotkey_manager,
            &Command::Exec {
                command: "sleep 1000".to_string(),
                track: true,
            },
        );

        assert!(matches!(result.response, Response::Ok));
        assert_eq!(result.effects.len(), 1);

        match &result.effects[0] {
            Effect::ExecCommandTracked { command, .. } => {
                assert_eq!(command, "sleep 1000");
            }
            _ => panic!("Expected ExecCommandTracked effect"),
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

    #[test]
    fn test_window_swap_produces_retile_effect() {
        let (mut state, mut hotkey_manager) = setup_state();

        let result = process_command(
            &mut state,
            &mut hotkey_manager,
            &Command::WindowSwap {
                direction: Direction::Next,
            },
        );

        assert!(matches!(result.response, Response::Ok));
        assert_eq!(result.effects.len(), 1);

        match &result.effects[0] {
            Effect::RetileDisplays(display_ids) => {
                assert_eq!(display_ids.len(), 1);
                assert_eq!(display_ids[0], 1);
            }
            _ => panic!("Expected RetileDisplays effect"),
        }
    }

    #[test]
    fn test_window_swap_no_target_has_no_effects() {
        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 0.0, 0.0, 960.0, 1080.0,
            )])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws);

        let (tx, _rx) = std_mpsc::channel();
        let dummy_source = Arc::new(AtomicPtr::new(std::ptr::null_mut()));
        let mut hotkey_manager = HotkeyManager::new(tx, dummy_source);

        // Only one window, so swap should do nothing
        let result = process_command(
            &mut state,
            &mut hotkey_manager,
            &Command::WindowSwap {
                direction: Direction::Next,
            },
        );

        assert!(matches!(result.response, Response::Ok));
        assert!(result.effects.is_empty());
    }

    #[test]
    fn test_output_focus_with_window_produces_focus_effect() {
        use yashiki_ipc::OutputDirection;

        let ws = MockWindowSystem::new()
            .with_displays(vec![
                create_test_display(1, 0.0, 0.0, 1920.0, 1080.0),
                create_test_display(2, 1920.0, 0.0, 1920.0, 1080.0),
            ])
            .with_windows(vec![
                create_test_window(100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0),
                create_test_window(101, 1001, "Terminal", 2000.0, 100.0, 800.0, 600.0),
            ])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws);

        let (tx, _rx) = std_mpsc::channel();
        let dummy_source = Arc::new(AtomicPtr::new(std::ptr::null_mut()));
        let mut hotkey_manager = HotkeyManager::new(tx, dummy_source);

        let result = process_command(
            &mut state,
            &mut hotkey_manager,
            &Command::OutputFocus {
                direction: OutputDirection::Next,
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
                assert_eq!(*window_id, 101);
                assert_eq!(*pid, 1001);
                assert!(is_output_change);
            }
            _ => panic!("Expected FocusWindow effect"),
        }
    }

    #[test]
    fn test_output_focus_empty_display_produces_warp_cursor_effect() {
        use yashiki_ipc::OutputDirection;

        let ws = MockWindowSystem::new()
            .with_displays(vec![
                create_test_display(1, 0.0, 0.0, 1920.0, 1080.0),
                create_test_display(2, 1920.0, 0.0, 1920.0, 1080.0),
            ])
            .with_windows(vec![
                // Only window on display 1
                create_test_window(100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0),
            ])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws);

        let (tx, _rx) = std_mpsc::channel();
        let dummy_source = Arc::new(AtomicPtr::new(std::ptr::null_mut()));
        let mut hotkey_manager = HotkeyManager::new(tx, dummy_source);

        let result = process_command(
            &mut state,
            &mut hotkey_manager,
            &Command::OutputFocus {
                direction: OutputDirection::Next,
            },
        );

        assert!(matches!(result.response, Response::Ok));
        assert_eq!(result.effects.len(), 1);

        match &result.effects[0] {
            Effect::WarpCursorToDisplay { display_id } => {
                assert_eq!(*display_id, 2);
            }
            _ => panic!(
                "Expected WarpCursorToDisplay effect, got {:?}",
                result.effects[0]
            ),
        }
    }

    #[test]
    fn test_rect_center() {
        use crate::core::Rect;

        let rect = Rect {
            x: 100,
            y: 200,
            width: 800,
            height: 600,
        };

        let (cx, cy) = rect.center();
        assert_eq!(cx, 500); // 100 + 800/2
        assert_eq!(cy, 500); // 200 + 600/2
    }
}
