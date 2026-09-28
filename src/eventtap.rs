//! Our own Core Graphics event tap.
//!
//! We drive the `CGEventTap` directly instead of relying on a crate like `rdev`
//! for one crucial reason: macOS *disables* an event tap whose callback takes
//! too long to return, and it does so silently. When that happens the only way
//! to recover - short of restarting the process - is to notice the
//! `kCGEventTapDisabledBy*` notifications macOS delivers an call
//! `CGEventTapEnable` again. `rdev::grab` neither exposes the tap handle nor
//! handles those notifications, so once the tap was disabled the hotkeys stayed
//! dead until a restart. See `tap_callback` for the recovery logic.
//!
//! We also keep the tap callback itself trivial: it only reads the keycode and
//! modifier flags and hands any matched hotkey to a worker thread. All the slow
//! window work (Accessibility round-trips to other apps, the resize `sleep`)
//! runs off the tap thread, so the callback returns in microseconds and macOS
//! has no reason to time the tap out in the first place.

use crate::hotkeys::{self, Action};
use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;

// --- Minimal Core Graphics / Core Foundation FFI ---

type CFMachPortRef = *const c_void;
type CFRunLoopSourceRef = *const c_void;
type CFRunLoopRef = *const c_void;
type CFRunLoopMode = *const c_void;
type CFAllocatorRef = *const c_void;
type CGEventRef = *const c_void;
type CGEventTapProxy = *const c_void;

type TapCallback = unsafe extern "C" fn(
    proxy: CGEventTapProxy,
    etype: u32,
    event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef;

// CGEventType values (the type is repr(u32) in Core Graphics). The two
// "tap disabled" values are delivered to the callback regardless of the event
// mask we register for.
const ET_KEY_DOWN: u32 = 10;
const ET_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const ET_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;
const ET_NULL: u32 = 0;

// CGEventField: the virtual keycode of a keyboard event.
const KEYBOARD_EVENT_KEYCODE: u32 = 9;

// CGEventFlags: device-independent modifier bits.
const FLAG_SHIFT: u64 = 0x0002_0000;
const FLAG_COMMAND: u64 = 0x0010_0000;

// CGEventTapLocation::HID / Placement::HeadInsert / Options::Default.
const TAP_LOCATION_HID: u32 = 0;
const TAP_PLACEMENT_HEAD_INSERT: u32 = 0;
const TAP_OPTION_DEFAULT: u32 = 0;

// clippy::duplicated_attributes fires on the two #[link]s sharing
// `kind = "framework"`, but linking two distinct frameworks this way is correct.
#[allow(non_snake_case, non_upper_case_globals, clippy::duplicated_attributes)]
#[link(name = "CoreGraphics", kind = "framework")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: TapCallback,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    fn CGEventGetFlags(event: CGEventRef) -> u64;
    fn CGEventSetType(event: CGEventRef, etype: u32);

    fn CFMachPortCreateRunLoopSource(
        allocator: CFAllocatorRef,
        port: CFMachPortRef,
        order: i64,
    ) -> CFRunLoopSourceRef;
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFRunLoopMode);
    fn CFRunLoopRun();

    static kCFRunLoopCommonModes: CFRunLoopMode;
}

/// The live tap handle, so the callback can re-enable it when macOS disables it.
static TAP: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

/// Hands matched hotkeys to the worker thread. Only the tap thread ever sends,
/// but the `Mutex` lets us keep the `Sender` in a global without an extra
/// crate for a `Sync` channel.
static ACTIONS: OnceLock<Mutex<Sender<Action>>> = OnceLock::new();

/// Never blocks: a failed send (worker gone) just drops the action. The tap
/// thread must return promptly or macOS will disable the tap.
fn dispatch(action: Action) {
    if let Some(tx) = ACTIONS.get() {
        if let Ok(tx) = tx.lock() {
            let _ = tx.send(action);
        }
    }
}

unsafe extern "C" fn tap_callback(
    _proxy: CGEventTapProxy,
    etype: u32,
    event: CGEventRef,
    _user_info: *mut c_void,
) -> CGEventRef {
    match etype {
        ET_TAP_DISABLED_BY_TIMEOUT | ET_TAP_DISABLED_BY_USER_INPUT => {
            // macOS disabled our tap. Turn it back on so the hotkeys keep
            // working without an app restart - this is the whole point of
            // running our own tap.
            let tap = TAP.load(Ordering::SeqCst);
            if !tap.is_null() {
                CGEventTapEnable(tap as CFMachPortRef, true);
            }
            crate::debug!("event tap disabled (type {:#x}); re-enabled", etype);
        }
        ET_KEY_DOWN => {
            let keycode = CGEventGetIntegerValueField(event, KEYBOARD_EVENT_KEYCODE);
            let flags = CGEventGetFlags(event);
            let cmd = flags & FLAG_COMMAND != 0;
            let shift = flags & FLAG_SHIFT != 0;

            if let Some(action) = hotkeys::resolve(keycode, cmd, shift) {
                dispatch(action);
                // Swallow the event so the keystroke doesn't also reach the
                // focused app.
                CGEventSetType(event, ET_NULL);
            }
        }
        _ => {}
    }
    event
}

/// Install the event tap and run the current thread's run loop forever.
pub fn run() {
    // Worker thread: runs the (potentially slow) window operations off the tap
    // thread. An unbounded channel means the tap callback never blocks.
    let (tx, rx) = mpsc::channel::<Action>();
    thread::spawn(move || {
        for action in rx {
            // A panic in one window operation must NOT kill the worker: the tap
            // would stay healthy and keep swallowing our hotkeys while nothing
            // ever ran again - silently reproducing the very "stops working
            // until restart" bug this whole module exists to prevent. Catch it
            // and keep serving the next action.
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match action {
                Action::Focus(target) => crate::display::focus_display(target),
                Action::MoveWindow(target) => crate::display::move_window_to_display(target),
            }));
            if outcome.is_err() {
                crate::debug!("window operation panicked; worker continuing");
            }
        }
    });
    let _ = ACTIONS.set(Mutex::new(tx));

    unsafe {
        let tap = CGEventTapCreate(
            TAP_LOCATION_HID,
            TAP_PLACEMENT_HEAD_INSERT,
            TAP_OPTION_DEFAULT,
            1u64 << ET_KEY_DOWN,
            tap_callback,
            std::ptr::null_mut(),
        );
        if tap.is_null() {
            eprintln!(
                "Could not create the event tap. Grant this app Accessibility \
                permission in System Settings > Privacy & Security > \
                Accessibility, then relaunch."
            );
            return;
        }
        let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
        if source.is_null() {
            eprintln!("Could not create a run loop source for the event tap.");
            return;
        }

        CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
        // Publish the tap handle only once we're committed to running the loop,
        // so the callback can re-enable it. The callback can't fire before
        // CFRunLooopRun, so this ordering is safe.
        TAP.store(tap as *mut c_void, Ordering::SeqCst);
        CGEventTapEnable(tap, true);
        CFRunLoopRun();
    }
}
