use std::collections::HashSet;

use core_foundation::array::CFArray;
use core_foundation::base::TCFType;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGSMainConnectionID() -> u32;
    fn CGSCopySpacesForWindows(
        cid: u32,
        mask: u32,
        window_ids: *const core_foundation::array::__CFArray,
    ) -> *const core_foundation::array::__CFArray;
    fn CGSCopyManagedDisplaySpaces(
        cid: u32,
    ) -> *const core_foundation::array::__CFArray;
}

const CGS_ALL_SPACES_MASK: u32 = 7;

fn get_current_space_ids(cid: u32) -> Option<HashSet<u64>> {
    unsafe {
        let ptr = CGSCopyManagedDisplaySpaces(cid);
        if ptr.is_null() {
            return None;
        }
        let displays: CFArray = CFArray::wrap_under_create_rule(ptr);
        let key = CFString::new("Current Space");
        let id_key = CFString::new("ManagedSpaceID");
        let mut ids = HashSet::new();

        for i in 0..displays.len() {
            let dict_ptr = *displays.get_unchecked(i);
            let dict: CFDictionary =
                CFDictionary::wrap_under_get_rule(dict_ptr as *const _);
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
    unsafe {
        let cid = CGSMainConnectionID();
        if cid == 0 {
            return None;
        }

        let current_spaces = get_current_space_ids(cid)?;
        if current_spaces.is_empty() {
            return None;
        }

        let wid = CFNumber::from(window_id as i64);
        let window_ids = CFArray::from_CFTypes(&[wid.as_CFType()]);

        let spaces_ptr =
            CGSCopySpacesForWindows(cid, CGS_ALL_SPACES_MASK, window_ids.as_concrete_TypeRef());

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
