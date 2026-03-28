use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, "toggle", "Toggle Sidebar", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Peekabrowser", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&toggle, &separator, &settings, &quit])?;

    TrayIconBuilder::with_id("main-tray")
        .tooltip("Peekabrowser")
        .icon(app.default_window_icon().cloned().unwrap())
        .menu(&menu)
        .menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => {
                crate::panel::toggle_panel(app);
            }
            "settings" => {
                crate::panel::show_panel(app);
                if let Some(window) = app.get_webview_window(crate::panel::SIDEBAR_LABEL) {
                    let _ = window.emit("open-settings", ());
                }
            }
            "quit" => {
                // Use std::process::exit to bypass the ExitRequested prevention handler
                std::process::exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                crate::panel::toggle_panel(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}
