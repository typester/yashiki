use core_foundation::array::CFArray;
use core_foundation::base::TCFType;
use core_foundation::number::CFNumber;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGSMainConnectionID() -> u32;
    fn CGSCopySpacesForWindows(
        cid: u32,
        mask: u32,
        window_ids: *const core_foundation::array::__CFArray,
    ) -> *const core_foundation::array::__CFArray;
}

const CGS_ALL_SPACES_MASK: u32 = 7;

/// Check whether a window exists on any macOS Space (including fullscreen Spaces
/// that Accessibility API cannot reach). Uses the CGS private API; returns `None`
/// if the call fails so the caller can fall back to existing heuristics.
pub fn window_exists_on_any_space(window_id: u32) -> Option<bool> {
    unsafe {
        let cid = CGSMainConnectionID();
        if cid == 0 {
            return None;
        }

        let wid = CFNumber::from(window_id as i64);
        let window_ids =
            CFArray::from_CFTypes(&[wid.as_CFType()]);

        let spaces_ptr =
            CGSCopySpacesForWindows(cid, CGS_ALL_SPACES_MASK, window_ids.as_concrete_TypeRef());

        if spaces_ptr.is_null() {
            return None;
        }

        let spaces: CFArray = CFArray::wrap_under_create_rule(spaces_ptr);
        Some(!spaces.is_empty())
    }
}
