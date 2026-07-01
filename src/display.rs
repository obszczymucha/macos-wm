use crate::ax;
use core_foundation::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
use core_foundation::base::{CFGetTypeID, CFRelease, TCFType};
use core_foundation::dictionary::{CFDictionary, CFDictionaryGetValueIfPresent, CFDictionaryRef};
use core_foundation::number::{CFNumber, CFNumberGetTypeID, CFNumberRef};
use core_foundation::string::CFString;
use core_graphics::display::{CGDisplay, CGRect};
use core_graphics::window::{
    kCGNullWindowID, kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly,
    CGWindowListCopyWindowInfo,
};
use std::ffi::c_void;
use std::ptr;

#[derive(Clone, Copy)]
pub enum Target {
    Top,
    Bottom,
}

fn cf_str(s: &str) -> CFString {
    CFString::new(s)
}

unsafe fn dict_number_i64(d: CFDictionaryRef, key: &CFString) -> Option<i64> {
    let mut val: *const c_void = ptr::null();

    if CFDictionaryGetValueIfPresent(d, key.as_concrete_TypeRef() as *const c_void, &mut val) == 0
        || val.is_null()
    {
        return None;
    }

    if CFGetTypeID(val) != CFNumberGetTypeID() {
        return None;
    }

    CFNumber::wrap_under_get_rule(val as CFNumberRef).to_i64()
}

unsafe fn dict_bounds(d: CFDictionaryRef, key: &CFString) -> Option<CGRect> {
    let mut val: *const c_void = ptr::null();

    if CFDictionaryGetValueIfPresent(d, key.as_concrete_TypeRef() as *const c_void, &mut val) == 0
        || val.is_null()
    {
        return None;
    }

    CGRect::from_dict_representation(&CFDictionary::wrap_under_get_rule(val as CFDictionaryRef))
}

fn focus_pid_window(pid: i64, window_index: usize) {
    ax::focus_window(pid as i32, window_index);
}

fn sorted_displays() -> Option<Vec<(u32, CGRect)>> {
    let active = CGDisplay::active_displays().ok()?;

    if active.len() < 2 {
        crate::debug!("Need 2 displays!");
        return None;
    }

    let mut by_y: Vec<(u32, CGRect)> = active
        .iter()
        .map(|id| (*id, CGDisplay::new(*id).bounds()))
        .collect();

    by_y.sort_by(|a, b| a.1.origin.y.partial_cmp(&b.1.origin.y).unwrap());
    Some(by_y)
}

struct WindowInfo {
    pid: i64,
    bounds: CGRect,
}

fn frontmost_window() -> Option<WindowInfo> {
    unsafe {
        let list = CGWindowListCopyWindowInfo(
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
            kCGNullWindowID,
        );

        if list.is_null() {
            return None;
        }

        let arr = list as CFArrayRef;
        let count = CFArrayGetCount(arr);

        let k_layer = cf_str("kCGWindowLayer");
        let k_bounds = cf_str("kCGWindowBounds");
        let k_pid = cf_str("kCGWindowOwnerPID");
        let k_alpha = cf_str("kCGWindowAlpha");

        let mut result = None;

        for i in 0..count {
            let item = CFArrayGetValueAtIndex(arr, i);
            if item.is_null() {
                continue;
            }

            let d = item as CFDictionaryRef;

            if dict_number_i64(d, &k_layer).unwrap_or(0) != 0 {
                continue;
            }
            if dict_number_i64(d, &k_alpha).unwrap_or(1) == 0 {
                continue;
            }

            let bounds = match dict_bounds(d, &k_bounds) {
                Some(b) if b.size.width >= 50.0 && b.size.height >= 50.0 => b,
                _ => continue,
            };

            let pid = match dict_number_i64(d, &k_pid) {
                Some(p) => p,
                None => continue,
            };

            result = Some(WindowInfo { pid, bounds });
            break; // window list is front-to-back; first normal window = frontmost
        }

        CFRelease(list as *const c_void);
        result
    }
}

pub fn move_window_to_display(target: Target) {
    let displays = match sorted_displays() {
        Some(d) => d,
        None => return,
    };

    let win = match frontmost_window() {
        Some(w) => w,
        None => {
            crate::debug!("No frontmost window.");
            return;
        }
    };

    let cx = win.bounds.origin.x + win.bounds.size.width / 2.0;
    let cy = win.bounds.origin.y + win.bounds.size.height / 2.0;

    let src = displays
        .iter()
        .find(|(_, b)| {
            cx >= b.origin.x
                && cx < b.origin.x + b.size.width
                && cy >= b.origin.y
                && cy < b.origin.y + b.size.height
        })
        .map(|(_, b)| *b)
        .unwrap_or(displays.first().unwrap().1);

    let dst = match target {
        Target::Top => displays.first().unwrap().1,
        Target::Bottom => displays.last().unwrap().1,
    };

    if (src.origin.x - dst.origin.x).abs() < 1.0 && (src.origin.y - dst.origin.y).abs() < 1.0 {
        return; // already on target display
    }

    crate::debug!(
        "dst origin=({}, {}) size=({}, {})",
        dst.origin.x,
        dst.origin.y,
        dst.size.width,
        dst.size.height
    );
    ax::move_and_resize_window(
        win.pid as i32,
        dst.origin.x,
        dst.origin.y,
        dst.size.width,
        dst.size.height,
    );
}

pub fn focus_display(target: Target) {
    let displays = match sorted_displays() {
        Some(d) => d,
        None => return,
    };

    let tb = match target {
        Target::Top => displays.first().unwrap().1,
        Target::Bottom => displays.last().unwrap().1,
    };

    unsafe {
        let list = CGWindowListCopyWindowInfo(
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
            kCGNullWindowID,
        );

        if list.is_null() {
            return;
        }
        let arr = list as CFArrayRef;
        let count = CFArrayGetCount(arr);

        let k_layer = cf_str("kCGWindowLayer");
        let k_bounds = cf_str("kCGWindowBounds");
        let k_pid = cf_str("kCGWindowOwnerPID");

        let mut pid_count: std::collections::HashMap<i64, usize> = Default::default();

        for i in 0..count {
            let item = CFArrayGetValueAtIndex(arr, i);
            if item.is_null() {
                continue;
            }
            let d = item as CFDictionaryRef;

            if dict_number_i64(d, &k_layer).unwrap_or(0) != 0 {
                continue;
            }

            let bounds = match dict_bounds(d, &k_bounds) {
                Some(b) if b.size.width >= 50.0 && b.size.height >= 50.0 => b,
                _ => continue,
            };

            let pid = match dict_number_i64(d, &k_pid) {
                Some(p) => p,
                None => continue,
            };

            let idx = *pid_count.entry(pid).or_insert(0);
            *pid_count.get_mut(&pid).unwrap() += 1;

            let cx = bounds.origin.x + bounds.size.width / 2.0;
            let cy = bounds.origin.y + bounds.size.height / 2.0;

            if cx >= tb.origin.x
                && cx < tb.origin.x + tb.size.width
                && cy >= tb.origin.y
                && cy < tb.origin.y + tb.size.height
            {
                CFRelease(list as *const c_void);
                focus_pid_window(pid, idx);
                return;
            }
        }

        CFRelease(list as *const c_void);
    }

    crate::debug!(
        "No window found on {} display!",
        match target {
            Target::Top => "top",
            Target::Bottom => "bottom",
        }
    );
}
