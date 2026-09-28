use crate::display::Target;

/// What a matched hotkey should do. Executed on the worker thread, never on the
/// event-tap thread.
pub enum Action {
    Focus(Target),
    MoveWindow(Target),
}

// macOS virtual key codes (layout-independent, from Carbon's `Events.h`).
const KEY_J: i64 = 0x26; // 38
const KEY_K: i64 = 0x28; // 40

/// Map a key-down to an action, if it is one of our hotkeys.
///
/// Modifier state is read from the event's flags rather than tracked across
/// separate key-up/key-down events. That means it works with either the left or
/// right Cmd/Shift, and - unlike a hand-maintained flag - it can never get stuck
/// "on" if a key-release is ever missed (e.g. while the tap was briefly
/// disabled).
pub fn resolve(keycode: i64, cmd: bool, shift: bool) -> Option<Action> {
    if !cmd {
        return None;
    }

    match (keycode, shift) {
        (KEY_K, true) => {
            crate::debug!("Cmd+Shift+K");
            Some(Action::MoveWindow(Target::Top))
        }
        (KEY_K, false) => {
            crate::debug!("Cmd+K");
            Some(Action::Focus(Target::Top))
        }
        (KEY_J, true) => {
            crate::debug!("Cmd+Shift+J");
            Some(Action::MoveWindow(Target::Bottom))
        }
        (KEY_J, false) => {
            crate::debug!("Cmd+Shift+K");
            Some(Action::Focus(Target::Bottom))
        }
        _ => None,
    }
}
