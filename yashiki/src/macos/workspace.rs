use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, DefinedClass};
use objc2_app_kit::{NSRunningApplication, NSWorkspace};
use objc2_foundation::{MainThreadMarker, NSNotification, NSObject, NSObjectProtocol, NSString};
use std::cell::RefCell;
use std::sync::mpsc as std_mpsc;

#[derive(Debug, Clone)]
pub enum WorkspaceEvent {
    AppLaunched { pid: i32 },
    AppTerminated { pid: i32 },
}

struct Ivars {
    event_tx: RefCell<Option<std_mpsc::Sender<WorkspaceEvent>>>,
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
            }
        }
    }
);

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
    fn new(event_tx: std_mpsc::Sender<WorkspaceEvent>, mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc::<Self>();
        let this = this.set_ivars(Ivars {
            event_tx: RefCell::new(Some(event_tx)),
        });
        unsafe { msg_send![super(this), init] }
    }
}

pub struct WorkspaceWatcher {
    _observer: Retained<WorkspaceObserver>,
}

impl WorkspaceWatcher {
    pub fn new(event_tx: std_mpsc::Sender<WorkspaceEvent>, mtm: MainThreadMarker) -> Self {
        let observer = WorkspaceObserver::new(event_tx, mtm);

        unsafe {
            let workspace = NSWorkspace::sharedWorkspace();
            let center = workspace.notificationCenter();

            let launch_name = NSString::from_str("NSWorkspaceDidLaunchApplicationNotification");
            let terminate_name =
                NSString::from_str("NSWorkspaceDidTerminateApplicationNotification");

            let observer_obj: &AnyObject =
                std::mem::transmute::<&WorkspaceObserver, &AnyObject>(&*observer);

            center.addObserver_selector_name_object(
                observer_obj,
                sel!(appLaunched:),
                Some(&launch_name),
                None,
            );

            center.addObserver_selector_name_object(
                observer_obj,
                sel!(appTerminated:),
                Some(&terminate_name),
                None,
            );
        }

        tracing::info!("Workspace watcher started");

        Self {
            _observer: observer,
        }
    }
}
