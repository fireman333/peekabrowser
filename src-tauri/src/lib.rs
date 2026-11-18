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
pub mod prompts;
pub mod records;
pub mod screenshot;
pub mod selection;
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
    init_logging();

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
    // Checked before the stores below create their files on first run.
    let returning_user = app_data_dir.join("destinations.json").exists()
        || app_data_dir.join("settings.json").exists();

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
        .manage(prompts::PromptStore::new(app_data_dir.clone()))
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

            // Top chrome above the page (address bar, navigation, tab row) and
            // its native menus (⋯, tab right-click).
            panel::create_chrome_panel(&handle)?;
            panel::create_grip_panel(&handle)?;
            handle.on_menu_event(|app, event| commands::handle_chrome_menu(app, event.id().as_ref()));

            // Create the floating destination picker popup
            panel::create_picker_panel(&handle)?;
            panel::lookup_highlight::create(&handle)?; // three-finger lookup highlight
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

            // three-finger lookup (opt-in; starts only if three_finger_lookup is on)
            hotkeys::three_finger::sync(&handle);

            // Debug builds only: PB_DEBUG_SHOW=1 shows the panel with the first
            // destination at launch (for screenshots / UI checks).
            #[cfg(debug_assertions)]
            if std::env::var_os("PB_DEBUG_SHOW").is_some() {
                let h = handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(3));
                    let h2 = h.clone();
                    let _ = h.run_on_main_thread(move || {
                        panel::show_panel(&h2);
                        if let Some(d) = h2.state::<destinations::DestinationManager>().get_all().into_iter().next() {
                            let _ = lifecycle::switch_destination(&h2, &d);
                        }
                    });
                });
            }

            // Builds before v2.0.5 were ad-hoc signed, so their Accessibility / Screen
            // Recording entries no longer match. Once, clear them and ask again.
            {
                let store = app.state::<app_settings::AppSettingsStore>();
                let mut s = store.get();
                if !s.permissions_migrated {
                    s.permissions_migrated = true;
                    store.update(s);
                    let installed = std::env::current_exe()
                        .map(|exe| updater::bundle_path_for(&exe).is_ok())
                        .unwrap_or(false);
                    if returning_user && installed {
                        let id = app.config().identifier.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_secs(2));
                            permissions::regrant_accessibility(&id);
                            permissions::clear_stale_screen_capture(&id);
                        });
                    }
                }
            }

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
            commands::resize_drag,
            commands::get_picker_data,
            commands::pick_destination,
            commands::hide_picker_panel,
            commands::open_settings_url,
            commands::regrant_accessibility,
            commands::open_trackpad_settings, // settings agent
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
            // chrome/tab commands
            commands::get_chrome_state,
            commands::chrome_navigate,
            commands::refresh_chrome,
            commands::reopen_closed_tab,
            commands::close_other_tabs,
            commands::close_tabs_to_right,
            commands::cycle_tab,
            commands::copy_page_url,
            commands::open_page_in_browser,
            commands::reload_page,
            commands::show_chrome_menu,
            commands::show_tab_menu,
            commands::get_prompt_templates,
            commands::set_prompt_profession,
            commands::set_template_prompts,
            commands::restore_template_prompts,
            commands::get_destination_prompts,
            commands::set_destination_prompts,
            commands::resize_picker,
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

#[cfg(test)]
mod acl_tests {
    //! The app ACL (build.rs + permissions/app.toml) must list exactly the
    //! commands in `generate_handler!`, or a window loses a command silently.
    use std::collections::BTreeSet;

    fn handler_commands() -> BTreeSet<String> {
        let src = include_str!("lib.rs");
        let start = src.find("generate_handler![").expect("generate_handler");
        let body = &src[start..start + src[start..].find("])").expect("end of handler list")];
        body.split("commands::").skip(1).map(|s| s.split(|c: char| c == ',' || c.is_whitespace()).next().unwrap().to_string()).collect()
    }

    fn quoted_in(src: &str, from: &str, to: &str) -> BTreeSet<String> {
        let a = src.find(from).expect(from);
        let b = a + src[a..].find(to).expect(to);
        src[a..b].split('"').skip(1).step_by(2).map(str::to_string).collect()
    }

    #[test]
    fn build_manifest_matches_handler() {
        let manifest = quoted_in(include_str!("../build.rs"), "const COMMANDS", "];");
        assert_eq!(manifest, handler_commands());
    }

    #[test]
    fn local_windows_set_allows_every_command() {
        let toml = include_str!("../permissions/app.toml");
        let local = quoted_in(toml, "identifier = \"local-windows\"", "]");
        let want: BTreeSet<String> = handler_commands().iter().map(|c| format!("allow-{}", c.replace('_', "-"))).collect();
        assert_eq!(local.into_iter().filter(|p| p.starts_with("allow-")).collect::<BTreeSet<_>>(), want);
    }

    #[test]
    fn pages_only_get_the_tab_shortcuts() {
        let toml = include_str!("../permissions/app.toml");
        let page = quoted_in(toml, "identifier = \"page-shortcuts\"", "]");
        let page: BTreeSet<String> = page.into_iter().filter(|p| p.starts_with("allow-")).collect();
        let want: BTreeSet<String> =
            ["allow-close-page", "allow-new-tab-for-active", "allow-cycle-tab", "allow-reopen-closed-tab"].iter().map(|s| s.to_string()).collect();
        assert_eq!(page, want);
        let caps = include_str!("../capabilities/pages.json");
        assert!(!caps.contains("core:") && !caps.contains("local-windows"), "pages must not get core/plugin/app-wide permissions");
    }
}

/// Log to ~/Library/Logs/Peekabrowser/peekabrowser.log (info by default,
/// RUST_LOG overrides) so problems in the installed app can be diagnosed.
/// Falls back to stderr if the file can't be opened.
fn init_logging() {
    let mut b = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("peekabrowser_lib=info,peekabrowser=info"));
    if let Some(home) = std::env::var_os("HOME") {
        let dir = std::path::PathBuf::from(home).join("Library/Logs/Peekabrowser");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("peekabrowser.log");
        // Start fresh when the log grows past 5 MB.
        if std::fs::metadata(&path).map(|m| m.len() > 5 * 1024 * 1024).unwrap_or(false) {
            let _ = std::fs::rename(&path, dir.join("peekabrowser.old.log"));
        }
        if let Ok(f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            b.target(env_logger::Target::Pipe(Box::new(f)));
        }
    }
    b.format_timestamp_millis();
    let _ = b.try_init();
}
