use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;

use core_foundation::runloop::{
    CFRunLoopGetMain, CFRunLoopSourceRef, CFRunLoopSourceSignal, CFRunLoopWakeUp,
};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, DefinedClass};
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};
use objc2_foundation::{MainThreadMarker, NSNotification, NSObject, NSObjectProtocol, NSString};

pub fn get_frontmost_app_pid() -> Option<i32> {
    let workspace = NSWorkspace::sharedWorkspace();
    workspace
        .frontmostApplication()
        .map(|app| app.processIdentifier())
}

/// Get the bundle identifier for an application with the given PID.
pub fn get_bundle_id_for_pid(pid: i32) -> Option<String> {
    let workspace = NSWorkspace::sharedWorkspace();
    let apps = workspace.runningApplications();
    for app in apps {
        if app.processIdentifier() == pid {
            return app.bundleIdentifier().map(|s| s.to_string());
        }
    }
    None
}

#[allow(deprecated)]
pub fn activate_application(pid: i32) -> bool {
    let workspace = NSWorkspace::sharedWorkspace();
    let apps = workspace.runningApplications();
    for app in apps {
        if app.processIdentifier() == pid {
            return app
                .activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
        }
    }
    false
}

pub fn exec_command(command: &str, path: &str) -> Result<(), String> {
    let mut cmd = std::process::Command::new("/bin/bash");
    cmd.arg("-c").arg(command);

    if !path.is_empty() {
        cmd.env("PATH", path);
    }

    match cmd.spawn() {
        Ok(_) => {
            tracing::info!("Executed command: {}", command);
            Ok(())
        }
        Err(e) => {
            let msg = format!("Failed to execute command '{}': {}", command, e);
            tracing::error!("{}", msg);
            Err(msg)
        }
    }
}

pub fn exec_command_tracked(command: &str, path: &str) -> Result<u32, String> {
    let mut cmd = std::process::Command::new("/bin/bash");
    cmd.arg("-c").arg(command);

    if !path.is_empty() {
        cmd.env("PATH", path);
    }

    match cmd.spawn() {
        Ok(child) => {
            let pid = child.id();
            tracing::info!("Executed tracked command: {} (pid={})", command, pid);
            Ok(pid)
        }
        Err(e) => {
            let msg = format!("Failed to execute command '{}': {}", command, e);
            tracing::error!("{}", msg);
            Err(msg)
        }
    }
}

pub fn terminate_process(pid: u32) {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    let nix_pid = Pid::from_raw(pid as i32);
    match kill(nix_pid, Signal::SIGTERM) {
        Ok(()) => {
            tracing::info!("Sent SIGTERM to process {}", pid);
        }
        Err(e) => {
            tracing::warn!("Failed to terminate process {}: {}", pid, e);
        }
    }
}

#[derive(Debug, Clone)]
pub enum WorkspaceEvent {
    AppLaunched { pid: i32 },
    AppTerminated { pid: i32 },
    AppActivated { pid: i32 },
    DisplaysChanged,
}

struct Ivars {
    event_tx: RefCell<Option<std_mpsc::Sender<WorkspaceEvent>>>,
    source_ptr: Arc<AtomicPtr<c_void>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[ivars = Ivars]
    struct WorkspaceObserver;

    unsafe impl NSObjectProtocol for WorkspaceObserver {}

    impl WorkspaceObserver {
        #[unsafe(method(appLaunched:))]
        fn app_launched(&self, notification: &NSNotification) {
            if let Some(pid) = get_pid_from_notification(notification) {
                tracing::debug!("App launched: pid {}", pid);
                let tx = self.ivars().event_tx.borrow();
                if let Some(sender) = tx.as_ref() {
                    let _: Result<(), _> = sender.send(WorkspaceEvent::AppLaunched { pid });
                }
                signal_runloop_source(&self.ivars().source_ptr);
            }
        }

        #[unsafe(method(appTerminated:))]
        fn app_terminated(&self, notification: &NSNotification) {
            if let Some(pid) = get_pid_from_notification(notification) {
                tracing::debug!("App terminated: pid {}", pid);
                let tx = self.ivars().event_tx.borrow();
                if let Some(sender) = tx.as_ref() {
                    let _: Result<(), _> = sender.send(WorkspaceEvent::AppTerminated { pid });
                }
                signal_runloop_source(&self.ivars().source_ptr);
            }
        }

        #[unsafe(method(appActivated:))]
        fn app_activated(&self, notification: &NSNotification) {
            if let Some(pid) = get_pid_from_notification(notification) {
                tracing::debug!("App activated: pid {}", pid);
                let tx = self.ivars().event_tx.borrow();
                if let Some(sender) = tx.as_ref() {
                    let _: Result<(), _> = sender.send(WorkspaceEvent::AppActivated { pid });
                }
                signal_runloop_source(&self.ivars().source_ptr);
            }
        }

        #[unsafe(method(displaysChanged:))]
        fn displays_changed(&self, _notification: &NSNotification) {
            tracing::debug!("Screen parameters changed");
            let tx = self.ivars().event_tx.borrow();
            if let Some(sender) = tx.as_ref() {
                let _: Result<(), _> = sender.send(WorkspaceEvent::DisplaysChanged);
            }
            signal_runloop_source(&self.ivars().source_ptr);
        }
    }
);

fn signal_runloop_source(source_ptr: &Arc<AtomicPtr<c_void>>) {
    let source = source_ptr.load(Ordering::Acquire);
    if !source.is_null() {
        unsafe {
            CFRunLoopSourceSignal(source as CFRunLoopSourceRef);
            CFRunLoopWakeUp(CFRunLoopGetMain());
        }
    }
}

fn get_pid_from_notification(notification: &NSNotification) -> Option<i32> {
    unsafe {
        let user_info = notification.userInfo()?;
        let key = NSString::from_str("NSWorkspaceApplicationKey");
        let app: Option<Retained<NSRunningApplication>> =
            msg_send![&user_info, objectForKey: &*key];
        app.map(|a| a.processIdentifier())
    }
}

impl WorkspaceObserver {
    fn new(
        event_tx: std_mpsc::Sender<WorkspaceEvent>,
        source_ptr: Arc<AtomicPtr<c_void>>,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let this = mtm.alloc::<Self>();
        let this = this.set_ivars(Ivars {
            event_tx: RefCell::new(Some(event_tx)),
            source_ptr,
        });
        unsafe { msg_send![super(this), init] }
    }
}

pub struct WorkspaceWatcher {
    _observer: Retained<WorkspaceObserver>,
}

impl WorkspaceWatcher {
    pub fn new(
        event_tx: std_mpsc::Sender<WorkspaceEvent>,
        source_ptr: Arc<AtomicPtr<c_void>>,
        mtm: MainThreadMarker,
    ) -> Self {
        let observer = WorkspaceObserver::new(event_tx, source_ptr, mtm);

        unsafe {
            let workspace = NSWorkspace::sharedWorkspace();
            let workspace_center = workspace.notificationCenter();

            let launch_name = NSString::from_str("NSWorkspaceDidLaunchApplicationNotification");
            let terminate_name =
                NSString::from_str("NSWorkspaceDidTerminateApplicationNotification");

            let observer_obj: &AnyObject =
                std::mem::transmute::<&WorkspaceObserver, &AnyObject>(&*observer);

            workspace_center.addObserver_selector_name_object(
                observer_obj,
                sel!(appLaunched:),
                Some(&launch_name),
                None,
            );

            workspace_center.addObserver_selector_name_object(
                observer_obj,
                sel!(appTerminated:),
                Some(&terminate_name),
                None,
            );

            let activate_name = NSString::from_str("NSWorkspaceDidActivateApplicationNotification");

            workspace_center.addObserver_selector_name_object(
                observer_obj,
                sel!(appActivated:),
                Some(&activate_name),
                None,
            );

            // Register for screen change notifications using default notification center.
            // Note: This notification doesn't work without NSApplication's event loop.
            // Display changes are detected via polling in timer_callback instead.
            let default_center = objc2_foundation::NSNotificationCenter::defaultCenter();
            let screen_changed_name =
                NSString::from_str("NSApplicationDidChangeScreenParametersNotification");

            default_center.addObserver_selector_name_object(
                observer_obj,
                sel!(displaysChanged:),
                Some(&screen_changed_name),
                None,
            );
        }

        tracing::info!("Workspace watcher started");

        Self {
            _observer: observer,
        }
    }
}
