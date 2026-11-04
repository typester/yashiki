use std::collections::HashSet;
use std::ffi::c_void;
use std::sync::OnceLock;

use core_foundation::array::{__CFArray, CFArray};
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_foundation::uuid::{CFUUIDCreateString, CFUUIDRef, CFUUID};
use core_foundation::ConcreteCFType;

use super::DisplayId;

#[link(name = "ColorSync", kind = "framework")]
extern "C" {
    fn CGDisplayCreateUUIDFromDisplayID(display: u32) -> CFUUIDRef;
}

type CGSMainConnectionIDFn = unsafe extern "C" fn() -> u32;
type CGSCopySpacesForWindowsFn =
    unsafe extern "C" fn(u32, u32, *const __CFArray) -> *const __CFArray;
type CGSCopyManagedDisplaySpacesFn = unsafe extern "C" fn(u32) -> *const __CFArray;

struct CgsApi {
    main_connection_id: CGSMainConnectionIDFn,
    copy_spaces_for_windows: CGSCopySpacesForWindowsFn,
    copy_managed_display_spaces: CGSCopyManagedDisplaySpacesFn,
}

static CGS_API: OnceLock<Option<CgsApi>> = OnceLock::new();

fn get_cgs_api() -> Option<&'static CgsApi> {
    CGS_API
        .get_or_init(|| unsafe {
            let handle = libc::dlopen(
                c"/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics".as_ptr(),
                libc::RTLD_LAZY,
            );
            if handle.is_null() {
                tracing::warn!("CGS: failed to dlopen CoreGraphics");
                return None;
            }

            let sym = |name: &std::ffi::CStr| -> *mut std::ffi::c_void {
                libc::dlsym(handle, name.as_ptr())
            };

            let main_cid = sym(c"CGSMainConnectionID");
            let copy_spaces = sym(c"CGSCopySpacesForWindows");
            let copy_display_spaces = sym(c"CGSCopyManagedDisplaySpaces");

            if main_cid.is_null() || copy_spaces.is_null() || copy_display_spaces.is_null() {
                tracing::warn!("CGS: one or more symbols not found, Space detection disabled");
                libc::dlclose(handle);
                return None;
            }

            tracing::info!("CGS private API loaded successfully");
            Some(CgsApi {
                main_connection_id: std::mem::transmute::<
                    *mut std::ffi::c_void,
                    CGSMainConnectionIDFn,
                >(main_cid),
                copy_spaces_for_windows: std::mem::transmute::<
                    *mut std::ffi::c_void,
                    CGSCopySpacesForWindowsFn,
                >(copy_spaces),
                copy_managed_display_spaces: std::mem::transmute::<
                    *mut std::ffi::c_void,
                    CGSCopyManagedDisplaySpacesFn,
                >(copy_display_spaces),
            })
        })
        .as_ref()
}

const CGS_ALL_SPACES_MASK: u32 = 7;
const CGS_SPACE_TYPE_FULLSCREEN: i64 = 4;

fn cgs_connection() -> Option<(&'static CgsApi, u32)> {
    let api = get_cgs_api()?;
    let cid = unsafe { (api.main_connection_id)() };
    (cid != 0).then_some((api, cid))
}

/// The Space a display is currently showing, from `CGSCopyManagedDisplaySpaces`.
struct DisplaySpace {
    /// Display UUID, or "Main" when displays don't have separate Spaces.
    identifier: String,
    current_space_id: u64,
    current_space_type: i64,
}

/// Look up `key` in a CF dictionary, returning the value only if it has the expected type.
fn dict_value<T: ConcreteCFType>(dict: &CFDictionary, key: &str) -> Option<T> {
    let key = CFString::new(key);
    let value = dict.find(key.as_concrete_TypeRef() as *const c_void)?;
    unsafe { CFType::wrap_under_get_rule(*value as CFTypeRef) }.downcast::<T>()
}

fn copy_display_spaces(api: &CgsApi, cid: u32) -> Option<Vec<DisplaySpace>> {
    let displays: CFArray = unsafe {
        let ptr = (api.copy_managed_display_spaces)(cid);
        if ptr.is_null() {
            return None;
        }
        CFArray::wrap_under_create_rule(ptr)
    };

    let spaces = displays
        .iter()
        .filter_map(|item| {
            let dict = unsafe { CFType::wrap_under_get_rule(*item as CFTypeRef) }
                .downcast::<CFDictionary>()?;
            let current = dict_value::<CFDictionary>(&dict, "Current Space")?;
            Some(DisplaySpace {
                identifier: dict_value::<CFString>(&dict, "Display Identifier")
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
                current_space_id: dict_value::<CFNumber>(&current, "ManagedSpaceID")?.to_i64()?
                    as u64,
                current_space_type: dict_value::<CFNumber>(&current, "type")
                    .and_then(|n| n.to_i64())
                    .unwrap_or(0),
            })
        })
        .collect();
    Some(spaces)
}

fn get_current_space_ids(api: &CgsApi, cid: u32) -> Option<HashSet<u64>> {
    let spaces = copy_display_spaces(api, cid)?;
    Some(spaces.iter().map(|s| s.current_space_id).collect())
}

fn display_uuid(display_id: DisplayId) -> Option<String> {
    unsafe {
        let uuid_ref = CGDisplayCreateUUIDFromDisplayID(display_id);
        if uuid_ref.is_null() {
            return None;
        }
        let uuid = CFUUID::wrap_under_create_rule(uuid_ref);
        let string_ref = CFUUIDCreateString(std::ptr::null(), uuid.as_concrete_TypeRef());
        if string_ref.is_null() {
            return None;
        }
        Some(CFString::wrap_under_create_rule(string_ref).to_string())
    }
}

/// Displays whose current Space is a native fullscreen Space. Empty when the CGS private API is unavailable.
pub fn fullscreen_space_displays(display_ids: &[DisplayId]) -> HashSet<DisplayId> {
    let Some(spaces) = cgs_connection().and_then(|(api, cid)| copy_display_spaces(api, cid)) else {
        return HashSet::new();
    };

    for space in spaces.iter().filter(|s| s.current_space_type != 0) {
        tracing::debug!(
            "CGS: display {} is showing Space {} of type {}",
            space.identifier,
            space.current_space_id,
            space.current_space_type
        );
    }

    let fullscreen: Vec<&DisplaySpace> = spaces
        .iter()
        .filter(|s| s.current_space_type == CGS_SPACE_TYPE_FULLSCREEN)
        .collect();
    if fullscreen.is_empty() {
        return HashSet::new();
    }
    // "Main" means all displays share one set of Spaces ("Displays have separate Spaces" is off)
    if fullscreen.iter().any(|s| s.identifier == "Main") {
        return display_ids.iter().copied().collect();
    }

    display_ids
        .iter()
        .copied()
        .filter(|&id| {
            display_uuid(id).is_some_and(|uuid| {
                fullscreen
                    .iter()
                    .any(|s| s.identifier.eq_ignore_ascii_case(&uuid))
            })
        })
        .collect()
}

/// Where a window lives relative to the Spaces the displays are currently showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpacePresence {
    /// On a Space that some display is currently showing.
    Current,
    /// Only on Spaces no display is showing (e.g. a native fullscreen Space the user left).
    Other,
    /// Not on any Space: the window is gone.
    Gone,
}

/// Find where a window lives relative to the current Spaces. Returns `None` if the CGS private API is
/// unavailable or its answer cannot be read.
pub fn window_space_presence(window_id: u32) -> Option<SpacePresence> {
    let (api, cid) = cgs_connection()?;
    let current_spaces = get_current_space_ids(api, cid)?;
    if current_spaces.is_empty() {
        return None;
    }

    unsafe {
        let wid = CFNumber::from(window_id as i64);
        let window_ids = CFArray::from_CFTypes(&[wid.as_CFType()]);

        let spaces_ptr = (api.copy_spaces_for_windows)(
            cid,
            CGS_ALL_SPACES_MASK,
            window_ids.as_concrete_TypeRef(),
        );

        if spaces_ptr.is_null() {
            return None;
        }

        let spaces: CFArray = CFArray::wrap_under_create_rule(spaces_ptr);
        if spaces.is_empty() {
            return Some(SpacePresence::Gone);
        }

        let window_spaces: HashSet<u64> = (0..spaces.len())
            .filter_map(|i| {
                let ptr = *spaces.get_unchecked(i);
                let num: CFNumber = CFNumber::wrap_under_get_rule(ptr as *const _);
                num.to_i64().map(|v| v as u64)
            })
            .collect();
        // Spaces were reported but none could be read: unknown, not "gone"
        if window_spaces.is_empty() {
            return None;
        }

        if window_spaces.is_disjoint(&current_spaces) {
            Some(SpacePresence::Other)
        } else {
            Some(SpacePresence::Current)
        }
    }
}
