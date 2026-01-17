use core_foundation::{
    array::CFArray, base::TCFType, dictionary::CFDictionary, number::CFNumber, string::CFString,
};
use core_graphics::display::{CGDisplay, CGGetActiveDisplayList, CGMainDisplayID};
use core_graphics::geometry::CGRect;
use core_graphics::window::{
    kCGNullWindowID, kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly,
    CGWindowListCopyWindowInfo,
};
use std::collections::HashSet;

pub type DisplayId = u32;

#[derive(Debug, Clone)]
pub struct DisplayInfo {
    pub id: DisplayId,
    pub frame: Bounds,
    pub is_main: bool,
}

#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub pid: i32,
    pub window_id: u32,
    pub name: Option<String>,
    pub owner_name: String,
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

        if info.layer != 0 {
            continue;
        }

        windows.push(info);
    }

    windows
}

pub fn get_running_app_pids() -> Vec<i32> {
    let windows = get_on_screen_windows();
    let pids: HashSet<i32> = windows.iter().map(|w| w.pid).collect();
    pids.into_iter().collect()
}

fn parse_window_info(dict: &CFDictionary) -> Option<WindowInfo> {
    let pid = get_number(dict, "kCGWindowOwnerPID")?.to_i32()?;
    let window_id = get_number(dict, "kCGWindowNumber")?.to_i32()? as u32;
    let layer = get_number(dict, "kCGWindowLayer")?.to_i32()?;
    let owner_name = get_string(dict, "kCGWindowOwnerName")?;
    let name = get_string(dict, "kCGWindowName");
    let bounds = parse_bounds(dict, "kCGWindowBounds")?;

    Some(WindowInfo {
        pid,
        window_id,
        name,
        owner_name,
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

pub fn get_main_display_size() -> (u32, u32) {
    let display_id = unsafe { CGMainDisplayID() };
    let display = CGDisplay::new(display_id);
    let width = display.pixels_wide() as u32;
    let height = display.pixels_high() as u32;
    (width, height)
}

pub fn get_all_displays() -> Vec<DisplayInfo> {
    let main_display_id = unsafe { CGMainDisplayID() };

    // Get count of active displays
    let mut display_count: u32 = 0;
    unsafe {
        CGGetActiveDisplayList(0, std::ptr::null_mut(), &mut display_count);
    }

    if display_count == 0 {
        return vec![];
    }

    // Get all display IDs
    let mut display_ids: Vec<u32> = vec![0; display_count as usize];
    unsafe {
        CGGetActiveDisplayList(display_count, display_ids.as_mut_ptr(), &mut display_count);
    }

    display_ids
        .into_iter()
        .map(|id| {
            let display = CGDisplay::new(id);
            let bounds: CGRect = display.bounds();
            DisplayInfo {
                id,
                frame: Bounds {
                    x: bounds.origin.x,
                    y: bounds.origin.y,
                    width: bounds.size.width,
                    height: bounds.size.height,
                },
                is_main: id == main_display_id,
            }
        })
        .collect()
}
