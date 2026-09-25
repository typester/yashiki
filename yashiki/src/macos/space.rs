use std::collections::HashSet;
use std::sync::OnceLock;

use core_foundation::array::{__CFArray, CFArray};
use core_foundation::base::TCFType;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;

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

fn get_current_space_ids(api: &CgsApi, cid: u32) -> Option<HashSet<u64>> {
    unsafe {
        let ptr = (api.copy_managed_display_spaces)(cid);
        if ptr.is_null() {
            return None;
        }
        let displays: CFArray = CFArray::wrap_under_create_rule(ptr);
        let key = CFString::new("Current Space");
        let id_key = CFString::new("ManagedSpaceID");
        let mut ids = HashSet::new();

        for i in 0..displays.len() {
            let dict_ptr = *displays.get_unchecked(i);
            let dict: CFDictionary = CFDictionary::wrap_under_get_rule(dict_ptr as *const _);
            let Some(space_ptr) = dict.find(key.as_concrete_TypeRef() as *const _) else {
                continue;
            };
            let space_dict: CFDictionary =
                CFDictionary::wrap_under_get_rule(*space_ptr as *const _);
            let Some(id_ptr) = space_dict.find(id_key.as_concrete_TypeRef() as *const _) else {
                continue;
            };
            let id_num: CFNumber = CFNumber::wrap_under_get_rule(*id_ptr as *const _);
            if let Some(id) = id_num.to_i64() {
                ids.insert(id as u64);
            }
        }
        Some(ids)
    }
}

/// Check whether a window is on a macOS Space other than any display's current
/// Space. Returns `None` if the CGS private API is unavailable.
pub fn window_is_on_other_space(window_id: u32) -> Option<bool> {
    let api = get_cgs_api()?;

    unsafe {
        let cid = (api.main_connection_id)();
        if cid == 0 {
            return None;
        }

        let current_spaces = get_current_space_ids(api, cid)?;
        if current_spaces.is_empty() {
            return None;
        }

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
            return Some(false);
        }

        let window_spaces: HashSet<u64> = (0..spaces.len())
            .filter_map(|i| {
                let ptr = *spaces.get_unchecked(i);
                let num: CFNumber = CFNumber::wrap_under_get_rule(ptr as *const _);
                num.to_i64().map(|v| v as u64)
            })
            .collect();

        Some(window_spaces.is_disjoint(&current_spaces))
    }
}
