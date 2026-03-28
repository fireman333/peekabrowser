use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::destinations::{Destination, DestinationManager};
use crate::webviews::{PageInfo, WebViewTabManager};

#[tauri::command]
pub fn toggle_sidebar(app: AppHandle) {
    crate::panel::toggle_panel(&app);
}

#[tauri::command]
pub fn show_sidebar(app: AppHandle) {
    crate::panel::show_panel(&app);
}

#[tauri::command]
pub fn hide_sidebar(app: AppHandle) {
    crate::panel::hide_panel(&app);
}

#[tauri::command]
pub fn get_destinations(dest_manager: State<DestinationManager>) -> Vec<Destination> {
    dest_manager.get_all()
}

#[tauri::command]
pub fn add_destination(
    app: AppHandle,
    dest_manager: State<DestinationManager>,
    name: String,
    url: String,
    icon: String,
) -> Result<Destination, String> {
    // Validate URL
    let validated_url = if !url.starts_with("http://") && !url.starts_with("https://") {
        format!("https://{}", url)
    } else {
        url
    };

    let id = uuid::Uuid::new_v4().to_string();
    let order = dest_manager.get_all().len();
    let dest = Destination {
        id,
        name,
        url: validated_url,
        icon,
        order,
    };
    dest_manager.add(dest.clone());

    // Notify sidebar to refresh destinations
    if let Some(sidebar) = app.get_webview_window(crate::panel::SIDEBAR_LABEL) {
        let _ = sidebar.emit("destinations-changed", ());
    }

    Ok(dest)
}

#[tauri::command]
pub fn update_destination(
    app: AppHandle,
    dest_manager: State<DestinationManager>,
    id: String,
    name: String,
    url: String,
    icon: String,
) -> Result<Destination, String> {
    dest_manager
        .update(&id, name, url, icon)
        .ok_or_else(|| "Destination not found".to_string())
        .map(|dest| {
            if let Some(sidebar) = app.get_webview_window(crate::panel::SIDEBAR_LABEL) {
                let _ = sidebar.emit("destinations-changed", ());
            }
            dest
        })
}

#[tauri::command]
pub fn remove_destination(
    app: AppHandle,
    dest_manager: State<DestinationManager>,
    tab_manager: State<std::sync::Mutex<WebViewTabManager>>,
    id: String,
) {
    dest_manager.remove(&id);
    // Remove all pages for this destination
    if let Ok(mut mgr) = tab_manager.lock() {
        let removed = mgr.remove_pages_for_dest(&id);
        for page in &removed {
            crate::panel::destroy_page_panel(&app, &page.label);
        }
        emit_pages_update(&app, &mgr);
    }

    // Notify sidebar to refresh destinations
    if let Some(sidebar) = app.get_webview_window(crate::panel::SIDEBAR_LABEL) {
        let _ = sidebar.emit("destinations-changed", ());
    }
}

#[tauri::command]
pub fn reorder_destinations(
    dest_manager: State<DestinationManager>,
    ordered_ids: Vec<String>,
) {
    dest_manager.reorder(ordered_ids);
}

/// Switch to a destination (clicked in sidebar).
/// Shows the last page for that dest, or creates a new one.
#[tauri::command]
pub fn switch_destination(
    app: AppHandle,
    tab_manager: State<std::sync::Mutex<WebViewTabManager>>,
    dest_manager: State<DestinationManager>,
    id: String,
) -> Result<(), String> {
    let dest = dest_manager
        .get_by_id(&id)
        .ok_or_else(|| format!("Destination '{}' not found", id))?;

    if let Ok(mut mgr) = tab_manager.lock() {
        // Check if there's an existing page for this destination
        if let Some(page) = mgr.get_last_page_for_dest(&id) {
            let label = page.label.clone();
            let page_id = page.id.clone();
            mgr.set_active(&page_id);
            crate::panel::show_page_viewer(&app, &label);
        } else {
            // Create a new page
            let page = mgr.create_page(&id, &dest.name, &dest.icon);
            mgr.set_active(&page.id);
            crate::panel::create_page_panel(&app, &page.label, &dest.url)
                .map_err(|e| e.to_string())?;
            crate::panel::set_active_page_label(&page.label);
        }

        // Notify frontend about page update
        emit_pages_update(&app, &mgr);
    }

    Ok(())
}

/// Send text to the active page viewer
#[tauri::command]
pub fn send_to_active(
    app: AppHandle,
    _tab_manager: State<std::sync::Mutex<WebViewTabManager>>,
    text: String,
) -> Result<(), String> {
    if let Some(label) = crate::panel::get_active_page_label() {
        if let Some(viewer) = app.get_webview_window(&label) {
            let escaped = text
                .replace('\\', "\\\\")
                .replace('`', "\\`")
                .replace('$', "\\$");
            let js = format!(
                "window.__airyInjectText && window.__airyInjectText(`{}`)",
                escaped
            );
            let _ = viewer.eval(&js);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_clipboard_text() -> String {
    String::new()
}

// ─── Page commands ──────────────────────────────────────────────────────────

/// Get all open pages
#[tauri::command]
pub fn get_pages(
    tab_manager: State<std::sync::Mutex<WebViewTabManager>>,
) -> Vec<PageInfo> {
    if let Ok(mgr) = tab_manager.lock() {
        mgr.get_all_pages()
    } else {
        vec![]
    }
}

/// Switch to a specific page
#[tauri::command]
pub fn switch_page(
    app: AppHandle,
    tab_manager: State<std::sync::Mutex<WebViewTabManager>>,
    page_id: String,
) -> Result<(), String> {
    if let Ok(mut mgr) = tab_manager.lock() {
        if let Some(page) = mgr.get_page(&page_id) {
            let label = page.label.clone();
            mgr.set_active(&page_id);
            crate::panel::show_page_viewer(&app, &label);
            emit_pages_update(&app, &mgr);
        }
    }
    Ok(())
}

/// Close a specific page
#[tauri::command]
pub fn close_page(
    app: AppHandle,
    tab_manager: State<std::sync::Mutex<WebViewTabManager>>,
    page_id: String,
) -> Result<(), String> {
    // Collect info while holding the lock, then release before panel operations
    let (removed_label, next_active_label, _pages_snapshot) = {
        let mut mgr = match tab_manager.lock() {
            Ok(m) => m,
            Err(_) => return Ok(()), // poisoned lock — bail gracefully
        };
        let removed_label = mgr.remove_page(&page_id).map(|p| p.label);
        let next_active_label = mgr.get_active_page().map(|p| p.label.clone());
        let pages_snapshot = mgr.get_all_pages();
        let active_id = mgr.active_page_id.clone();
        // Emit pages update while we still have the snapshot
        if let Some(sidebar) = app.get_webview_window(crate::panel::SIDEBAR_LABEL) {
            let _ = sidebar.emit("pages-updated", &pages_snapshot);
            if let Some(ref aid) = active_id {
                let _ = sidebar.emit("active-page-changed", aid);
            }
        }
        (removed_label, next_active_label, pages_snapshot)
    };
    // Lock is released — safe to do panel operations now
    if let Some(label) = removed_label {
        crate::panel::destroy_page_panel(&app, &label);
    }
    if let Some(label) = next_active_label {
        crate::panel::show_page_viewer(&app, &label);
    }
    Ok(())
}

// ─── Picker commands ────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct PickerData {
    destinations: Vec<Destination>,
    text: String,
}

#[tauri::command]
pub fn get_picker_data(
    dest_manager: State<DestinationManager>,
    picker_state: State<crate::PickerState>,
) -> PickerData {
    PickerData {
        destinations: dest_manager.get_all(),
        text: picker_state.0.lock().unwrap().clone(),
    }
}

/// User picked a destination in the picker popup — always creates a NEW page
#[tauri::command]
pub fn pick_destination(
    app: AppHandle,
    dest_manager: State<DestinationManager>,
    tab_manager: State<std::sync::Mutex<WebViewTabManager>>,
    id: String,
    text: String,
) -> Result<(), String> {
    crate::panel::hide_picker(&app);
    crate::panel::show_panel(&app);

    let dest = dest_manager
        .get_by_id(&id)
        .ok_or_else(|| format!("Destination '{}' not found", id))?;

    // Create a new page for this query
    let page_label;
    if let Ok(mut mgr) = tab_manager.lock() {
        let page = mgr.create_page(&id, &dest.name, &dest.icon);
        page_label = page.label.clone();
        mgr.set_active(&page.id);
        emit_pages_update(&app, &mgr);
    } else {
        return Err("Lock failed".to_string());
    }

    // Create the page viewer panel
    crate::panel::create_page_panel(&app, &page_label, &dest.url)
        .map_err(|e| e.to_string())?;
    crate::panel::set_active_page_label(&page_label);

    // Inject content after page loads
    let app2 = app.clone();
    let label_clone = page_label.clone();
    let is_screenshot = text.starts_with("__screenshot__:");
    std::thread::spawn(move || {
        if is_screenshot {
            // Screenshot mode: inject image via synthetic paste event with DataTransfer.
            // Uses a guard flag to only inject ONCE — no retries after success.
            let data_url = text.strip_prefix("__screenshot__:").unwrap_or("");
            let paste_js = format!(r#"
(function() {{
    // Guard: only inject once
    if (sessionStorage.getItem('__peekabrowserScreenshotDone')) return;

    var dataUrl = "{}";
    // Convert data URL to Blob
    var parts = dataUrl.split(',');
    var mime = parts[0].match(/:(.*?);/)[1];
    var b64 = atob(parts[1]);
    var arr = new Uint8Array(b64.length);
    for (var i = 0; i < b64.length; i++) arr[i] = b64.charCodeAt(i);
    var blob = new Blob([arr], {{ type: mime }});
    var file = new File([blob], 'screenshot.png', {{ type: mime }});

    // Find the input element
    var selectors = [
        '#prompt-textarea',
        'div.ProseMirror[contenteditable]',
        'rich-textarea [contenteditable="true"]',
        'textarea:not([readonly])',
        '[contenteditable="true"]'
    ];
    var target = null;
    for (var sel of selectors) {{
        var el = document.querySelector(sel);
        if (el && el.offsetParent !== null) {{ target = el; break; }}
    }}
    if (!target) return; // Page not ready yet, let retry handle it
    target.focus();

    // Mark as done BEFORE dispatching (prevent re-entry)
    sessionStorage.setItem('__peekabrowserScreenshotDone', '1');

    // Try synthetic paste event
    var dt = new DataTransfer();
    dt.items.add(file);
    var pasteEvt = new ClipboardEvent('paste', {{
        clipboardData: dt,
        bubbles: true,
        cancelable: true
    }});
    target.dispatchEvent(pasteEvt);

    // Also try drag-and-drop as fallback (only once)
    setTimeout(function() {{
        var dt2 = new DataTransfer();
        dt2.items.add(file);
        var dropEvt = new DragEvent('drop', {{
            dataTransfer: dt2,
            bubbles: true,
            cancelable: true
        }});
        target.dispatchEvent(new DragEvent('dragenter', {{ dataTransfer: dt2, bubbles: true }}));
        target.dispatchEvent(new DragEvent('dragover', {{ dataTransfer: dt2, bubbles: true }}));
        target.dispatchEvent(dropEvt);
    }}, 300);
}})();
"#, data_url);
            // Retry up to 3 times, but the JS guard ensures only the first success takes effect
            for &delay_ms in &[2500u64, 4000, 6000] {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                if let Some(viewer) = app2.get_webview_window(&label_clone) {
                    let _ = viewer.eval(&paste_js);
                }
            }
        } else {
            // Text mode: inject text
            let escaped = text
                .replace('\\', "\\\\")
                .replace('`', "\\`")
                .replace('$', "\\$");
            let inject_js = build_inject_js(&escaped);
            for &delay_ms in &[1500u64, 3000, 5000] {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                if let Some(viewer) = app2.get_webview_window(&label_clone) {
                    let _ = viewer.eval(&inject_js);
                }
            }
        }
    });

    Ok(())
}

/// Build the injection JS string
fn build_inject_js(escaped_text: &str) -> String {
    format!(r#"
(function() {{
    if (sessionStorage.getItem('__peekabrowserInjected')) return;
    var text = `{}`;

    var host = location.hostname;
    if (host.includes('google.com') && !host.includes('gemini')) {{
        var q = document.querySelector('textarea[name="q"], input[name="q"]');
        if (q) {{
            var proto = q.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
            var desc = Object.getOwnPropertyDescriptor(proto, 'value');
            if (desc && desc.set) desc.set.call(q, text);
            q.dispatchEvent(new Event('input', {{bubbles:true}}));
            q.dispatchEvent(new Event('change', {{bubbles:true}}));
            setTimeout(function() {{
                var form = q.closest('form');
                if (form) form.submit();
            }}, 300);
            sessionStorage.setItem('__peekabrowserInjected', '1');
            return;
        }}
    }}

    function pressEnter(el) {{
        setTimeout(function() {{
            el.dispatchEvent(new KeyboardEvent('keydown', {{key:'Enter', code:'Enter', keyCode:13, which:13, bubbles:true}}));
            el.dispatchEvent(new KeyboardEvent('keypress', {{key:'Enter', code:'Enter', keyCode:13, which:13, bubbles:true}}));
            el.dispatchEvent(new KeyboardEvent('keyup', {{key:'Enter', code:'Enter', keyCode:13, which:13, bubbles:true}}));
            var submitBtn = document.querySelector('button[aria-label*="Send"], button[aria-label*="send"], button[data-testid="send-button"], button.send-button, button[type="submit"]');
            if (submitBtn) submitBtn.click();
        }}, 500);
    }}

    if (host.includes('gemini.google.com')) {{
        var rich = document.querySelector('rich-textarea');
        if (rich) {{
            var inner = rich.querySelector('.ql-editor, [contenteditable="true"], .textarea');
            if (!inner) inner = rich.querySelector('div[contenteditable], p[contenteditable]');
            if (!inner) inner = rich;
            inner.focus();
            document.execCommand('selectAll', false, null);
            document.execCommand('insertText', false, text);
            rich.dispatchEvent(new Event('input', {{bubbles:true}}));
            setTimeout(function() {{
                var sendBtn = document.querySelector('button.send-button, button[aria-label*="Send"], button[aria-label*="送出"], .send-button-container button, button[data-test-id="send-button"]');
                if (sendBtn) sendBtn.click();
                else pressEnter(inner);
            }}, 500);
            sessionStorage.setItem('__peekabrowserInjected', '1');
            return;
        }}
        var ce = document.querySelector('[contenteditable="true"]');
        if (ce) {{
            ce.focus();
            document.execCommand('selectAll', false, null);
            document.execCommand('insertText', false, text);
            pressEnter(ce);
            sessionStorage.setItem('__peekabrowserInjected', '1');
            return;
        }}
    }}

    var selectors = [
        '#prompt-textarea',
        'div.ProseMirror[contenteditable]',
        'textarea[placeholder*="Message"]',
        'textarea[placeholder*="Ask"]',
        'textarea:not([readonly])',
        'input[type="text"]:not([readonly])',
        '[contenteditable="true"]'
    ];
    for (var sel of selectors) {{
        var el = document.querySelector(sel);
        if (el && el.offsetParent !== null) {{
            if (el.tagName === 'TEXTAREA' || el.tagName === 'INPUT') {{
                var desc = Object.getOwnPropertyDescriptor(
                    el.tagName === 'TEXTAREA' ? window.HTMLTextAreaElement.prototype : window.HTMLInputElement.prototype,
                    'value'
                );
                if (desc && desc.set) {{ desc.set.call(el, text); }}
                el.dispatchEvent(new Event('input', {{bubbles: true}}));
                el.dispatchEvent(new Event('change', {{bubbles: true}}));
            }} else {{
                el.focus();
                document.execCommand('selectAll', false, null);
                document.execCommand('insertText', false, text);
            }}
            pressEnter(el);
            sessionStorage.setItem('__peekabrowserInjected', '1');
            return;
        }}
    }}
}})();
"#, escaped_text)
}

#[tauri::command]
pub fn hide_picker_panel(app: AppHandle) {
    crate::panel::hide_picker(&app);
}

// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn set_viewer_width(app: AppHandle, preset: String) {
    let (screen_width, _) = crate::panel::current_screen_size();
    let (total_width, height_ratio) = match preset.as_str() {
        "short" => (screen_width / 3.0, 0.50),
        "long" => (screen_width * 2.0 / 3.0, 0.85),
        _ => (screen_width / 2.0, 0.70),
    };
    let viewer_width = (total_width - crate::panel::TAB_BAR_WIDTH).max(200.0);
    crate::panel::set_viewer_width_value(viewer_width);
    crate::panel::set_height_ratio(height_ratio);
    crate::panel::resize_panels(&app, viewer_width);
}

#[tauri::command]
pub fn open_settings_window(app: AppHandle) {
    use tauri::WebviewWindowBuilder;
    let label = "settings-window";
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.set_focus();
        return;
    }

    let (screen_w, screen_h) = crate::panel::get_primary_screen_size();
    let win_w = 520.0_f64;
    let win_h = 480.0_f64;
    let x = (screen_w - win_w) / 2.0;
    let y = (screen_h - win_h) / 2.0;

    let _ = WebviewWindowBuilder::new(
        &app,
        label,
        tauri::WebviewUrl::App("settings.html".into()),
    )
    .title("Peekabrowser Settings")
    .inner_size(win_w, win_h)
    .position(x, y)
    .resizable(true)
    .decorations(true)
    .always_on_top(true)
    .visible(true)
    .build();
}

#[tauri::command]
pub fn open_settings_url(url: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Screenshot: hide sidebar, use macOS screencapture interactive mode, show picker.
#[tauri::command]
pub fn take_screenshot(app: AppHandle) {
    crate::panel::hide_panel(&app);

    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));

        let tmp_path = "/tmp/peekabrowser_screenshot.png";
        let _ = std::fs::remove_file(tmp_path);

        let status = std::process::Command::new("/usr/sbin/screencapture")
            .args(["-i", "-x", tmp_path])
            .status();

        log::info!("screencapture status: {:?}", status);

        let (cx, cy) = crate::panel::get_cursor_topleft_pos();

        match status {
            Ok(s) if s.success() && std::path::Path::new(tmp_path).exists() => {
                if let Ok(data) = std::fs::read(tmp_path) {
                    let b64 = base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        &data,
                    );
                    let data_url = format!("data:image/png;base64,{}", b64);
                    if let Some(state) = app.try_state::<crate::PickerState>() {
                        *state.0.lock().unwrap() = format!("__screenshot__:{}", data_url);
                    }
                    let _ = std::fs::remove_file(tmp_path);
                    log::info!("Screenshot captured, showing picker");
                }
                // Wake up the Accessory app, then show picker on main thread
                crate::hotkeys::global_shortcuts::activate_app();
                let app2 = app.clone();
                let _ = app.run_on_main_thread(move || {
                    crate::panel::show_picker(&app2, cx, cy);
                });
            }
            Ok(_) => {
                log::info!("Screenshot cancelled");
                crate::hotkeys::global_shortcuts::activate_app();
                let app2 = app.clone();
                let _ = app.run_on_main_thread(move || {
                    crate::panel::show_panel(&app2);
                });
            }
            _ => {
                log::warn!("screencapture failed or permission denied");
                crate::permissions::open_screen_recording_settings();
                crate::hotkeys::global_shortcuts::activate_app();
                let app2 = app.clone();
                let _ = app.run_on_main_thread(move || {
                    crate::panel::show_panel(&app2);
                });
            }
        }
    });
}

/// Reload the active page viewer
#[tauri::command]
pub fn reload_active_page(app: AppHandle) {
    if let Some(label) = crate::panel::get_active_page_label() {
        if let Some(viewer) = app.get_webview_window(&label) {
            let _ = viewer.eval("location.reload()");
        }
    }
}

// ─── Shortcut commands ──────────────────────────────────────────────────────

#[tauri::command]
pub fn get_shortcuts(
    store: State<crate::hotkeys::shortcut_store::ShortcutStore>,
) -> crate::hotkeys::shortcut_store::ShortcutConfig {
    store.get()
}

#[tauri::command]
pub fn save_shortcuts(
    app: AppHandle,
    store: State<crate::hotkeys::shortcut_store::ShortcutStore>,
    config: crate::hotkeys::shortcut_store::ShortcutConfig,
) -> Result<(), String> {
    // Validate all shortcuts before saving
    for (name, val) in [
        ("toggle_sidebar", &config.toggle_sidebar),
        ("screenshot", &config.screenshot),
        ("export", &config.export),
    ] {
        if crate::hotkeys::shortcut_store::parse_shortcut(val).is_none() {
            return Err(format!("Invalid shortcut for {}: {}", name, val));
        }
    }

    store.update(config);

    // Re-register shortcuts with new config
    if let Err(e) = crate::hotkeys::global_shortcuts::re_register_shortcuts(&app) {
        return Err(format!("Failed to register shortcuts: {}", e));
    }

    Ok(())
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Emit pages update to the sidebar frontend
fn emit_pages_update(app: &AppHandle, mgr: &WebViewTabManager) {
    if let Some(sidebar) = app.get_webview_window(crate::panel::SIDEBAR_LABEL) {
        let _ = sidebar.emit("pages-updated", mgr.get_all_pages());
        if let Some(active) = mgr.get_active_page() {
            let _ = sidebar.emit("active-page-changed", &active.id);
        }
    }
}
