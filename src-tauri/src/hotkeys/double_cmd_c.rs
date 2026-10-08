//! ⌘C ⌘C detection without a 30 ms forever-poll.
//!
//! macOS has no public cross-app pasteboard-change notification, so the
//! pasteboard `changeCount` still has to be sampled — but only as much as needed:
//!
//! - If Peekabrowser is trusted for Accessibility, a global key monitor sees
//!   ⌘C (it observes only; the source app still receives the keystroke) and
//!   opens a short fast-sampling window. Between copies the sampler sleeps
//!   in a slow idle interval just to keep its baseline current.
//! - Without Accessibility, sampling is adaptive: slow while idle, fast for a
//!   short window after any clipboard change (so the second copy is timed
//!   precisely).
//!
//! Only text changes count; file/image copies reset the chain.

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

const DOUBLE_TAP_WINDOW_MS: u64 = 500;
const FAST: Duration = Duration::from_millis(30);
const BURST: Duration = Duration::from_millis(1200);
/// Idle interval when ⌘C key events wake us up (baseline refresh only).
const IDLE_WITH_KEYS: Duration = Duration::from_millis(2000);
/// Idle interval when we must notice the first copy by sampling.
const IDLE_SAMPLING: Duration = Duration::from_millis(200);

/// Set by the key monitor; wakes the sampler into a fast window.
static WAKE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

pub fn start_double_cmd_c_detector(app: AppHandle) {
    let keys = install_key_monitor(&app);
    log::info!(
        "double-copy: {}",
        if keys { "key-event triggered sampling" } else { "adaptive sampling (no Accessibility)" }
    );
    std::thread::spawn(move || monitor_pasteboard(app, keys));
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

fn monitor_pasteboard(app: AppHandle, keys: bool) {
    let mut last_count: i64 = get_pasteboard_change_count();
    let mut last_change_time: u64 = 0;
    let mut last_had_text = true;
    let mut fast_until = Instant::now();

    loop {
        let interval = if Instant::now() < fast_until {
            FAST
        } else if keys {
            IDLE_WITH_KEYS
        } else {
            IDLE_SAMPLING
        };
        let mut sampled_fast = interval == FAST;
        if sleep_or_wake(interval) {
            fast_until = Instant::now() + BURST;
            sampled_fast = true;
        }

        let current_count = get_pasteboard_change_count();
        if current_count == last_count {
            continue;
        }
        fast_until = Instant::now() + BURST;

        let now = current_timestamp_ms();
        let time_diff = now.saturating_sub(last_change_time);
        let jump = (current_count - last_count).unsigned_abs();
        let has_text = pasteboard_has_text();

        // Double copy: two text changes within the window, or both landed in
        // one short sampling interval (changeCount jumped by ≥ 2). A jump seen
        // after a long idle sleep could be any two writes, so it doesn't count.
        let short_interval = sampled_fast || !keys;
        let is_double = has_text
            && ((jump == 1 && time_diff < DOUBLE_TAP_WINDOW_MS && last_change_time > 0 && last_had_text)
                || (jump >= 2 && short_interval));

        if is_double {
            let text = get_clipboard_text();
            if !text.is_empty() {
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
                // Start a fresh chain so a third copy doesn't re-trigger.
                last_change_time = 0;
                last_had_text = has_text;
                last_count = current_count;
                continue;
            }
        }

        last_change_time = if has_text { now } else { 0 };
        last_had_text = has_text;
        last_count = current_count;
    }
}

/// Observe ⌘C globally (requires Accessibility; never consumes the event).
#[cfg(target_os = "macos")]
fn install_key_monitor(_app: &AppHandle) -> bool {
    if !crate::native::accessibility_trusted() {
        return false;
    }
    use block2::RcBlock;
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    const KEY_DOWN_MASK: u64 = 1 << 10;
    const COMMAND_FLAG: usize = 1 << 20;
    const KEYCODE_C: u16 = 8;
    unsafe {
        let Some(cls) = AnyClass::get(c"NSEvent") else { return false };
        let block = RcBlock::new(|ev: *mut AnyObject| {
            if ev.is_null() {
                return;
            }
            let flags: usize = msg_send![ev, modifierFlags];
            let code: u16 = msg_send![ev, keyCode];
            if code == KEYCODE_C && flags & COMMAND_FLAG != 0 {
                wake();
            }
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
fn install_key_monitor(_app: &AppHandle) -> bool {
    false
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
