use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::mpsc as std_mpsc;

use core_foundation::base::TCFType;
use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoop};
use core_foundation::string::{CFString, CFStringRef};

use crate::event::Event;
use crate::macos::accessibility::{
    notification, AXObserver, AXObserverRef, AXUIElement, AXUIElementRef,
};
use crate::macos::display::get_on_screen_windows;

pub struct ObserverManager {
    observers: HashMap<i32, AXObserver>,
    event_tx: std_mpsc::Sender<Event>,
}

struct CallbackContext {
    pid: i32,
    event_tx: std_mpsc::Sender<Event>,
}

impl ObserverManager {
    pub fn new(event_tx: std_mpsc::Sender<Event>) -> Self {
        Self {
            observers: HashMap::new(),
            event_tx,
        }
    }

    pub fn start(&mut self) {
        let windows = get_on_screen_windows();
        let mut pids: Vec<i32> = windows.iter().map(|w| w.pid).collect();
        pids.sort();
        pids.dedup();

        tracing::info!("Starting observers for {} applications", pids.len());

        for pid in pids {
            if let Err(e) = self.add_observer(pid) {
                tracing::warn!("Failed to add observer for pid {}: {}", pid, e);
            }
        }
    }

    pub fn add_observer(&mut self, pid: i32) -> Result<(), i32> {
        if self.observers.contains_key(&pid) {
            return Ok(());
        }

        let observer = AXObserver::new(pid, observer_callback)?;
        let app = AXUIElement::application(pid);

        let context = Box::new(CallbackContext {
            pid,
            event_tx: self.event_tx.clone(),
        });
        let refcon = Box::into_raw(context) as *mut c_void;

        let notifications = [
            notification::WINDOW_CREATED,
            notification::WINDOW_MOVED,
            notification::WINDOW_RESIZED,
            notification::WINDOW_MINIATURIZED,
            notification::WINDOW_DEMINIATURIZED,
            notification::FOCUSED_WINDOW_CHANGED,
            notification::UI_ELEMENT_DESTROYED,
            notification::APPLICATION_ACTIVATED,
            notification::APPLICATION_DEACTIVATED,
            notification::APPLICATION_HIDDEN,
            notification::APPLICATION_SHOWN,
        ];

        for notif in notifications {
            if let Err(e) = observer.add_notification(&app, notif, refcon) {
                tracing::debug!(
                    "Failed to add notification {} for pid {}: {}",
                    notif,
                    pid,
                    e
                );
            }
        }

        let run_loop = CFRunLoop::get_current();
        let source = observer.run_loop_source();
        run_loop.add_source(&source, unsafe { kCFRunLoopDefaultMode });

        self.observers.insert(pid, observer);
        tracing::debug!("Added observer for pid {}", pid);

        Ok(())
    }

    pub fn remove_observer(&mut self, pid: i32) {
        if self.observers.remove(&pid).is_some() {
            tracing::debug!("Removed observer for pid {}", pid);
        }
    }
}

extern "C" fn observer_callback(
    _observer: AXObserverRef,
    _element: AXUIElementRef,
    notification: CFStringRef,
    refcon: *mut c_void,
) {
    if refcon.is_null() {
        return;
    }

    let context = unsafe { &*(refcon as *const CallbackContext) };
    let notif = unsafe { CFString::wrap_under_get_rule(notification) };
    let notif_str = notif.to_string();

    let event = match notif_str.as_str() {
        notification::WINDOW_CREATED => Some(Event::WindowCreated { pid: context.pid }),
        notification::UI_ELEMENT_DESTROYED => Some(Event::WindowDestroyed { pid: context.pid }),
        notification::FOCUSED_WINDOW_CHANGED => Some(Event::FocusedWindowChanged),
        notification::WINDOW_MOVED => Some(Event::WindowMoved { pid: context.pid }),
        notification::WINDOW_RESIZED => Some(Event::WindowResized { pid: context.pid }),
        notification::WINDOW_MINIATURIZED => Some(Event::WindowMiniaturized { pid: context.pid }),
        notification::WINDOW_DEMINIATURIZED => {
            Some(Event::WindowDeminiaturized { pid: context.pid })
        }
        notification::APPLICATION_ACTIVATED => {
            Some(Event::ApplicationActivated { pid: context.pid })
        }
        notification::APPLICATION_DEACTIVATED => Some(Event::ApplicationDeactivated),
        notification::APPLICATION_HIDDEN => Some(Event::ApplicationHidden),
        notification::APPLICATION_SHOWN => Some(Event::ApplicationShown),
        _ => {
            tracing::debug!("Unknown notification: {}", notif_str);
            None
        }
    };

    if let Some(event) = event {
        tracing::debug!("Observer event: {:?}", event);
        if let Err(e) = context.event_tx.send(event) {
            tracing::error!("Failed to send event: {}", e);
        }
    }
}
