use core_foundation::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
use core_foundation::base::{CFRelease, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::string::CFString;
use std::ffi::c_void;

const K_AX_VALUE_CG_POINT_TYPE: u32 = 1;
const K_AX_VALUE_CG_SIZE_TYPE: u32 = 2;

#[repr(C)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
struct CGSize {
    width: f64,
    height: f64,
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> *mut c_void;
    fn AXUIElementCopyAttributeValue(
        element: *const c_void,
        attribute: *const c_void,
        value: *mut *mut c_void,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: *const c_void,
        attribute: *const c_void,
        value: *const c_void,
    ) -> i32;
    fn AXUIElementPerformAction(element: *const c_void, action: *const c_void) -> i32;
    fn AXValueCreate(the_type: u32, value_ptr: *const c_void) -> *mut c_void;
}

unsafe fn get_windows(app: *const c_void) -> Option<*mut c_void> {
    let attr = CFString::new("AXWindows");
    let mut val: *mut c_void = std::ptr::null_mut();

    if AXUIElementCopyAttributeValue(app, attr.as_concrete_TypeRef() as *const c_void, &mut val)
        == 0
        && !val.is_null()
    {
        Some(val)
    } else {
        None
    }
}

pub fn focus_window(pid: i32, window_index: usize) {
    unsafe {
        let app = AXUIElementCreateApplication(pid);
        if app.is_null() {
            return;
        }

        // Bring app to front.
        let frontmost_attr = CFString::new("AXFrontmost");
        AXUIElementSetAttributeValue(
            app,
            frontmost_attr.as_concrete_TypeRef() as *const c_void,
            CFBoolean::true_value().as_concrete_TypeRef() as *const c_void,
        );

        // Raise the specific window.
        if let Some(windows) = get_windows(app) {
            let arr = windows as CFArrayRef;
            let count = CFArrayGetCount(arr) as usize;

            if window_index < count {
                let win = CFArrayGetValueAtIndex(arr, window_index as isize);

                if !win.is_null() {
                    let raise = CFString::new("AXRaise");
                    AXUIElementPerformAction(win, raise.as_concrete_TypeRef() as *const c_void);
                }
            }

            CFRelease(windows as *const c_void);
        }

        CFRelease(app as *const c_void);
    }
}

pub fn move_and_resize_window(pid: i32, x: f64, y: f64, width: f64, height: f64) {
    unsafe {
        let app = AXUIElementCreateApplication(pid);
        if app.is_null() {
            return;
        }

        let focused_attr = CFString::new("AXFocusedWindow");
        let mut window: *mut c_void = std::ptr::null_mut();
        let err = AXUIElementCopyAttributeValue(
            app,
            focused_attr.as_concrete_TypeRef() as *const c_void,
            &mut window,
        );

        CFRelease(app as *const c_void);

        if err != 0 || window.is_null() {
            return;
        }

        let pos_attr = CFString::new("AXPosition");
        let point = CGPoint { x, y };
        let ax_pos = AXValueCreate(
            K_AX_VALUE_CG_POINT_TYPE,
            &point as *const _ as *const c_void,
        );

        if !ax_pos.is_null() {
            AXUIElementSetAttributeValue(
                window,
                pos_attr.as_concrete_TypeRef() as *const c_void,
                ax_pos,
            );
            CFRelease(ax_pos as *const c_void);
        }

        // Wait for macOS to move the window to the new display before setting size,
        // otherwise size gets clamped to the source display's dimensions.
        std::thread::sleep(std::time::Duration::from_millis(30));

        let size_attr = CFString::new("AXSize");
        let size = CGSize { width, height };
        let ax_size = AXValueCreate(K_AX_VALUE_CG_SIZE_TYPE, &size as *const _ as *const c_void);

        if !ax_size.is_null() {
            AXUIElementSetAttributeValue(
                window,
                size_attr.as_concrete_TypeRef() as *const c_void,
                ax_size,
            );
            CFRelease(ax_size as *const c_void);
        }
    }
}
