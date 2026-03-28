use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use super::shortcut_store::{parse_shortcut, ShortcutStore};

pub fn register_shortcuts(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let store = app.state::<ShortcutStore>();
    let config = store.get();

    let toggle_shortcut = parse_shortcut(&config.toggle_sidebar)
        .map(|(m, c)| Shortcut::new(m, c))
        .ok_or("Invalid toggle shortcut")?;

    let screenshot_shortcut = parse_shortcut(&config.screenshot)
        .map(|(m, c)| Shortcut::new(m, c))
        .ok_or("Invalid screenshot shortcut")?;

    let export_shortcut = parse_shortcut(&config.export)
        .map(|(m, c)| Shortcut::new(m, c))
        .ok_or("Invalid export shortcut")?;

    let app_handle = app.clone();

    app.global_shortcut().on_shortcuts(
        [toggle_shortcut, screenshot_shortcut, export_shortcut],
        move |_app, shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            if shortcut == &toggle_shortcut {
                crate::panel::toggle_panel(&app_handle);
            } else if shortcut == &screenshot_shortcut {
                do_screenshot(&app_handle);
            } else if shortcut == &export_shortcut {
                log::info!("Export shortcut triggered");
            }
        },
    )?;

    log::info!(
        "Global shortcuts registered: toggle={}, screenshot={}, export={}",
        config.toggle_sidebar,
        config.screenshot,
        config.export
    );
    Ok(())
}

/// Unregister all current shortcuts, then re-register with new config
pub fn re_register_shortcuts(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let _ = app.global_shortcut().unregister_all();
    register_shortcuts(app)
}

fn do_screenshot(app: &AppHandle) {
    crate::panel::hide_panel(app);
    let app2 = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));

        let tmp_path = "/tmp/peekabrowser_screenshot.png";
        let _ = std::fs::remove_file(tmp_path);

        let status = std::process::Command::new("/usr/sbin/screencapture")
            .args(["-i", "-x", tmp_path])
            .status();

        log::info!("screencapture (shortcut) status: {:?}", status);

        // Capture cursor position while still on the background thread
        // (NSEvent mouseLocation is thread-safe)
        let (cx, cy) = crate::panel::get_cursor_topleft_pos();

        match status {
            Ok(s) if s.success() && std::path::Path::new(tmp_path).exists() => {
                if let Ok(data) = std::fs::read(tmp_path) {
                    let b64 = base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        &data,
                    );
                    let data_url = format!("data:image/png;base64,{}", b64);
                    if let Some(state) = app2.try_state::<crate::PickerState>() {
                        *state.0.lock().unwrap() = format!("__screenshot__:{}", data_url);
                    }
                    let _ = std::fs::remove_file(tmp_path);
                    log::info!("Screenshot captured, showing picker");
                }

                // Wake up the Accessory app's event loop, then show picker on main thread
                activate_app();
                let app3 = app2.clone();
                let _ = app2.run_on_main_thread(move || {
                    crate::panel::show_picker(&app3, cx, cy);
                });
            }
            Ok(_) => {
                log::info!("Screenshot cancelled by user");
                activate_app();
                let app3 = app2.clone();
                let _ = app2.run_on_main_thread(move || {
                    crate::panel::show_panel(&app3);
                });
            }
            _ => {
                log::warn!("screencapture failed from shortcut, opening settings");
                crate::permissions::open_screen_recording_settings();
                activate_app();
                let app3 = app2.clone();
                let _ = app2.run_on_main_thread(move || {
                    crate::panel::show_panel(&app3);
                });
            }
        }
    });
}

/// Wake up the Accessory app's main thread event loop.
/// After screencapture exits, macOS may not deliver events to an Accessory app
/// until it is explicitly activated.
pub fn activate_app() {
    #[cfg(target_os = "macos")]
    unsafe {
        use objc::{msg_send, sel, sel_impl};
        let ns_app: *mut objc::runtime::Object =
            msg_send![objc::runtime::Class::get("NSApplication").unwrap(), sharedApplication];
        let _: () = msg_send![ns_app, activateIgnoringOtherApps: true];
    }
}
