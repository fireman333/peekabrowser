use tauri::{AppHandle, Manager};

const DOUBLE_TAP_WINDOW_MS: u64 = 500;
const POLL_MS: u64 = 50;

pub fn start_double_cmd_c_detector(app: AppHandle) {
    std::thread::spawn(move || {
        monitor_pasteboard(app);
    });
}

fn monitor_pasteboard(app: AppHandle) {
    let mut last_count: i64 = get_pasteboard_change_count();
    let mut last_change_time: u64 = 0;

    loop {
        std::thread::sleep(std::time::Duration::from_millis(POLL_MS));

        let current_count = get_pasteboard_change_count();

        if current_count != last_count {
            let now = current_timestamp_ms();
            let time_diff = now.saturating_sub(last_change_time);

            if time_diff < DOUBLE_TAP_WINDOW_MS && last_change_time > 0 {
                let text = get_clipboard_text();
                if !text.is_empty() {
                    log::info!("Double-copy detected ({} chars)", text.len());

                    // Store text in shared picker state
                    if let Some(state) = app.try_state::<crate::PickerState>() {
                        *state.0.lock().unwrap() = text;
                    }

                    // Get cursor pos on background thread (NSEvent mouseLocation is thread-safe)
                    let (cx, cy) = crate::panel::get_cursor_topleft_pos();

                    // Show picker on main thread (window ops require main thread)
                    let app2 = app.clone();
                    let _ = app.run_on_main_thread(move || {
                        crate::panel::show_picker(&app2, cx, cy);
                    });
                }
            }

            last_change_time = now;
            last_count = current_count;
        }
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
