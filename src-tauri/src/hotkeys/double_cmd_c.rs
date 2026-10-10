//! ⌘C ⌘C detection.
//!
//! A double copy fires only when both of these hold:
//! - two real ⌘C key presses (no other modifiers, no key repeat) within
//!   `DOUBLE_TAP_WINDOW_MS`, seen by a global key monitor, and
//! - two text clipboard changes in the same window.
//!
//! The key monitor needs Accessibility. Without it the feature stays off:
//! clipboard timing alone also matches copy buttons, menu copies, apps that
//! write the pasteboard twice per copy, and Universal Clipboard items arriving
//! from another Mac. Accessibility is re-checked while missing, so granting it
//! (or re-granting it after an update invalidated the old grant) takes effect
//! without a relaunch.
//!
//! macOS has no public cross-app pasteboard-change notification, so the
//! pasteboard `changeCount` is still sampled: slowly while idle (baseline
//! refresh), fast for a short window after each ⌘C key press.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

const DOUBLE_TAP_WINDOW_MS: u64 = 500;
const FAST: Duration = Duration::from_millis(30);
const BURST: Duration = Duration::from_millis(1200);
/// Idle interval between ⌘C presses (baseline refresh and Accessibility re-check).
const IDLE: Duration = Duration::from_millis(2000);

/// Set by the key monitor; wakes the sampler into a fast window.
static WAKE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

/// Whether the global ⌘C key monitor is installed (⌘C ⌘C is live).
static KEYS_ACTIVE: AtomicBool = AtomicBool::new(false);
/// An install is queued on the main thread.
static INSTALLING: AtomicBool = AtomicBool::new(false);

/// Timestamps (ms) of the last two ⌘C key presses: (previous, latest).
static TAPS: Mutex<(u64, u64)> = Mutex::new((0, 0));

/// Whether ⌘C ⌘C is currently able to fire (key monitor installed).
pub fn is_active() -> bool {
    KEYS_ACTIVE.load(Ordering::Relaxed)
}

pub fn start_double_cmd_c_detector(app: AppHandle) {
    ensure_key_monitor(&app);
    std::thread::spawn(move || monitor_pasteboard(app));
}

/// Install the key monitor on the main thread once Accessibility is granted.
fn ensure_key_monitor(app: &AppHandle) {
    if is_active() || !crate::native::accessibility_trusted() {
        return;
    }
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return;
    }
    let queued = app.run_on_main_thread(|| {
        let ok = install_key_monitor();
        KEYS_ACTIVE.store(ok, Ordering::SeqCst);
        INSTALLING.store(false, Ordering::SeqCst);
        log::info!("double-copy: key monitor {}", if ok { "installed" } else { "failed" });
    });
    if queued.is_err() {
        INSTALLING.store(false, Ordering::SeqCst);
    }
}

fn record_tap() {
    if let Ok(mut t) = TAPS.lock() {
        *t = (t.1, current_timestamp_ms());
    }
}

/// True if two ⌘C presses landed within the window and the latest one is
/// recent enough to have caused the clipboard change seen at `now`.
fn recent_double_tap(now: u64) -> bool {
    let Ok(t) = TAPS.lock() else { return false };
    let (prev, last) = *t;
    prev > 0
        && last.saturating_sub(prev) < DOUBLE_TAP_WINDOW_MS
        && now.saturating_sub(last) < BURST.as_millis() as u64
}

fn clear_taps() {
    if let Ok(mut t) = TAPS.lock() {
        *t = (0, 0);
    }
}

fn wake() {
    let (lock, cv) = &WAKE;
    if let Ok(mut w) = lock.lock() {
        *w = true;
        cv.notify_one();
    }
}

/// Sleep for `d`, returning early (true) if woken by a ⌘C key event.
fn sleep_or_wake(d: Duration) -> bool {
    let (lock, cv) = &WAKE;
    let Ok(guard) = lock.lock() else {
        std::thread::sleep(d);
        return false;
    };
    let (mut guard, _) = cv.wait_timeout_while(guard, d, |w| !*w).unwrap_or_else(|e| e.into_inner());
    let woke = *guard;
    *guard = false;
    woke
}

/// How long a double-copy-shaped clipboard change may wait for its second ⌘C
/// key event (the key monitor can be delivered after the pasteboard write).
const PENDING_MS: u64 = 300;

fn monitor_pasteboard(app: AppHandle) {
    let mut last_count: i64 = get_pasteboard_change_count();
    let mut last_change_time: u64 = 0;
    let mut last_had_text = true;
    let mut fast_until = Instant::now();
    // Time a double-copy-shaped change was seen before its second key press.
    let mut pending: Option<u64> = None;

    loop {
        let interval = if Instant::now() < fast_until { FAST } else { IDLE };
        if sleep_or_wake(interval) {
            fast_until = Instant::now() + BURST;
        }
        if interval == IDLE {
            ensure_key_monitor(&app);
        }

        let current_count = get_pasteboard_change_count();
        let now = current_timestamp_ms();
        if current_count == last_count {
            if let Some(seen) = pending {
                if now.saturating_sub(seen) > PENDING_MS {
                    pending = None;
                } else if recent_double_tap(now) {
                    pending = None;
                    if fire(&app) {
                        last_change_time = 0;
                    }
                }
            }
            continue;
        }

        let time_diff = now.saturating_sub(last_change_time);
        let jump = (current_count - last_count).unsigned_abs();
        // Universal Clipboard items from another Mac never count.
        let has_text = pasteboard_has_text() && !pasteboard_is_remote();
        last_count = current_count;
        pending = None;

        let shaped = is_active()
            && has_text
            && ((jump == 1 && time_diff < DOUBLE_TAP_WINDOW_MS && last_change_time > 0 && last_had_text)
                || jump >= 2);
        last_had_text = has_text;
        last_change_time = if has_text { now } else { 0 };

        if shaped {
            if recent_double_tap(now) {
                if fire(&app) {
                    last_change_time = 0;
                }
            } else {
                pending = Some(now);
            }
        }
    }
}

/// Show the picker (or auto-send) with the clipboard text. Returns true if it
/// fired; the tap chain is reset so a third copy doesn't re-trigger.
fn fire(app: &AppHandle) -> bool {
    let text = get_clipboard_text();
    if text.is_empty() {
        return false;
    }
    clear_taps();
    log::info!("double-copy detected ({} chars)", text.chars().count());
    let source_app = crate::native::frontmost_app_name();
    if let Some(state) = app.try_state::<crate::delivery::PickerState>() {
        if let Ok(mut s) = state.0.lock() {
            s.payload = Some(crate::delivery::Payload::Text { text });
            s.source_app = source_app;
        }
    }
    let auto_first = app
        .try_state::<crate::app_settings::AppSettingsStore>()
        .map(|s| s.get().auto_send_first)
        .unwrap_or(false);
    let app2 = app.clone();
    if auto_first {
        let _ = app.run_on_main_thread(move || {
            if let Err(e) = crate::commands::pick_destination_by_index(&app2, 0) {
                log::warn!("auto-send to first destination failed: {}", e);
            }
        });
    } else {
        let (cx, cy) = crate::panel::get_cursor_topleft_pos();
        let _ = app.run_on_main_thread(move || crate::panel::show_picker(&app2, cx, cy));
    }
    true
}

/// Observe plain ⌘C globally (requires Accessibility; never consumes the event).
/// Must run on the main thread.
#[cfg(target_os = "macos")]
fn install_key_monitor() -> bool {
    use block2::RcBlock;
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    const KEY_DOWN_MASK: u64 = 1 << 10;
    const SHIFT_FLAG: usize = 1 << 17;
    const CONTROL_FLAG: usize = 1 << 18;
    const OPTION_FLAG: usize = 1 << 19;
    const COMMAND_FLAG: usize = 1 << 20;
    const MODIFIERS: usize = SHIFT_FLAG | CONTROL_FLAG | OPTION_FLAG | COMMAND_FLAG;
    const KEYCODE_C: u16 = 8;
    unsafe {
        let Some(cls) = AnyClass::get(c"NSEvent") else { return false };
        let block = RcBlock::new(|ev: *mut AnyObject| {
            if ev.is_null() {
                return;
            }
            let flags: usize = msg_send![ev, modifierFlags];
            let code: u16 = msg_send![ev, keyCode];
            if code != KEYCODE_C || flags & MODIFIERS != COMMAND_FLAG {
                return; // not plain ⌘C (⌘⇧C, ⌘⌥C … are other shortcuts)
            }
            let repeat: bool = msg_send![ev, isARepeat];
            if repeat {
                return; // holding ⌘C must not count as a double tap
            }
            record_tap();
            wake();
        });
        let m: *mut AnyObject = msg_send![cls, addGlobalMonitorForEventsMatchingMask: KEY_DOWN_MASK, handler: &*block];
        if m.is_null() {
            return false;
        }
        let _: *mut AnyObject = msg_send![m, retain]; // lives for the app's lifetime
        true
    }
}

#[cfg(not(target_os = "macos"))]
fn install_key_monitor() -> bool {
    false
}

/// Universal Clipboard (Handoff) marks items copied on another device.
fn pasteboard_is_remote() -> bool {
    #[cfg(target_os = "macos")]
    unsafe {
        use objc::runtime::Object;
        use objc::{msg_send, sel, sel_impl};
        let cls = objc::runtime::Class::get("NSPasteboard").unwrap();
        let pb: *mut Object = msg_send![cls, generalPasteboard];
        let ns_string_cls = objc::runtime::Class::get("NSString").unwrap();
        let s = b"com.apple.is-remote-clipboard\0";
        let remote_type: *mut Object = msg_send![ns_string_cls,
            stringWithUTF8String: s.as_ptr() as *const std::os::raw::c_char];
        let arr_cls = objc::runtime::Class::get("NSArray").unwrap();
        let types_arr: *mut Object = msg_send![arr_cls, arrayWithObject: remote_type];
        let available: *mut Object = msg_send![pb, availableTypeFromArray: types_arr];
        !available.is_null()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Check if the pasteboard contains text content (avoids crash on file/image-only clipboard)
fn pasteboard_has_text() -> bool {
    #[cfg(target_os = "macos")]
    unsafe {
        use objc::runtime::Object;
        use objc::{msg_send, sel, sel_impl};
        let cls = objc::runtime::Class::get("NSPasteboard").unwrap();
        let pb: *mut Object = msg_send![cls, generalPasteboard];
        let utf8_type = {
            let ns_string_cls = objc::runtime::Class::get("NSString").unwrap();
            let s = b"public.utf8-plain-text\0";
            let raw: *mut Object = msg_send![ns_string_cls,
                stringWithUTF8String: s.as_ptr() as *const std::os::raw::c_char];
            raw
        };
        let arr_cls = objc::runtime::Class::get("NSArray").unwrap();
        let types_arr: *mut Object = msg_send![arr_cls, arrayWithObject: utf8_type];
        let available: *mut Object = msg_send![pb, availableTypeFromArray: types_arr];
        !available.is_null()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// NSPasteboard changeCount increments on every write, even if content is identical.
fn get_pasteboard_change_count() -> i64 {
    #[cfg(target_os = "macos")]
    unsafe {
        use objc::runtime::Object;
        use objc::{msg_send, sel, sel_impl};
        let cls = objc::runtime::Class::get("NSPasteboard").unwrap();
        let pb: *mut Object = msg_send![cls, generalPasteboard];
        let count: i64 = msg_send![pb, changeCount];
        count
    }
    #[cfg(not(target_os = "macos"))]
    {
        0
    }
}

fn get_clipboard_text() -> String {
    #[cfg(target_os = "macos")]
    unsafe {
        use objc::runtime::Object;
        use objc::{msg_send, sel, sel_impl};
        let cls = objc::runtime::Class::get("NSPasteboard").unwrap();
        let pb: *mut Object = msg_send![cls, generalPasteboard];
        let utf8_type = {
            let ns_string_cls = objc::runtime::Class::get("NSString").unwrap();
            let s = b"public.utf8-plain-text\0";
            let raw: *mut Object = msg_send![ns_string_cls,
                stringWithUTF8String: s.as_ptr() as *const std::os::raw::c_char];
            raw
        };
        let ns_string: *mut Object = msg_send![pb, stringForType: utf8_type];
        if ns_string.is_null() {
            return String::new();
        }
        let cstr: *const std::os::raw::c_char = msg_send![ns_string, UTF8String];
        if cstr.is_null() {
            return String::new();
        }
        std::ffi::CStr::from_ptr(cstr).to_string_lossy().into_owned()
    }
    #[cfg(not(target_os = "macos"))]
    {
        String::new()
    }
}

fn current_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
