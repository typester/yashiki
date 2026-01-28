use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use core_foundation::base::TCFType;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop, CFRunLoopSource};
use core_foundation_sys::mach_port::CFMachPortRef;
use core_foundation_sys::runloop::{CFRunLoopSourceRef, CFRunLoopSourceSignal};
use core_graphics::event::{
    CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CallbackResult,
};

extern "C" {
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MousePosition {
    pub x: i32,
    pub y: i32,
}

pub struct MouseTracker {
    event_tx: mpsc::Sender<MousePosition>,
    tap: Option<MouseTap>,
    runloop_source: Arc<AtomicPtr<c_void>>,
    last_position: Arc<std::sync::Mutex<Option<MousePosition>>>,
}

impl MouseTracker {
    pub fn new(
        event_tx: mpsc::Sender<MousePosition>,
        runloop_source: Arc<AtomicPtr<c_void>>,
    ) -> Self {
        Self {
            event_tx,
            tap: None,
            runloop_source,
            last_position: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub fn start(&mut self) -> Result<(), String> {
        if self.tap.is_some() {
            return Ok(());
        }
        self.tap = Some(self.create_tap()?);
        tracing::info!("Mouse tracker started");
        Ok(())
    }

    pub fn stop(&mut self) {
        if self.tap.take().is_some() {
            tracing::info!("Mouse tracker stopped");
        }
    }

    pub fn is_running(&self) -> bool {
        self.tap.is_some()
    }

    fn create_tap(&self) -> Result<MouseTap, String> {
        let tx = self.event_tx.clone();
        let source = Arc::clone(&self.runloop_source);
        let last_pos = Arc::clone(&self.last_position);

        let mach_port_ptr: Arc<AtomicPtr<c_void>> = Arc::new(AtomicPtr::new(ptr::null_mut()));
        let mach_port_for_callback = Arc::clone(&mach_port_ptr);

        // Throttle threshold: only send if position changed by at least 5px
        const THROTTLE_THRESHOLD: i32 = 5;

        let tap = CGEventTap::new(
            CGEventTapLocation::Session,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            vec![CGEventType::MouseMoved],
            move |_proxy, event_type, event| {
                match event_type {
                    CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
                        let reason = if matches!(event_type, CGEventType::TapDisabledByTimeout) {
                            "timeout"
                        } else {
                            "user input"
                        };
                        tracing::warn!("Mouse event tap disabled by {}, re-enabling...", reason);
                        let ptr = mach_port_for_callback.load(Ordering::Acquire);
                        if !ptr.is_null() {
                            unsafe {
                                CGEventTapEnable(ptr as CFMachPortRef, true);
                            }
                        }
                        return CallbackResult::Keep;
                    }
                    _ => {}
                }

                let location = event.location();
                let new_pos = MousePosition {
                    x: location.x as i32,
                    y: location.y as i32,
                };

                // Throttle: only send if position changed significantly
                let should_send = {
                    let mut last = last_pos.lock().unwrap();
                    match *last {
                        Some(prev) => {
                            let dx = (new_pos.x - prev.x).abs();
                            let dy = (new_pos.y - prev.y).abs();
                            if dx >= THROTTLE_THRESHOLD || dy >= THROTTLE_THRESHOLD {
                                *last = Some(new_pos);
                                true
                            } else {
                                false
                            }
                        }
                        None => {
                            *last = Some(new_pos);
                            true
                        }
                    }
                };

                if should_send && tx.send(new_pos).is_ok() {
                    // Signal CFRunLoopSource for immediate processing
                    let source_ptr = source.load(Ordering::Acquire);
                    if !source_ptr.is_null() {
                        unsafe {
                            CFRunLoopSourceSignal(source_ptr as CFRunLoopSourceRef);
                        }
                    }
                }

                CallbackResult::Keep
            },
        )
        .map_err(|_| {
            "Failed to create mouse event tap. Make sure Accessibility permission is granted."
        })?;

        mach_port_ptr.store(
            tap.mach_port().as_concrete_TypeRef() as *mut c_void,
            Ordering::Release,
        );

        tap.enable();

        let source = tap
            .mach_port()
            .create_runloop_source(0)
            .map_err(|_| "Failed to create run loop source for mouse tracker")?;

        CFRunLoop::get_current().add_source(&source, unsafe { kCFRunLoopCommonModes });

        Ok(MouseTap {
            _tap: tap,
            _source: source,
        })
    }
}

struct MouseTap {
    _tap: CGEventTap<'static>,
    _source: CFRunLoopSource,
}
