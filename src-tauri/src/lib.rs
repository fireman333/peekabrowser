pub mod commands;
pub mod destinations;
pub mod export;
pub mod hotkeys;
pub mod panel;
pub mod permissions;
pub mod screenshot;
pub mod tray;
pub mod webviews;

use tauri::{Emitter, Manager};

use destinations::DestinationManager;
use hotkeys::shortcut_store::ShortcutStore;
use webviews::WebViewTabManager;

/// Holds the clipboard text captured on double Cmd+C, for the picker popup to retrieve
pub struct PickerState(pub std::sync::Mutex<String>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    // Resolve app data directory for persistent storage
    let app_data_dir = if let Some(home) = std::env::var_os("HOME") {
        std::path::PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("com.peekabrowser.app")
    } else {
        std::path::PathBuf::from(".")
    };

    tauri::Builder::default()
        .plugin(tauri_nspanel::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(DestinationManager::new(app_data_dir.clone()))
        .manage(ShortcutStore::new(app_data_dir))
        .manage(std::sync::Mutex::new(WebViewTabManager::new()))
        .manage(PickerState(std::sync::Mutex::new(String::new())))
        .manage(commands::SystemConfigState::new())
        .setup(|app| {
            let handle = app.handle().clone();

            // Set as accessory app (no dock icon)
            #[cfg(target_os = "macos")]
            {
                use tauri::ActivationPolicy;
                app.set_activation_policy(ActivationPolicy::Accessory);
            }

            // Create the sidebar NSPanel
            panel::create_sidebar_panel(&handle)?;

            // Create the floating destination picker popup
            panel::create_picker_panel(&handle)?;

            // Setup system tray
            tray::setup_tray(&handle)?;

            // Register global shortcuts
            if let Err(e) = hotkeys::global_shortcuts::register_shortcuts(&handle) {
                log::warn!("Failed to register shortcuts: {}", e);
            }

            // Start edge hover detector
            panel::hover_detector::start_hover_detector(handle.clone());

            // Start double-copy detector
            hotkeys::double_cmd_c::start_double_cmd_c_detector(handle.clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::toggle_sidebar,
            commands::toggle_pin,
            commands::is_pinned,
            commands::show_sidebar,
            commands::hide_sidebar,
            commands::open_system_app,
            commands::get_destinations,
            commands::add_destination,
            commands::update_destination,
            commands::remove_destination,
            commands::reorder_destinations,
            commands::switch_destination,
            commands::new_tab,
            commands::new_tab_for_active,
            commands::send_to_active,
            commands::get_clipboard_text,
            commands::set_viewer_width,
            commands::get_picker_data,
            commands::pick_destination,
            commands::hide_picker_panel,
            commands::open_settings_url,
            commands::open_settings_window,
            commands::get_pages,
            commands::switch_page,
            commands::close_page,
            commands::take_screenshot,
            commands::reload_active_page,
            commands::open_active_in_browser,
            commands::get_shortcuts,
            commands::save_shortcuts,
            commands::get_system_config_data,
            commands::create_system_item,
            commands::close_system_config,
            commands::run_ocr,
        ])
        // Prevent page panel window close from exiting the app.
        // Settings window is allowed to close normally.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let label = window.label();
                // Allow settings/system-config windows and intentionally-closing pages
                if label != "settings-window" && label != "system-config"
                    && !panel::is_page_closing(label)
                {
                    api.prevent_close();
                    // If a page viewer requested close (e.g. Cmd+W from page),
                    // perform cleanup via close_page logic
                    if label.starts_with("page-") {
                        let app = window.app_handle().clone();
                        let label_owned = label.to_string();
                        // Find the page_id from the label and close it
                        let tab_mgr = app.state::<std::sync::Mutex<webviews::WebViewTabManager>>();
                        let (removed_label, next_active_label) = {
                            let mut mgr = match tab_mgr.lock() {
                                Ok(m) => m,
                                Err(_) => return,
                            };
                            // Find page_id by label
                            let page_id = mgr.get_all_pages().iter()
                                .find(|p| p.label == label_owned)
                                .map(|p| p.id.clone());
                            if let Some(pid) = page_id {
                                let removed = mgr.remove_page(&pid).map(|p| p.label);
                                let next = mgr.get_active_page().map(|p| p.label.clone());
                                let pages = mgr.get_all_pages();
                                let active_id = mgr.active_page_id.clone();
                                if let Some(sidebar) = app.get_webview_window(panel::SIDEBAR_LABEL) {
                                    let _ = sidebar.emit("pages-updated", &pages);
                                    if let Some(ref aid) = active_id {
                                        let _ = sidebar.emit("active-page-changed", aid);
                                    }
                                }
                                (removed, next)
                            } else {
                                (None, None)
                            }
                        };
                        if let Some(rl) = removed_label {
                            panel::destroy_page_panel(&app, &rl);
                        }
                        if let Some(nl) = next_active_label {
                            panel::show_page_viewer(&app, &nl);
                        }
                    }
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Peekabrowser")
        .run(|_app, event| {
            // Prevent Tauri from auto-exiting when a webview window is destroyed.
            // Page panels are regularly created/destroyed — app should only exit
            // via the tray Quit menu.
            if let tauri::RunEvent::ExitRequested { api, .. } = &event {
                api.prevent_exit();
            }
        });
}
