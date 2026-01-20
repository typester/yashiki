use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::mpsc::Sender;
use std::sync::OnceLock;

use core_foundation::{
    array::CFArray, base::TCFType, dictionary::CFDictionary, number::CFNumber, string::CFString,
};
use core_graphics::display::{CGDirectDisplayID, CGDisplayBounds, CGMainDisplayID};
use core_graphics::window::{
    kCGNullWindowID, kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly,
    CGWindowListCopyWindowInfo,
};
use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;

use super::get_bundle_id_for_pid;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGDisplayRegisterReconfigurationCallback(
        callback: unsafe extern "C" fn(CGDirectDisplayID, u32, *mut c_void),
        user_info: *mut c_void,
    ) -> i32;
}

#[derive(Debug, Clone)]
pub struct DisplayReconfigEvent {
    pub display_id: DisplayId,
    pub flags: u32,
}

static DISPLAY_RECONFIG_TX: OnceLock<Sender<DisplayReconfigEvent>> = OnceLock::new();

extern "C" fn display_reconfig_callback(
    display_id: CGDirectDisplayID,
    flags: u32,
    _user_info: *mut c_void,
) {
    if let Some(tx) = DISPLAY_RECONFIG_TX.get() {
        let _ = tx.send(DisplayReconfigEvent { display_id, flags });
    }
}

pub fn register_display_callback(tx: Sender<DisplayReconfigEvent>) -> anyhow::Result<()> {
    DISPLAY_RECONFIG_TX
        .set(tx)
        .map_err(|_| anyhow::anyhow!("Display callback already registered"))?;

    let result = unsafe {
        CGDisplayRegisterReconfigurationCallback(display_reconfig_callback, std::ptr::null_mut())
    };

    if result != 0 {
        anyhow::bail!("Failed to register display callback: {}", result);
    }

    tracing::info!("Display reconfiguration callback registered");
    Ok(())
}

pub type DisplayId = u32;

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, Copy, Default, PartialEq)]
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
    let display_ids = get_active_display_ids();
    if display_ids.is_empty() {
        return Vec::new();
    }

    let main_display_id = unsafe { CGMainDisplayID() };
    let menu_bar_heights = detect_menu_bar_heights();

    // Get display names from NSScreen (names don't change with resolution)
    let display_names = get_display_names();

    display_ids
        .iter()
        .map(|&display_id| {
            let bounds = get_display_bounds(display_id);
            let menu_bar_height = menu_bar_heights.get(&display_id).copied().unwrap_or(0.0);

            // Visible frame = full bounds minus menu bar at top
            let visible_y = bounds.y + menu_bar_height;
            let visible_height = bounds.height - menu_bar_height;

            let name = display_names
                .get(&display_id)
                .cloned()
                .unwrap_or_else(|| format!("Display {}", display_id));

            DisplayInfo {
                id: display_id,
                name,
                frame: Bounds {
                    x: bounds.x,
                    y: visible_y,
                    width: bounds.width,
                    height: visible_height,
                },
                is_main: display_id == main_display_id,
            }
        })
        .collect()
}

/// Get display names from NSScreen (best effort, may be cached but names don't change)
fn get_display_names() -> HashMap<DisplayId, String> {
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let screens = NSScreen::screens(mtm);

    screens
        .iter()
        .filter_map(|screen| {
            let display_id = get_display_id_for_screen(&screen)?;
            let name = screen.localizedName().to_string();
            Some((display_id, name))
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

/// Get active display IDs using Core Graphics directly.
/// Unlike NSScreen::screens(), this doesn't depend on NSApplication's event loop.
pub fn get_active_display_ids() -> Vec<DisplayId> {
    use core_graphics::display::CGGetActiveDisplayList;

    const MAX_DISPLAYS: u32 = 16;
    let mut display_ids: [u32; 16] = [0; 16];
    let mut display_count: u32 = 0;

    let result = unsafe {
        CGGetActiveDisplayList(MAX_DISPLAYS, display_ids.as_mut_ptr(), &mut display_count)
    };

    if result != 0 {
        return Vec::new();
    }

    display_ids[..display_count as usize].to_vec()
}

/// Get display bounds in Core Graphics coordinates using CGDisplayBounds.
fn get_display_bounds(display_id: DisplayId) -> Bounds {
    let rect = unsafe { CGDisplayBounds(display_id) };
    Bounds {
        x: rect.origin.x,
        y: rect.origin.y,
        width: rect.size.width,
        height: rect.size.height,
    }
}

/// Detect menu bar heights for each display by looking at Window Server windows.
/// Menu bars are at layer 24, owned by "Window Server", thin (height < 50) and screen-wide.
/// Returns a map of display_id -> menu_bar_height.
fn detect_menu_bar_heights() -> HashMap<DisplayId, f64> {
    let active_display_ids = get_active_display_ids();
    if active_display_ids.is_empty() {
        return HashMap::new();
    }

    // Get display bounds for all active displays
    let display_bounds: Vec<(DisplayId, Bounds)> = active_display_ids
        .iter()
        .map(|&id| (id, get_display_bounds(id)))
        .collect();

    let options = kCGWindowListOptionOnScreenOnly;
    let window_list: CFArray = unsafe {
        CFArray::wrap_under_create_rule(CGWindowListCopyWindowInfo(options, kCGNullWindowID))
    };

    let mut menu_bar_heights: HashMap<DisplayId, f64> = HashMap::new();

    for i in 0..window_list.len() {
        let dict_ptr = unsafe { *window_list.get_unchecked(i) };
        let dict: CFDictionary = unsafe { CFDictionary::wrap_under_get_rule(dict_ptr as *const _) };

        // Check if this is a Window Server window at layer 24 (menu bar layer)
        let Some(layer) = get_number(&dict, "kCGWindowLayer").and_then(|n| n.to_i32()) else {
            continue;
        };
        if layer != 24 {
            continue;
        }

        let Some(owner_name) = get_string(&dict, "kCGWindowOwnerName") else {
            continue;
        };
        if owner_name != "Window Server" {
            continue;
        }

        let Some(bounds) = parse_bounds(&dict, "kCGWindowBounds") else {
            continue;
        };

        // Menu bar should be thin (height < 50) and wide (width > 500)
        if bounds.height >= 50.0 || bounds.width <= 500.0 {
            continue;
        }

        // Match to display by comparing position and width
        for &(display_id, ref display) in &display_bounds {
            if (bounds.x - display.x).abs() < 1.0
                && (bounds.y - display.y).abs() < 1.0
                && (bounds.width - display.width).abs() < 1.0
            {
                menu_bar_heights.insert(display_id, bounds.height);
                break;
            }
        }
    }

    menu_bar_heights
}
