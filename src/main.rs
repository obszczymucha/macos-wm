mod ax;
mod display;
mod hotkeys;

pub static DEBUG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

macro_rules! debug {
    ($($arg:tt)*) => {
        if crate::DEBUG.load(std::sync::atomic::Ordering::Relaxed) {
            eprintln!($($arg)*);
        }
    }
}

pub(crate) use debug;

fn main() {
    if std::env::args().any(|a| a == "--debug") {
        DEBUG.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    if let Err(e) = rdev::grab(hotkeys::callback) {
        eprintln!("Could not grab the keyboard: {:?}", e);
    }
}
