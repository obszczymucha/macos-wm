use crate::display::{focus_display, move_window_to_display, Target};
use rdev::{Event, EventType, Key};
use std::sync::atomic::{AtomicBool, Ordering};

static CMD_DOWN: AtomicBool = AtomicBool::new(false);
static SHIFT_DOWN: AtomicBool = AtomicBool::new(false);

pub fn callback(event: Event) -> Option<Event> {
    match event.event_type {
        EventType::KeyPress(Key::MetaLeft) => {
            CMD_DOWN.store(true, Ordering::SeqCst);
        }
        EventType::KeyRelease(Key::MetaLeft) => {
            CMD_DOWN.store(false, Ordering::SeqCst);
        }
        EventType::KeyPress(Key::ShiftLeft) => {
            SHIFT_DOWN.store(true, Ordering::SeqCst);
        }
        EventType::KeyRelease(Key::ShiftLeft) => {
            SHIFT_DOWN.store(false, Ordering::SeqCst);
        }
        EventType::KeyPress(Key::KeyK) if CMD_DOWN.load(Ordering::SeqCst) => {
            if SHIFT_DOWN.load(Ordering::SeqCst) {
                crate::debug!("Cmd+Shift+K");
                move_window_to_display(Target::Top);
            } else {
                crate::debug!("Cmd+K");
                focus_display(Target::Top);
            }
            return None; // Don't propagate
        }
        EventType::KeyPress(Key::KeyJ) if CMD_DOWN.load(Ordering::SeqCst) => {
            if SHIFT_DOWN.load(Ordering::SeqCst) {
                crate::debug!("Cmd+Shift+J");
                move_window_to_display(Target::Bottom);
            } else {
                crate::debug!("Cmd+J");
                focus_display(Target::Bottom);
            }
            return None; // Don't propagate
        }
        _ => {}
    }
    Some(event)
}
