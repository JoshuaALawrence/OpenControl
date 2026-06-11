use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const INTERRUPT_MSG: &str =
    "OpenControl was stopped by the user with the physical Escape key. \
Stop your work, do not call further OpenControl tools in this turn, and send a final message \
noting that the user stopped OpenControl.";

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static LAST_INPUT_MS: AtomicI64 = AtomicI64::new(0);
static WATCHER_STARTED: AtomicBool = AtomicBool::new(false);

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Mark that synthetic input is happening now (suppresses the Escape watcher for a
/// short cooldown so an injected Escape isn't mistaken for a user interrupt).
pub fn mark_input() {
    LAST_INPUT_MS.store(now_ms(), Ordering::Relaxed);
}

pub fn is_interrupted() -> bool {
    INTERRUPTED.load(Ordering::Relaxed)
}

pub fn clear_interrupt() {
    INTERRUPTED.store(false, Ordering::Relaxed);
}

/// Start the physical-Escape watcher once (idempotent).
pub fn spawn_escape_watcher() {
    if WATCHER_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE};
        loop {
            std::thread::sleep(std::time::Duration::from_millis(40));
            if now_ms() - LAST_INPUT_MS.load(Ordering::Relaxed) < 300 {
                continue; // ignore Escape right after our own input
            }
            let down = unsafe { GetAsyncKeyState(VK_ESCAPE.0 as i32) };
            if (down as u16 & 0x8000) != 0 {
                INTERRUPTED.store(true, Ordering::Relaxed);
            }
        }
    });
}
