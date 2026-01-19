use core_foundation::{
    array::CFArray,
    base::{CFTypeID, TCFType},
    boolean::CFBoolean,
    declare_TCFType, impl_TCFType,
    runloop::CFRunLoopSource,
    string::{CFString, CFStringRef},
};
use core_graphics::geometry::{CGPoint, CGSize};
use std::ffi::c_void;
use std::ptr;

pub type AXError = i32;
pub const AX_ERROR_SUCCESS: AXError = 0;
pub const AX_ERROR_FAILURE: AXError = -25200;

#[repr(C)]
pub struct __AXUIElement(c_void);
pub type AXUIElementRef = *mut __AXUIElement;

#[repr(C)]
pub struct __AXObserver(c_void);
pub type AXObserverRef = *mut __AXObserver;

pub type AXObserverCallback = extern "C" fn(
    observer: AXObserverRef,
    element: AXUIElementRef,
    notification: CFStringRef,
    refcon: *mut c_void,
);

declare_TCFType!(AXUIElement, AXUIElementRef);
impl_TCFType!(AXUIElement, AXUIElementRef, AXUIElementGetTypeID);

declare_TCFType!(AXObserver, AXObserverRef);
impl_TCFType!(AXObserver, AXObserverRef, AXObserverGetTypeID);

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementGetTypeID() -> CFTypeID;
    fn AXObserverGetTypeID() -> CFTypeID;
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut *mut c_void,
    ) -> AXError;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *const c_void,
    ) -> AXError;
    fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> AXError;
    fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> AXError;
    fn AXValueCreate(value_type: u32, value: *const c_void) -> *mut c_void;
    fn AXValueGetValue(value: *const c_void, value_type: u32, value_ptr: *mut c_void) -> bool;

    fn AXObserverCreate(
        application: i32,
        callback: AXObserverCallback,
        out_observer: *mut AXObserverRef,
    ) -> AXError;
    fn AXObserverAddNotification(
        observer: AXObserverRef,
        element: AXUIElementRef,
        notification: CFStringRef,
        refcon: *mut c_void,
    ) -> AXError;
    fn AXObserverRemoveNotification(
        observer: AXObserverRef,
        element: AXUIElementRef,
        notification: CFStringRef,
    ) -> AXError;
    fn AXObserverGetRunLoopSource(observer: AXObserverRef) -> *mut c_void;

    // Private API to get CGWindowID from AXUIElement
    fn _AXUIElementGetWindow(element: AXUIElementRef, window_id: *mut u32) -> AXError;
}

const AX_VALUE_TYPE_CGPOINT: u32 = 1;
const AX_VALUE_TYPE_CGSIZE: u32 = 2;

mod action {
    pub const RAISE: &str = "AXRaise";
    pub const PRESS: &str = "AXPress";
}

mod attr {
    pub const WINDOWS: &str = "AXWindows";
    pub const FOCUSED_WINDOW: &str = "AXFocusedWindow";
    pub const FOCUSED_APPLICATION: &str = "AXFocusedApplication";
    pub const TITLE: &str = "AXTitle";
    pub const POSITION: &str = "AXPosition";
    pub const SIZE: &str = "AXSize";
    pub const MINIMIZED: &str = "AXMinimized";
    pub const CLOSE_BUTTON: &str = "AXCloseButton";
    pub const SUBROLE: &str = "AXSubrole";
    pub const IDENTIFIER: &str = "AXIdentifier";
    pub const FULLSCREEN_BUTTON: &str = "AXFullScreenButton";
    pub const MINIMIZE_BUTTON: &str = "AXMinimizeButton";
    pub const ZOOM_BUTTON: &str = "AXZoomButton";
    pub const ENABLED: &str = "AXEnabled";
}

pub mod notification {
    pub const WINDOW_CREATED: &str = "AXWindowCreated";
    pub const WINDOW_MOVED: &str = "AXWindowMoved";
    pub const WINDOW_RESIZED: &str = "AXWindowResized";
    pub const WINDOW_MINIATURIZED: &str = "AXWindowMiniaturized";
    pub const WINDOW_DEMINIATURIZED: &str = "AXWindowDeminiaturized";
    pub const FOCUSED_WINDOW_CHANGED: &str = "AXFocusedWindowChanged";
    pub const UI_ELEMENT_DESTROYED: &str = "AXUIElementDestroyed";
    pub const APPLICATION_ACTIVATED: &str = "AXApplicationActivated";
    pub const APPLICATION_DEACTIVATED: &str = "AXApplicationDeactivated";
    pub const APPLICATION_HIDDEN: &str = "AXApplicationHidden";
    pub const APPLICATION_SHOWN: &str = "AXApplicationShown";
}

pub fn is_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

pub fn is_trusted_with_prompt() -> bool {
    use core_foundation::dictionary::CFDictionary;

    let key = CFString::new("AXTrustedCheckOptionPrompt");
    let dict = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);

    unsafe { AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef() as *const c_void) }
}

impl AXUIElement {
    pub fn system_wide() -> Self {
        unsafe {
            let raw = AXUIElementCreateSystemWide();
            Self::wrap_under_create_rule(raw)
        }
    }

    pub fn application(pid: i32) -> Self {
        unsafe {
            let raw = AXUIElementCreateApplication(pid);
            Self::wrap_under_create_rule(raw)
        }
    }

    pub fn pid(&self) -> Result<i32, AXError> {
        let mut pid: i32 = 0;
        let err = unsafe { AXUIElementGetPid(self.as_concrete_TypeRef(), &mut pid) };
        if err == AX_ERROR_SUCCESS {
            Ok(pid)
        } else {
            Err(err)
        }
    }

    pub fn window_id(&self) -> Option<u32> {
        let mut wid: u32 = 0;
        let err = unsafe { _AXUIElementGetWindow(self.as_concrete_TypeRef(), &mut wid) };
        if err == AX_ERROR_SUCCESS {
            Some(wid)
        } else {
            None
        }
    }

    fn get_attribute(&self, name: &str) -> Result<*mut c_void, AXError> {
        let attr = CFString::new(name);
        let mut value: *mut c_void = ptr::null_mut();
        let err = unsafe {
            AXUIElementCopyAttributeValue(
                self.as_concrete_TypeRef(),
                attr.as_concrete_TypeRef(),
                &mut value,
            )
        };
        if err == AX_ERROR_SUCCESS && !value.is_null() {
            Ok(value)
        } else {
            Err(err)
        }
    }

    fn set_attribute(&self, name: &str, value: *const c_void) -> Result<(), AXError> {
        let attr = CFString::new(name);
        let err = unsafe {
            AXUIElementSetAttributeValue(
                self.as_concrete_TypeRef(),
                attr.as_concrete_TypeRef(),
                value,
            )
        };
        if err == AX_ERROR_SUCCESS {
            Ok(())
        } else {
            Err(err)
        }
    }

    pub fn title(&self) -> Result<String, AXError> {
        let value = self.get_attribute(attr::TITLE)?;
        let cf = unsafe { CFString::wrap_under_create_rule(value as *const _) };
        Ok(cf.to_string())
    }

    pub fn position(&self) -> Result<CGPoint, AXError> {
        let value = self.get_attribute(attr::POSITION)?;
        let mut point = CGPoint::new(0.0, 0.0);
        let ok = unsafe {
            AXValueGetValue(
                value,
                AX_VALUE_TYPE_CGPOINT,
                &mut point as *mut CGPoint as *mut c_void,
            )
        };
        if ok {
            Ok(point)
        } else {
            Err(AX_ERROR_FAILURE)
        }
    }

    pub fn size(&self) -> Result<CGSize, AXError> {
        let value = self.get_attribute(attr::SIZE)?;
        let mut size = CGSize::new(0.0, 0.0);
        let ok = unsafe {
            AXValueGetValue(
                value,
                AX_VALUE_TYPE_CGSIZE,
                &mut size as *mut CGSize as *mut c_void,
            )
        };
        if ok {
            Ok(size)
        } else {
            Err(AX_ERROR_FAILURE)
        }
    }

    pub fn set_position(&self, point: CGPoint) -> Result<(), AXError> {
        let value = unsafe {
            AXValueCreate(
                AX_VALUE_TYPE_CGPOINT,
                &point as *const CGPoint as *const c_void,
            )
        };
        if value.is_null() {
            return Err(AX_ERROR_FAILURE);
        }
        self.set_attribute(attr::POSITION, value)
    }

    pub fn set_size(&self, size: CGSize) -> Result<(), AXError> {
        let value = unsafe {
            AXValueCreate(
                AX_VALUE_TYPE_CGSIZE,
                &size as *const CGSize as *const c_void,
            )
        };
        if value.is_null() {
            return Err(AX_ERROR_FAILURE);
        }
        self.set_attribute(attr::SIZE, value)
    }

    pub fn is_minimized(&self) -> Result<bool, AXError> {
        let value = self.get_attribute(attr::MINIMIZED)?;
        let cf = unsafe { CFBoolean::wrap_under_create_rule(value as *const _) };
        Ok(cf.into())
    }

    pub fn set_minimized(&self, minimized: bool) -> Result<(), AXError> {
        let value = if minimized {
            CFBoolean::true_value()
        } else {
            CFBoolean::false_value()
        };
        self.set_attribute(attr::MINIMIZED, value.as_CFTypeRef())
    }

    pub fn windows(&self) -> Result<Vec<AXUIElement>, AXError> {
        let value = self.get_attribute(attr::WINDOWS)?;
        let arr: CFArray = unsafe { CFArray::wrap_under_create_rule(value as *const _) };
        let mut result = Vec::with_capacity(arr.len() as usize);
        for i in 0..arr.len() {
            let elem = unsafe {
                let ptr = *arr.get_unchecked(i);
                AXUIElement::wrap_under_get_rule(ptr as AXUIElementRef)
            };
            result.push(elem);
        }
        Ok(result)
    }

    pub fn focused_window(&self) -> Result<AXUIElement, AXError> {
        let value = self.get_attribute(attr::FOCUSED_WINDOW)?;
        Ok(unsafe { AXUIElement::wrap_under_create_rule(value as AXUIElementRef) })
    }

    pub fn focused_application(&self) -> Result<AXUIElement, AXError> {
        let value = self.get_attribute(attr::FOCUSED_APPLICATION)?;
        Ok(unsafe { AXUIElement::wrap_under_create_rule(value as AXUIElementRef) })
    }

    pub fn raise(&self) -> Result<(), AXError> {
        let action = CFString::new(action::RAISE);
        let err = unsafe {
            AXUIElementPerformAction(self.as_concrete_TypeRef(), action.as_concrete_TypeRef())
        };
        if err == AX_ERROR_SUCCESS {
            Ok(())
        } else {
            Err(err)
        }
    }

    pub fn press(&self) -> Result<(), AXError> {
        let action = CFString::new(action::PRESS);
        let err = unsafe {
            AXUIElementPerformAction(self.as_concrete_TypeRef(), action.as_concrete_TypeRef())
        };
        if err == AX_ERROR_SUCCESS {
            Ok(())
        } else {
            Err(err)
        }
    }

    pub fn close_button(&self) -> Result<AXUIElement, AXError> {
        let value = self.get_attribute(attr::CLOSE_BUTTON)?;
        Ok(unsafe { AXUIElement::wrap_under_create_rule(value as AXUIElementRef) })
    }

    pub fn subrole(&self) -> Result<String, AXError> {
        let value = self.get_attribute(attr::SUBROLE)?;
        let cf = unsafe { CFString::wrap_under_create_rule(value as *const _) };
        Ok(cf.to_string())
    }

    pub fn identifier(&self) -> Result<String, AXError> {
        let value = self.get_attribute(attr::IDENTIFIER)?;
        let cf = unsafe { CFString::wrap_under_create_rule(value as *const _) };
        Ok(cf.to_string())
    }

    pub fn has_close_button(&self) -> bool {
        match self.get_attribute(attr::CLOSE_BUTTON) {
            Ok(value) => {
                // Wrap to ensure proper release when dropped
                let _ = unsafe { AXUIElement::wrap_under_create_rule(value as AXUIElementRef) };
                true
            }
            Err(_) => false,
        }
    }

    pub fn has_fullscreen_button(&self) -> bool {
        match self.get_attribute(attr::FULLSCREEN_BUTTON) {
            Ok(value) => {
                let _ = unsafe { AXUIElement::wrap_under_create_rule(value as AXUIElementRef) };
                true
            }
            Err(_) => false,
        }
    }

    pub fn has_minimize_button(&self) -> bool {
        match self.get_attribute(attr::MINIMIZE_BUTTON) {
            Ok(value) => {
                let _ = unsafe { AXUIElement::wrap_under_create_rule(value as AXUIElementRef) };
                true
            }
            Err(_) => false,
        }
    }

    pub fn has_zoom_button(&self) -> bool {
        match self.get_attribute(attr::ZOOM_BUTTON) {
            Ok(value) => {
                let _ = unsafe { AXUIElement::wrap_under_create_rule(value as AXUIElementRef) };
                true
            }
            Err(_) => false,
        }
    }

    /// Check if this element is enabled (AXEnabled attribute)
    pub fn is_enabled(&self) -> Result<bool, AXError> {
        let value = self.get_attribute(attr::ENABLED)?;
        let cf = unsafe { CFBoolean::wrap_under_create_rule(value as *const _) };
        Ok(cf.into())
    }

    /// Get button info (exists + enabled) for close button
    pub fn get_close_button_info(&self) -> (bool, Option<bool>) {
        match self.get_attribute(attr::CLOSE_BUTTON) {
            Ok(value) => {
                let btn = unsafe { AXUIElement::wrap_under_create_rule(value as AXUIElementRef) };
                let enabled = btn.is_enabled().ok();
                (true, enabled)
            }
            Err(_) => (false, None),
        }
    }

    /// Get button info (exists + enabled) for fullscreen button
    pub fn get_fullscreen_button_info(&self) -> (bool, Option<bool>) {
        match self.get_attribute(attr::FULLSCREEN_BUTTON) {
            Ok(value) => {
                let btn = unsafe { AXUIElement::wrap_under_create_rule(value as AXUIElementRef) };
                let enabled = btn.is_enabled().ok();
                (true, enabled)
            }
            Err(_) => (false, None),
        }
    }

    /// Get button info (exists + enabled) for minimize button
    pub fn get_minimize_button_info(&self) -> (bool, Option<bool>) {
        match self.get_attribute(attr::MINIMIZE_BUTTON) {
            Ok(value) => {
                let btn = unsafe { AXUIElement::wrap_under_create_rule(value as AXUIElementRef) };
                let enabled = btn.is_enabled().ok();
                (true, enabled)
            }
            Err(_) => (false, None),
        }
    }

    /// Get button info (exists + enabled) for zoom button
    pub fn get_zoom_button_info(&self) -> (bool, Option<bool>) {
        match self.get_attribute(attr::ZOOM_BUTTON) {
            Ok(value) => {
                let btn = unsafe { AXUIElement::wrap_under_create_rule(value as AXUIElementRef) };
                let enabled = btn.is_enabled().ok();
                (true, enabled)
            }
            Err(_) => (false, None),
        }
    }
}

impl AXObserver {
    pub fn new(pid: i32, callback: AXObserverCallback) -> Result<Self, AXError> {
        let mut observer: AXObserverRef = ptr::null_mut();
        let err = unsafe { AXObserverCreate(pid, callback, &mut observer) };
        if err == AX_ERROR_SUCCESS && !observer.is_null() {
            Ok(unsafe { Self::wrap_under_create_rule(observer) })
        } else {
            Err(err)
        }
    }

    pub fn add_notification(
        &self,
        element: &AXUIElement,
        notification: &str,
        refcon: *mut c_void,
    ) -> Result<(), AXError> {
        let notif = CFString::new(notification);
        let err = unsafe {
            AXObserverAddNotification(
                self.as_concrete_TypeRef(),
                element.as_concrete_TypeRef(),
                notif.as_concrete_TypeRef(),
                refcon,
            )
        };
        if err == AX_ERROR_SUCCESS {
            Ok(())
        } else {
            Err(err)
        }
    }

    pub fn remove_notification(
        &self,
        element: &AXUIElement,
        notification: &str,
    ) -> Result<(), AXError> {
        let notif = CFString::new(notification);
        let err = unsafe {
            AXObserverRemoveNotification(
                self.as_concrete_TypeRef(),
                element.as_concrete_TypeRef(),
                notif.as_concrete_TypeRef(),
            )
        };
        if err == AX_ERROR_SUCCESS {
            Ok(())
        } else {
            Err(err)
        }
    }

    pub fn run_loop_source(&self) -> CFRunLoopSource {
        unsafe {
            let source = AXObserverGetRunLoopSource(self.as_concrete_TypeRef());
            CFRunLoopSource::wrap_under_get_rule(source as *mut _)
        }
    }
}

pub fn get_focused_window() -> Result<AXUIElement, AXError> {
    // Use NSWorkspace as primary method (more robust for Electron apps like Teams)
    if let Some(pid) = super::workspace::get_frontmost_app_pid() {
        let app = AXUIElement::application(pid);
        match app.focused_window() {
            Ok(win) => return Ok(win),
            Err(e) => {
                tracing::debug!(
                    "focused_window via NSWorkspace failed for pid {}: {}",
                    pid,
                    e
                );
            }
        }
    }

    // Fallback: use accessibility API directly
    let _ = super::display::get_on_screen_windows();
    let system = AXUIElement::system_wide();
    let app = match system.focused_application() {
        Ok(app) => app,
        Err(e) => {
            tracing::error!("focused_application failed: {}", e);
            return Err(e);
        }
    };
    match app.focused_window() {
        Ok(win) => Ok(win),
        Err(e) => {
            tracing::error!("focused_window failed: {}", e);
            Err(e)
        }
    }
}
