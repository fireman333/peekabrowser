use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

use crate::i18n::{tr, trf};

const TRAY_ID: &str = "main-tray";

/// Menu-bar menu in the current interface language.
fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let toggle = MenuItem::with_id(app, "toggle", tr("顯示／隱藏側邊欄", "Toggle Sidebar"), true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let settings = MenuItem::with_id(app, "settings", tr("設定…", "Settings…"), true, None::<&str>)?;
    let updates = MenuItem::with_id(app, "check-updates", tr("檢查更新…", "Check for Updates…"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", tr("結束 Peekabrowser", "Quit Peekabrowser"), true, None::<&str>)?;
    Menu::with_items(app, &[&toggle, &separator, &settings, &updates, &quit])
}

/// Rebuild the menu after the interface language changes.
pub fn refresh_menu(app: &AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), build_menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;

    TrayIconBuilder::with_id(TRAY_ID)
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
            "check-updates" => {
                let _ = crate::commands::open_settings_window(app.clone());
                let app = app.clone();
                std::thread::spawn(move || {
                    let msg = match crate::updater::check(&app) {
                        Ok(i) if i.available => trf(
                            "Peekabrowser {} 已推出 — 請到設定中安裝。",
                            "Peekabrowser {} is available — see Settings to install.",
                            &[&i.latest],
                        ),
                        Ok(_) => tr("Peekabrowser 已是最新版本。", "Peekabrowser is up to date.").to_string(),
                        Err(e) => trf("檢查更新失敗：{}", "Update check failed: {}", &[&e]),
                    };
                    crate::lifecycle::notify(&app, &msg);
                });
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
