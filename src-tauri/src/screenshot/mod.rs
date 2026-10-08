//! Interactive screen capture (shared by the shortcut and the sidebar command).

use tauri::{AppHandle, Manager};

/// Where the latest capture lives (per-user temp dir, not world-readable /tmp).
pub fn capture_path() -> std::path::PathBuf {
    std::env::temp_dir().join("peekabrowser_screenshot.png")
}

/// Hide the panel, run `screencapture -i`, then show the picker with an Image payload.
pub fn capture_to_picker(app: &AppHandle) {
    let source_app = crate::native::frontmost_app_name();
    crate::panel::hide_panel(app);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        let path = capture_path();
        let _ = std::fs::remove_file(&path);

        let status = std::process::Command::new("/usr/sbin/screencapture")
            .args(["-i", "-x"])
            .arg(&path)
            .status();
        log::info!("screencapture status: {:?}", status);

        let (cx, cy) = crate::panel::get_cursor_topleft_pos();
        crate::hotkeys::global_shortcuts::activate_app();
        let app2 = app.clone();
        match status {
            Ok(s) if s.success() && path.exists() => {
                if let Some(state) = app.try_state::<crate::delivery::PickerState>() {
                    if let Ok(mut st) = state.0.lock() {
                        st.payload = Some(crate::delivery::Payload::Image { path: path.clone() });
                        st.source_app = source_app;
                    }
                }
                let _ = app.run_on_main_thread(move || crate::panel::show_picker(&app2, cx, cy));
            }
            Ok(_) => {
                log::info!("Screenshot cancelled");
                let _ = app.run_on_main_thread(move || crate::panel::show_panel(&app2));
            }
            Err(_) => {
                log::warn!("screencapture failed or permission denied");
                crate::permissions::open_screen_recording_settings();
                let _ = app.run_on_main_thread(move || crate::panel::show_panel(&app2));
            }
        }
    });
}
