use core_foundation::{
    array::CFArray,
    base::{CFTypeID, TCFType},
    boolean::CFBoolean,
    declare_TCFType, impl_TCFType,
    string::{CFString, CFStringRef},
};
use core_graphics::geometry::{CGPoint, CGSize};
use std::ffi::c_void;
use std::ptr;

pub type AXError = i32;
pub const AX_ERROR_SUCCESS: AXError = 0;
pub const AX_ERROR_FAILURE: AXError = -25200;
pub const AX_ERROR_INVALID_UIELEMENT: AXError = -25202;
pub const AX_ERROR_ATTRIBUTE_UNSUPPORTED: AXError = -25205;
pub const AX_ERROR_NO_VALUE: AXError = -25212;

#[repr(C)]
pub struct __AXUIElement(c_void);
pub type AXUIElementRef = *mut __AXUIElement;

declare_TCFType!(AXUIElement, AXUIElementRef);
impl_TCFType!(AXUIElement, AXUIElementRef, AXUIElementGetTypeID);

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementGetTypeID() -> CFTypeID;
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
    fn AXValueCreate(value_type: u32, value: *const c_void) -> *mut c_void;
    fn AXValueGetValue(value: *const c_void, value_type: u32, value_ptr: *mut c_void) -> bool;
}

const AX_VALUE_TYPE_CGPOINT: u32 = 1;
const AX_VALUE_TYPE_CGSIZE: u32 = 2;

mod attr {
    pub const WINDOWS: &str = "AXWindows";
    pub const FOCUSED_WINDOW: &str = "AXFocusedWindow";
    pub const FOCUSED_APPLICATION: &str = "AXFocusedApplication";
    pub const TITLE: &str = "AXTitle";
    pub const POSITION: &str = "AXPosition";
    pub const SIZE: &str = "AXSize";
    pub const MINIMIZED: &str = "AXMinimized";
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
}

pub fn get_focused_window() -> Result<AXUIElement, AXError> {
    let system = AXUIElement::system_wide();
    let app = system.focused_application()?;
    app.focused_window()
}
