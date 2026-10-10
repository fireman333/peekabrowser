pub mod activity;
pub mod app_settings;
pub mod commands;
pub mod delivery;
pub mod destinations;
pub mod hotkeys;
pub mod i18n;
pub mod lifecycle;
pub mod native;
pub mod panel;
pub mod permissions;
pub mod picker_keys;
pub mod records;
pub mod screenshot;
pub mod tray;
pub mod updater;
pub mod webviews;

use tauri::Manager;

use destinations::DestinationManager;
use hotkeys::shortcut_store::ShortcutStore;
use webviews::WebViewTabManager;


pub use delivery::PickerState;

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

    let records_dir = app_data_dir.clone();

    tauri::Builder::default()
        .plugin(tauri_nspanel::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        // Launch at login via a per-user LaunchAgent (works for ad-hoc signed builds).
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(DestinationManager::new(app_data_dir.clone()))
        .manage(ShortcutStore::new(app_data_dir.clone()))
        .manage(app_settings::AppSettingsStore::new(app_data_dir.clone()))
        .manage(std::sync::Mutex::new(WebViewTabManager::new()))
        .manage(PickerState(std::sync::Mutex::new(Default::default())))
        .manage(commands::SystemConfigState::new())
        .setup(move |app| {
            let handle = app.handle().clone();

            // Interface language for menu bar, notifications and window titles.
            i18n::set_language(&app.state::<app_settings::AppSettingsStore>().get().language);

            // Set as accessory app (no dock icon)
            #[cfg(target_os = "macos")]
            {
                use tauri::ActivationPolicy;
                app.set_activation_policy(ActivationPolicy::Accessory);
                // No app-lifetime App Nap opt-out: activity tokens are held only
                // while a user-requested generation runs (see activity.rs).
            }

            // Local query records (SQLite). The app keeps working without them.
            match records::RecordStore::open(&records_dir) {
                Ok(store) => {
                    app.manage(store);
                }
                Err(e) => log::warn!("records unavailable: {}", e),
            }

            // Create the sidebar NSPanel
            panel::create_sidebar_panel(&handle)?;

            // Create the floating destination picker popup
            panel::create_picker_panel(&handle)?;
            picker_keys::install(&handle);

            // Setup system tray
            tray::setup_tray(&handle)?;

            // Register global shortcuts
            if let Err(e) = hotkeys::global_shortcuts::register_shortcuts(&handle) {
                log::warn!("Failed to register shortcuts: {}", e);
            }

            // Start edge hover detector
            panel::hover_detector::start_hover_detector(handle.clone());

            // Launch at login: the LaunchAgent stores the executable path, so rewrite it
            // after the app was moved/updated (skipped when running from a DMG or translocated).
            {
                use tauri_plugin_autostart::ManagerExt;
                let launcher = handle.autolaunch();
                let installed = std::env::current_exe()
                    .map(|exe| updater::bundle_path_for(&exe).is_ok())
                    .unwrap_or(false);
                if installed && launcher.is_enabled().unwrap_or(false) {
                    if let Err(e) = launcher.enable() {
                        log::warn!("autostart: refresh failed: {}", e);
                    }
                }
            }

            // Update checks: once shortly after launch, then daily (if enabled)
            updater::start_background_checks(handle.clone());

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
            commands::open_accessibility_settings,
            commands::open_settings_window,
            commands::get_pages,
            commands::switch_page,
            commands::close_page,
            commands::take_screenshot,
            commands::reload_active_page,
            commands::go_back,
            commands::go_forward,
            commands::open_active_in_browser,
            commands::get_shortcuts,
            commands::save_shortcuts,
            commands::get_system_config_data,
            commands::create_system_item,
            commands::close_system_config,
            commands::run_ocr,
            commands::save_answer,
            commands::list_records,
            commands::update_record,
            commands::delete_record,
            commands::get_record_attachment,
            commands::copy_record_markdown,
            commands::export_record_markdown,
            commands::open_record,
            commands::open_records_window,
            commands::get_app_settings,
            commands::save_app_settings,
            commands::get_autostart,
            commands::set_autostart,
            commands::get_diagnostics,
            commands::get_material_kind,
            commands::get_app_version,
            commands::get_update_status,
            commands::check_for_updates,
            commands::install_update,
            commands::open_release_page,
        ])
        // Prevent page panel window close from exiting the app.
        // Settings window is allowed to close normally.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let label = window.label();
                // Allow settings/system-config windows and intentionally-closing pages
                if label != "settings-window" && label != "system-config" && label != "records-window"
                    && !panel::is_page_closing(label)
                {
                    api.prevent_close();
                    // A page viewer requested close (e.g. Cmd+W): close that page.
                    if label.starts_with("page-") {
                        let app = window.app_handle().clone();
                        let label_owned = label.to_string();
                        let app2 = app.clone();
                        let _ = app.run_on_main_thread(move || {
                            lifecycle::close_page_by_label(&app2, &label_owned);
                        });
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
