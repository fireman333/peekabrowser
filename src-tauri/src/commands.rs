use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::destinations::{Destination, DestinationManager};
use crate::webviews::{PageInfo, WebViewTabManager};

/// State for the system config window (Calendar/Reminders)
pub struct SystemConfigState {
    pub item_type: Mutex<String>,  // "calendar" or "reminders"
    pub text: Mutex<String>,
    pub needs_ocr: Mutex<bool>,    // true if text should come from OCR
}

impl SystemConfigState {
    pub fn new() -> Self {
        Self {
            item_type: Mutex::new(String::new()),
            text: Mutex::new(String::new()),
            needs_ocr: Mutex::new(false),
        }
    }
}

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
pub fn open_system_app(app_name: String) -> Result<(), String> {
    std::process::Command::new("open")
        .arg("-a")
        .arg(&app_name)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
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
    // Validate URL (skip system:// internal URLs)
    let validated_url = if url.starts_with("system://") {
        url
    } else if !url.starts_with("http://") && !url.starts_with("https://") {
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
    app: AppHandle,
    dest_manager: State<DestinationManager>,
    ordered_ids: Vec<String>,
) {
    dest_manager.reorder(ordered_ids);
    if let Some(sidebar) = app.get_webview_window(crate::panel::SIDEBAR_LABEL) {
        let _ = sidebar.emit("destinations-changed", ());
    }
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

/// Handle system:// destinations — store state and open config window
fn handle_system_destination(app: &AppHandle, url: &str, text: &str) -> Result<(), String> {
    let item_type = if url.contains("calendar") {
        "calendar"
    } else if url.contains("reminders") {
        "reminders"
    } else {
        return Err(format!("Unknown system destination: {}", url));
    };

    // Store config state for the window to read
    if let Some(state) = app.try_state::<SystemConfigState>() {
        *state.item_type.lock().unwrap() = item_type.to_string();
        *state.text.lock().unwrap() = text.to_string();
    }

    // Open the config window
    open_system_config_window_inner(app);
    Ok(())
}

fn open_system_config_window_inner(app: &AppHandle) {
    use tauri::WebviewWindowBuilder;
    let label = "system-config";
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.set_focus();
        return;
    }

    let (screen_w, screen_h) = crate::panel::get_primary_screen_size();
    let win_w = 420.0_f64;
    let win_h = 380.0_f64;
    let x = (screen_w - win_w) / 2.0;
    let y = (screen_h - win_h) / 2.0;

    let _ = WebviewWindowBuilder::new(
        app,
        label,
        tauri::WebviewUrl::App("system-config.html".into()),
    )
    .title("Quick Create")
    .inner_size(win_w, win_h)
    .position(x, y)
    .resizable(false)
    .decorations(true)
    .always_on_top(true)
    .visible(true)
    .build();
}

#[derive(Serialize)]
pub struct SystemConfigData {
    item_type: String,
    text: String,
    lists: Vec<String>,
    needs_ocr: bool,
}

#[tauri::command]
pub fn get_system_config_data(
    config_state: State<SystemConfigState>,
) -> Result<SystemConfigData, String> {
    let item_type = config_state.item_type.lock().unwrap().clone();
    let text = config_state.text.lock().unwrap().clone();

    // Query available lists via AppleScript (launches app if needed)
    let lists = if item_type == "calendar" {
        query_applescript_list(
            "Calendar",
            r#"tell application "Calendar" to name of every calendar whose writable is true"#,
        )
    } else {
        query_applescript_list(
            "Reminders",
            r#"tell application "Reminders" to name of every list"#,
        )
    };

    let needs_ocr = config_state.needs_ocr.lock().unwrap().clone();

    Ok(SystemConfigData {
        item_type,
        text,
        lists,
        needs_ocr,
    })
}

fn query_applescript_list(app_name: &str, script: &str) -> Vec<String> {
    // Ensure the app is running first (required for AppleScript queries)
    let _ = std::process::Command::new("open")
        .args(["-gj", "-a", app_name]) // -g: don't bring to front, -j: launch hidden
        .output();
    // Small delay for the app to initialize
    std::thread::sleep(std::time::Duration::from_millis(1500));

    match std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
    {
        Ok(out) if out.status.success() => {
            let raw = String::from_utf8_lossy(&out.stdout);
            raw.trim()
                .split(", ")
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect()
        }
        Ok(out) => {
            log::warn!("AppleScript list query failed: {}", String::from_utf8_lossy(&out.stderr));
            vec![]
        }
        Err(e) => {
            log::error!("osascript exec failed: {}", e);
            vec![]
        }
    }
}

#[tauri::command]
pub fn create_system_item(
    app: AppHandle,
    item_type: String,
    text: String,
    list_name: String,
    start_time: Option<String>,
    end_time: Option<String>,
) -> Result<(), String> {
    let text_escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    let list_escaped = list_name.replace('\\', "\\\\").replace('"', "\\\"");

    let script = if item_type == "calendar" {
        // Build date setting AppleScript
        let date_script = if let Some(ref start) = start_time {
            let end = end_time.as_deref().unwrap_or(start);
            format!(
                r#"set startDate to my parseDate("{}")
        set endDate to my parseDate("{}")"#,
                start, end
            )
        } else {
            "set startDate to (current date)\n        set endDate to startDate + 3600".to_string()
        };

        format!(
            r#"on parseDate(dateStr)
    -- dateStr format: "2026-03-29T14:30"
    set oldDelims to AppleScript's text item delimiters
    set AppleScript's text item delimiters to {{"T", "-", ":"}}
    set parts to text items of dateStr
    set AppleScript's text item delimiters to oldDelims
    set d to current date
    set year of d to (item 1 of parts) as integer
    set month of d to (item 2 of parts) as integer
    set day of d to (item 3 of parts) as integer
    set hours of d to (item 4 of parts) as integer
    set minutes of d to (item 5 of parts) as integer
    set seconds of d to 0
    return d
end parseDate

tell application "Calendar"
    tell calendar "{}"
        {}
        make new event at end with properties {{summary:"{}", start date:startDate, end date:endDate}}
    end tell
end tell"#,
            list_escaped, date_script, text_escaped
        )
    } else {
        // Reminders
        let due_part = if let Some(ref start) = start_time {
            if !start.is_empty() {
                format!(
                    r#", due date:my parseDate("{}")"#,
                    start
                )
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        if due_part.is_empty() {
            format!(
                r#"tell application "Reminders"
    tell list "{}"
        make new reminder with properties {{name:"{}"}}
    end tell
end tell"#,
                list_escaped, text_escaped
            )
        } else {
            format!(
                r#"on parseDate(dateStr)
    set oldDelims to AppleScript's text item delimiters
    set AppleScript's text item delimiters to {{"T", "-", ":"}}
    set parts to text items of dateStr
    set AppleScript's text item delimiters to oldDelims
    set d to current date
    set year of d to (item 1 of parts) as integer
    set month of d to (item 2 of parts) as integer
    set day of d to (item 3 of parts) as integer
    set hours of d to (item 4 of parts) as integer
    set minutes of d to (item 5 of parts) as integer
    set seconds of d to 0
    return d
end parseDate

tell application "Reminders"
    tell list "{}"
        make new reminder with properties {{name:"{}"{}}}
    end tell
end tell"#,
                list_escaped, text_escaped, due_part
            )
        }
    };

    let is_calendar = item_type == "calendar";
    std::thread::spawn(move || {
        // Ensure the target app is running
        let app_name = if is_calendar { "Calendar" } else { "Reminders" };
        let _ = std::process::Command::new("open")
            .args(["-gj", "-a", app_name])
            .output();
        std::thread::sleep(std::time::Duration::from_millis(1000));

        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output();
        match output {
            Ok(out) if out.status.success() => {
                let kind = if is_calendar { "行事曆事件" } else { "提醒事項" };
                let notif = format!(
                    r#"display notification "已建立{}" with title "Peekabrowser""#,
                    kind
                );
                let _ = std::process::Command::new("osascript")
                    .arg("-e")
                    .arg(&notif)
                    .output();
            }
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr);
                eprintln!("AppleScript error: {}", err);
            }
            Err(e) => {
                eprintln!("Failed to run osascript: {}", e);
            }
        }
    });

    // Close config window
    if let Some(w) = app.get_webview_window("system-config") {
        let _ = w.close();
    }

    Ok(())
}

#[tauri::command]
pub fn close_system_config(app: AppHandle) {
    if let Some(w) = app.get_webview_window("system-config") {
        let _ = w.close();
    }
}

/// Public command: run OCR on the last screenshot and return extracted text.
/// Called asynchronously by the system-config window frontend.
#[tauri::command]
pub fn run_ocr(app: AppHandle) -> Result<String, String> {
    ocr_screenshot(&app)
}

/// Run OCR on the screenshot using the bundled ocr-helper binary.
/// If the bundled binary can't be found or executed, compiles from
/// embedded Swift source as a fallback (cached for subsequent calls).
fn ocr_screenshot(_app: &AppHandle) -> Result<String, String> {
    let screenshot_path = "/tmp/peekabrowser_screenshot.png";
    if !std::path::Path::new(screenshot_path).exists() {
        log::error!("OCR: screenshot file not found");
        return Err("Screenshot file not found".to_string());
    }

    // Try to find and use the bundled binary first
    let bundled_binary = std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.parent()
                .and_then(|macos| macos.parent())
                .map(|contents| contents.join("Resources/assets/ocr-helper"))
        })
        .filter(|p| p.exists());

    if let Some(ref binary_path) = bundled_binary {
        log::info!("OCR: trying bundled binary at {:?}", binary_path);
        // Clear quarantine attribute if present
        let _ = std::process::Command::new("xattr")
            .args(["-d", "com.apple.quarantine"])
            .arg(binary_path)
            .output();

        let output = std::process::Command::new(binary_path)
            .arg(screenshot_path)
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                log::info!("OCR: bundled binary success, {} chars", text.len());
                return Ok(text);
            }
            Ok(out) => {
                log::warn!("OCR: bundled binary failed: {}", String::from_utf8_lossy(&out.stderr));
            }
            Err(e) => {
                log::warn!("OCR: bundled binary exec error: {}", e);
            }
        }
    }

    // Fallback: compile from source and cache the binary
    let cached_binary = "/tmp/peekabrowser_ocr_helper";
    if !std::path::Path::new(cached_binary).exists() {
        log::info!("OCR: compiling Swift helper from source...");
        let swift_src = include_str!("../ocr-helper/main.swift");
        let src_path = "/tmp/peekabrowser_ocr.swift";
        std::fs::write(src_path, swift_src)
            .map_err(|e| format!("Write OCR source failed: {}", e))?;

        let compile = std::process::Command::new("swiftc")
            .args(["-O", src_path, "-o", cached_binary])
            .output()
            .map_err(|e| format!("swiftc failed: {}", e))?;

        if !compile.status.success() {
            let err = String::from_utf8_lossy(&compile.stderr);
            log::error!("OCR: compile failed: {}", err);
            return Err(format!("OCR compile failed: {}", err));
        }
        log::info!("OCR: compiled successfully to {}", cached_binary);
    }

    let output = std::process::Command::new(cached_binary)
        .arg(screenshot_path)
        .output()
        .map_err(|e| format!("OCR exec failed: {}", e))?;

    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        log::info!("OCR: extracted {} chars", text.len());
        Ok(text)
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        log::error!("OCR: process failed: {}", err);
        Err(format!("OCR failed: {}", err))
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

    let dest = dest_manager
        .get_by_id(&id)
        .ok_or_else(|| format!("Destination '{}' not found", id))?;

    // Handle system:// destinations (Calendar, Reminders) via AppleScript
    // Also handle "https://system://" which can happen if URL was auto-prefixed
    let system_url = if dest.url.starts_with("system://") {
        Some(dest.url.clone())
    } else if dest.url.starts_with("https://system://") {
        Some(dest.url.replace("https://system://", "system://"))
    } else {
        None
    };
    if let Some(sys_url) = system_url {
        let is_screenshot = text.starts_with("__screenshot__:");
        let actual_text = if is_screenshot {
            String::new() // OCR will be done async by the config window
        } else {
            text.clone()
        };
        // Set needs_ocr flag so the config window knows to run OCR
        if let Some(state) = app.try_state::<SystemConfigState>() {
            *state.needs_ocr.lock().unwrap() = is_screenshot;
        }
        return handle_system_destination(&app, &sys_url, &actual_text);
    }

    crate::panel::show_panel(&app);

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
                    // Keep screenshot file for potential OCR use by system destinations
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
