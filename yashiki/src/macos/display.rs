use core_foundation::{
    array::CFArray, base::TCFType, dictionary::CFDictionary, number::CFNumber, string::CFString,
};
use core_graphics::display::CGMainDisplayID;
use core_graphics::window::{
    kCGNullWindowID, kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly,
    CGWindowListCopyWindowInfo,
};
use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;

use super::get_bundle_id_for_pid;

pub type DisplayId = u32;

#[derive(Debug, Clone)]
pub struct DisplayInfo {
    pub id: DisplayId,
    pub name: String,
    pub frame: Bounds,
    pub is_main: bool,
}

#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub pid: i32,
    pub window_id: u32,
    pub name: Option<String>,
    pub owner_name: String,
    pub bundle_id: Option<String>,
    pub bounds: Bounds,
    pub layer: i32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub fn get_on_screen_windows() -> Vec<WindowInfo> {
    let options = kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements;
    let window_list: CFArray = unsafe {
        CFArray::wrap_under_create_rule(CGWindowListCopyWindowInfo(options, kCGNullWindowID))
    };

    let mut windows = Vec::new();

    for i in 0..window_list.len() {
        let dict_ptr = unsafe { *window_list.get_unchecked(i) };
        let dict: CFDictionary = unsafe { CFDictionary::wrap_under_get_rule(dict_ptr as *const _) };

        let Some(info) = parse_window_info(&dict) else {
            continue;
        };

        // Skip non-normal layer windows
        if info.layer != 0 {
            continue;
        }

        windows.push(info);
    }

    windows
}

/// Get all on-screen windows without layer filtering.
/// Used for --all option to include popup/utility windows.
pub fn get_all_windows_unfiltered() -> Vec<WindowInfo> {
    let options = kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements;
    let window_list: CFArray = unsafe {
        CFArray::wrap_under_create_rule(CGWindowListCopyWindowInfo(options, kCGNullWindowID))
    };

    let mut windows = Vec::new();

    for i in 0..window_list.len() {
        let dict_ptr = unsafe { *window_list.get_unchecked(i) };
        let dict: CFDictionary = unsafe { CFDictionary::wrap_under_get_rule(dict_ptr as *const _) };

        let Some(info) = parse_window_info(&dict) else {
            continue;
        };

        windows.push(info);
    }

    windows
}

fn parse_window_info(dict: &CFDictionary) -> Option<WindowInfo> {
    let pid = get_number(dict, "kCGWindowOwnerPID")?.to_i32()?;
    let window_id = get_number(dict, "kCGWindowNumber")?.to_i32()? as u32;
    let layer = get_number(dict, "kCGWindowLayer")?.to_i32()?;
    let owner_name = get_string(dict, "kCGWindowOwnerName")?;
    let name = get_string(dict, "kCGWindowName");
    let bounds = parse_bounds(dict, "kCGWindowBounds")?;
    let bundle_id = get_bundle_id_for_pid(pid);

    Some(WindowInfo {
        pid,
        window_id,
        name,
        owner_name,
        bundle_id,
        bounds,
        layer,
    })
}

fn get_number(dict: &CFDictionary, key: &str) -> Option<CFNumber> {
    let key = CFString::new(key);
    unsafe {
        let value = dict.find(key.as_concrete_TypeRef() as *const _)?;
        Some(CFNumber::wrap_under_get_rule(*value as *const _))
    }
}

fn get_string(dict: &CFDictionary, key: &str) -> Option<String> {
    let key = CFString::new(key);
    unsafe {
        let value = dict.find(key.as_concrete_TypeRef() as *const _)?;
        let cf_str = CFString::wrap_under_get_rule(*value as *const _);
        Some(cf_str.to_string())
    }
}

fn parse_bounds(dict: &CFDictionary, key: &str) -> Option<Bounds> {
    let key = CFString::new(key);
    unsafe {
        let value = dict.find(key.as_concrete_TypeRef() as *const _)?;
        let bounds_dict = CFDictionary::wrap_under_get_rule(*value as *const _);

        let x = get_number(&bounds_dict, "X")?.to_f64()?;
        let y = get_number(&bounds_dict, "Y")?.to_f64()?;
        let width = get_number(&bounds_dict, "Width")?.to_f64()?;
        let height = get_number(&bounds_dict, "Height")?.to_f64()?;

        Some(Bounds {
            x,
            y,
            width,
            height,
        })
    }
}

pub fn get_all_displays() -> Vec<DisplayInfo> {
    let main_display_id = unsafe { CGMainDisplayID() };

    // Get MainThreadMarker - this is safe because we're called from the main thread
    let mtm = unsafe { MainThreadMarker::new_unchecked() };

    // Use NSScreen to get visible frames (excluding menu bar and dock)
    let screens = NSScreen::screens(mtm);

    screens
        .iter()
        .filter_map(|screen| {
            // Get CGDirectDisplayID from screen's deviceDescription
            let display_id = get_display_id_for_screen(&screen)?;
            let visible_frame = screen.visibleFrame();

            // Get display name via localizedName (macOS 10.15+)
            let name = screen.localizedName().to_string();

            // NSScreen uses bottom-left origin, convert to top-left
            // visibleFrame.origin.y is distance from bottom of screen
            let full_frame = screen.frame();
            let top_left_y =
                full_frame.size.height - visible_frame.origin.y - visible_frame.size.height;

            Some(DisplayInfo {
                id: display_id,
                name,
                frame: Bounds {
                    x: visible_frame.origin.x,
                    y: top_left_y,
                    width: visible_frame.size.width,
                    height: visible_frame.size.height,
                },
                is_main: display_id == main_display_id,
            })
        })
        .collect()
}

fn get_display_id_for_screen(screen: &NSScreen) -> Option<DisplayId> {
    use objc2_foundation::NSNumber;

    let desc = screen.deviceDescription();
    let key = objc2_foundation::ns_string!("NSScreenNumber");
    let value = desc.objectForKey(key)?;

    // The value is an NSNumber containing the CGDirectDisplayID
    let number: &NSNumber = unsafe { &*(&*value as *const _ as *const NSNumber) };
    Some(number.unsignedIntValue())
}
