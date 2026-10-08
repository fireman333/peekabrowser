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
                crate::commands::save_answer_and_notify(&app_handle);
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
    crate::screenshot::capture_to_picker(app);
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
