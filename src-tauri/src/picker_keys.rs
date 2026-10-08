//! Picker keyboard shortcuts: C / V / B / N / M pick the first five
//! destinations, Esc dismisses.
//!
//! Handled natively with a *local* NSEvent key-down monitor (our own app's
//! events only; no Accessibility permission). The picker panel becomes key
//! window when shown, so key presses reach us while the source app stays
//! active. Matching uses hardware key codes, which don't depend on the
//! keyboard layout or input method — 注音 / Pinyin / Japanese IMEs included —
//! and matched events are consumed before the IME or the web view sees them.

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::AppHandle;

static PICKER_VISIBLE: AtomicBool = AtomicBool::new(false);

/// ANSI virtual key codes, in finger order across the bottom row.
pub const PICK_KEYS: [(u16, char); 5] = [(8, 'C'), (9, 'V'), (11, 'B'), (45, 'N'), (46, 'M')];
const KEY_ESCAPE: u16 = 53;

pub fn set_picker_visible(v: bool) {
    PICKER_VISIBLE.store(v, Ordering::SeqCst);
}

/// Index of the destination a key code selects, if any.
pub fn pick_index(key_code: u16) -> Option<usize> {
    PICK_KEYS.iter().position(|(k, _)| *k == key_code)
}

#[cfg(target_os = "macos")]
pub fn install(app: &AppHandle) {
    use block2::RcBlock;
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    const KEY_DOWN_MASK: u64 = 1 << 10;
    // Command | Control | Option: leave real shortcuts alone (Shift/Caps are fine).
    const BLOCKING_MODIFIERS: usize = (1 << 20) | (1 << 18) | (1 << 19);

    let app = app.clone();
    unsafe {
        let Some(cls) = AnyClass::get(c"NSEvent") else { return };
        let block = RcBlock::new(move |ev: *mut AnyObject| -> *mut AnyObject {
            if ev.is_null() || !PICKER_VISIBLE.load(Ordering::SeqCst) {
                return ev;
            }
            let flags: usize = msg_send![ev, modifierFlags];
            if flags & BLOCKING_MODIFIERS != 0 {
                return ev;
            }
            let code: u16 = msg_send![ev, keyCode];
            if code == KEY_ESCAPE {
                crate::panel::hide_picker(&app);
                return std::ptr::null_mut();
            }
            if let Some(idx) = pick_index(code) {
                let repeat: bool = msg_send![ev, isARepeat];
                if !repeat {
                    // We're on the main thread (local monitors run there).
                    if let Err(e) = crate::commands::pick_destination_by_index(&app, idx) {
                        log::warn!("picker key: {}", e);
                    }
                }
                return std::ptr::null_mut(); // consumed: never reaches the IME
            }
            ev
        });
        let m: *mut AnyObject = msg_send![cls, addLocalMonitorForEventsMatchingMask: KEY_DOWN_MASK, handler: &*block];
        if !m.is_null() {
            let _: *mut AnyObject = msg_send![m, retain]; // lives for the app's lifetime
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn install(_app: &AppHandle) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bottom_row_maps_to_first_five() {
        assert_eq!(pick_index(8), Some(0)); // C
        assert_eq!(pick_index(9), Some(1)); // V
        assert_eq!(pick_index(11), Some(2)); // B
        assert_eq!(pick_index(45), Some(3)); // N
        assert_eq!(pick_index(46), Some(4)); // M
        assert_eq!(pick_index(10), None); // § / non-mapped
        assert_eq!(pick_index(KEY_ESCAPE), None);
    }
}
