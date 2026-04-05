use tauri::{AppHandle, Manager};
use std::io::Write;

const DOUBLE_TAP_WINDOW_MS: u64 = 500;
const POLL_MS: u64 = 30;

fn peeka_log(msg: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/peekabrowser.log")
    {
        let ts = current_timestamp_ms();
        let _ = writeln!(f, "[{}] {}", ts, msg);
    }
}

pub fn start_double_cmd_c_detector(app: AppHandle) {
    std::thread::spawn(move || {
        monitor_pasteboard(app);
    });
}

fn monitor_pasteboard(app: AppHandle) {
    let mut last_count: i64 = get_pasteboard_change_count();
    let mut last_change_time: u64 = 0;
    // Track if last clipboard content was text (to avoid re-triggering on non-text)
    let mut last_had_text = true;

    loop {
        std::thread::sleep(std::time::Duration::from_millis(POLL_MS));

        let current_count = get_pasteboard_change_count();

        if current_count != last_count {
            let now = current_timestamp_ms();
            let time_diff = now.saturating_sub(last_change_time);
            let jump = (current_count - last_count).unsigned_abs();

            // Check if clipboard has text FIRST (safe even for file/image clipboard)
            let has_text = pasteboard_has_text();

            peeka_log(&format!(
                "CB change: count {}→{} jump={} has_text={} time_diff={}ms last_had_text={}",
                last_count, current_count, jump, has_text, time_diff, last_had_text
            ));

            // Double-copy detected in two ways:
            // 1. Two separate changes within DOUBLE_TAP_WINDOW_MS (normal case)
            // 2. changeCount jumped ≥ 2 in a single poll cycle (both copies
            //    happened within one 30ms interval)
            // Only count text clipboard changes for timing (ignore file copies)
            let is_double = has_text && (
                (jump == 1 && time_diff < DOUBLE_TAP_WINDOW_MS && last_change_time > 0 && last_had_text)
                || jump >= 2
            );

            if is_double {
                let text = get_clipboard_text();
                if !text.is_empty() {
                    peeka_log(&format!("Double-copy detected ({} chars, jump={})", text.len(), jump));

                    // Store text in shared picker state
                    if let Some(state) = app.try_state::<crate::PickerState>() {
                        *state.0.lock().unwrap() = text;
                    }

                    // Get cursor pos on background thread (NSEvent mouseLocation is thread-safe)
                    let (cx, cy) = crate::panel::get_cursor_topleft_pos();
                    peeka_log(&format!("Showing picker at ({}, {})", cx, cy));

                    // Show picker on main thread (window ops require main thread)
                    let app2 = app.clone();
                    match app.run_on_main_thread(move || {
                        peeka_log("show_picker main thread callback fired");
                        crate::panel::show_picker(&app2, cx, cy);
                        peeka_log("show_picker done");
                    }) {
                        Ok(_) => peeka_log("run_on_main_thread dispatched OK"),
                        Err(e) => peeka_log(&format!("run_on_main_thread FAILED: {:?}", e)),
                    }
                } else {
                    peeka_log("Double-copy detected but clipboard text is empty");
                }
            }

            // Only update timing for text changes (non-text copies reset the chain)
            if has_text {
                last_change_time = now;
            } else {
                last_change_time = 0; // Reset so next text copy starts fresh
            }
            last_had_text = has_text;
            last_count = current_count;
        }
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
        // Create an NSArray with just the one type to check
        let arr_cls = objc::runtime::Class::get("NSArray").unwrap();
        let types_arr: *mut Object = msg_send![arr_cls, arrayWithObject: utf8_type];
        // availableTypeFromArray: returns nil if none of the types are available
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
        std::ffi::CStr::from_ptr(cstr)
            .to_string_lossy()
            .into_owned()
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
