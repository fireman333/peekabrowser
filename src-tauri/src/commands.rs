use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::delivery::{Delivery, Payload, PickerState};
use crate::i18n::tr;
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

/// Toggle pin state — when pinned, auto-hide is disabled.
/// Returns the new pinned state.
#[tauri::command]
pub fn toggle_pin() -> bool {
    crate::panel::hover_detector::toggle_pin()
}

/// Check if panel is currently pinned
#[tauri::command]
pub fn is_pinned() -> bool {
    crate::panel::hover_detector::is_pinned()
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
    clip_prompt: Option<String>,
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
        clip_prompt: clip_prompt.unwrap_or_default(),
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
    clip_prompt: Option<String>,
) -> Result<Destination, String> {
    dest_manager
        .update(&id, name, url, icon, clip_prompt.unwrap_or_default())
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
    let _ = tab_manager;
    crate::lifecycle::remove_pages_for_dest(&app, &id);

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
/// Shows the last page for that dest (restoring it if unloaded), or creates one.
#[tauri::command]
pub fn switch_destination(
    app: AppHandle,
    dest_manager: State<DestinationManager>,
    id: String,
) -> Result<(), String> {
    let dest = dest_manager
        .get_by_id(&id)
        .ok_or_else(|| format!("Destination '{}' not found", id))?;
    crate::lifecycle::switch_destination(&app, &dest)
}

/// Open a new tab for the given destination (always creates a new page).
#[tauri::command]
pub fn new_tab(
    app: AppHandle,
    dest_manager: State<DestinationManager>,
    id: String,
) -> Result<(), String> {
    let dest = dest_manager
        .get_by_id(&id)
        .ok_or_else(|| format!("Destination '{}' not found", id))?;
    crate::lifecycle::open_new_page(&app, &dest).map(|_| ())
}

/// Open a new tab for the currently active destination (called from page viewer's Cmd+N).
#[tauri::command]
pub fn new_tab_for_active(
    app: AppHandle,
    tab_manager: State<std::sync::Mutex<WebViewTabManager>>,
    dest_manager: State<DestinationManager>,
) -> Result<(), String> {
    let dest_id = {
        let mgr = tab_manager.lock().map_err(|_| "Lock failed")?;
        mgr.get_active_page()
            .map(|p| p.dest_id.clone())
            .ok_or_else(|| tr("沒有開啟中的頁面", "No active page").to_string())?
    };
    new_tab(app, dest_manager, dest_id)
}

/// Send text to the active page viewer
#[tauri::command]
pub fn send_to_active(app: AppHandle, text: String) -> Result<(), String> {
    let target = crate::lifecycle::active_target(&app).ok_or(tr("沒有開啟中的頁面", "No active page"))?;
    crate::delivery::spawn(
        app,
        Delivery { target, payload: Payload::Text { text }, prompt: String::new(), record_id: None },
    );
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

/// Switch to a specific page (restores it if it was unloaded)
#[tauri::command]
pub fn switch_page(app: AppHandle, page_id: String) -> Result<(), String> {
    crate::lifecycle::activate_page(&app, &page_id)
}

/// Close a specific page
#[tauri::command]
pub fn close_page(app: AppHandle, page_id: String) -> Result<(), String> {
    crate::lifecycle::close_page(&app, &page_id);
    Ok(())
}

// ─── Picker commands ────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct PickerData {
    destinations: Vec<Destination>,
    /// "text" | "image" | "" (nothing pending)
    kind: String,
    /// Text to send (text payloads).
    text: String,
    /// Small preview for image payloads (data URL).
    image_preview: Option<String>,
}

#[tauri::command]
pub fn get_picker_data(
    dest_manager: State<DestinationManager>,
    picker_state: State<PickerState>,
) -> PickerData {
    let pending = picker_state.0.lock().map(|s| s.payload.clone()).unwrap_or(None);
    let (kind, text, image_preview) = match pending {
        Some(Payload::Text { text }) => ("text", text, None),
        Some(Payload::Image { path }) => {
            let preview = std::fs::read(&path).ok().map(|d| {
                format!("data:image/png;base64,{}", base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &d))
            });
            ("image", String::new(), preview)
        }
        None => ("", String::new(), None),
    };
    PickerData { destinations: dest_manager.get_all(), kind: kind.into(), text, image_preview }
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
    .title(window_title("system-config").unwrap_or("Quick Create"))
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
pub async fn get_system_config_data(
    config_state: State<'_, SystemConfigState>,
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
pub async fn run_ocr() -> Result<String, String> {
    ocr_image(&crate::screenshot::capture_path())
}

/// Run OCR on the screenshot using the bundled ocr-helper binary.
/// If the bundled binary can't be found or executed, compiles from
/// embedded Swift source as a fallback (cached for subsequent calls).
pub fn ocr_image(path: &std::path::Path) -> Result<String, String> {
    let screenshot_path = path;
    if !screenshot_path.exists() {
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

/// User picked a destination in the picker popup — always creates a NEW page.
/// The payload comes from the pending query captured at trigger time; `text`
/// is accepted for compatibility but a typed payload is preferred.
#[tauri::command]
pub fn pick_destination(app: AppHandle, id: String, text: Option<String>) -> Result<(), String> {
    pick_destination_impl(&app, &id, text)
}

/// Pick the n-th destination in picker order (keyboard shortcut / auto-send).
pub fn pick_destination_by_index(app: &AppHandle, index: usize) -> Result<(), String> {
    let id = app
        .state::<DestinationManager>()
        .get_all()
        .get(index)
        .map(|d| d.id.clone())
        .ok_or("No destination at that position")?;
    pick_destination_impl(app, &id, None)
}

/// Shared by the picker click, its keyboard shortcuts and auto-send.
/// Must run on the main thread (creates/positions windows).
pub fn pick_destination_impl(app: &AppHandle, id: &str, text: Option<String>) -> Result<(), String> {
    let app = app.clone();
    let id = id.to_string();
    let dest_manager = app.state::<DestinationManager>();
    let picker_state = app.state::<PickerState>();
    crate::panel::hide_picker(&app);

    let dest = dest_manager
        .get_by_id(&id)
        .ok_or_else(|| format!("Destination '{}' not found", id))?;

    // Take the pending query so a later pick can't resend stale content.
    let pending = picker_state.0.lock().map(|mut s| std::mem::take(&mut *s)).unwrap_or_default();
    let payload = match (pending.payload, text) {
        (Some(p), _) => p,
        (None, Some(t)) if !t.is_empty() => Payload::Text { text: t },
        _ => return Err(tr("沒有可傳送的內容", "Nothing to send").into()),
    };

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
        let (is_image, actual_text) = match &payload {
            Payload::Image { .. } => (true, String::new()), // OCR runs async in the config window
            Payload::Text { text } => (false, text.clone()),
        };
        if let Some(state) = app.try_state::<SystemConfigState>() {
            *state.needs_ocr.lock().unwrap() = is_image;
        }
        return handle_system_destination(&app, &sys_url, &actual_text);
    }

    crate::panel::show_panel(&app);

    // Record what is being asked, where, and from which app.
    let record_id = app.try_state::<crate::records::RecordStore>().and_then(|store| {
        let (selection, attachment) = match &payload {
            Payload::Text { text } => (Some(text.clone()), None),
            Payload::Image { path } => {
                let name = format!("{}.png", uuid::Uuid::new_v4());
                let _ = std::fs::create_dir_all(&store.attachments_dir);
                let ok = std::fs::copy(path, store.attachments_dir.join(&name)).is_ok();
                (None, ok.then(|| format!("attachments/{}", name)))
            }
        };
        let prompt = match &payload {
            Payload::Text { text } => format!("{}{}", dest.clip_prompt, text),
            Payload::Image { .. } => dest.clip_prompt.clone(),
        };
        store
            .create_query(crate::records::NewQuery {
                source_app: pending.source_app.clone(),
                selection_text: selection,
                attachment_path: attachment,
                action_id: "send".into(),
                destination_id: dest.id.clone(),
                destination_name: dest.name.clone(),
                prompt,
            })
            .map_err(|e| log::warn!("records: create failed: {}", e))
            .ok()
            .map(|r| r.id)
    });
    if let Some(w) = app.get_webview_window("records-window") {
        let _ = w.emit("records-changed", ());
    }

    let page = crate::lifecycle::open_new_page(&app, &dest)?;
    if let Some(rid) = &record_id {
        crate::lifecycle::link_query(&app, &page.id, rid);
    }
    let label = page.label.clone().ok_or("page has no viewer")?;
    let target = crate::delivery::Target {
        page_id: page.id.clone(),
        slot_gen: crate::panel::slot_generation(&label).ok_or("viewer slot missing")?,
        label,
    };
    crate::delivery::spawn(
        app.clone(),
        Delivery { target, payload, prompt: dest.clip_prompt.clone(), record_id },
    );
    Ok(())
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
    let win_w = 560.0_f64;
    let win_h = 640.0_f64;
    let x = (screen_w - win_w) / 2.0;
    let y = (screen_h - win_h) / 2.0;

    let _ = WebviewWindowBuilder::new(
        &app,
        label,
        tauri::WebviewUrl::App("settings.html".into()),
    )
    .title(window_title("settings-window").unwrap_or("Peekabrowser Settings"))
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
    crate::screenshot::capture_to_picker(&app);
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

/// Navigate the active page viewer back
#[tauri::command]
pub fn go_back(app: AppHandle) {
    if let Some(label) = crate::panel::get_active_page_label() {
        if let Some(viewer) = app.get_webview_window(&label) {
            let _ = viewer.eval("history.back()");
        }
    }
}

/// Navigate the active page viewer forward
#[tauri::command]
pub fn go_forward(app: AppHandle) {
    if let Some(label) = crate::panel::get_active_page_label() {
        if let Some(viewer) = app.get_webview_window(&label) {
            let _ = viewer.eval("history.forward()");
        }
    }
}

/// Open the active page's current URL in the default browser
#[tauri::command]
pub fn open_active_in_browser(app: AppHandle) -> Result<(), String> {
    let label = crate::panel::get_active_page_label()
        .ok_or(tr("沒有開啟中的頁面", "No active page"))?;
    let viewer = app.get_webview_window(&label)
        .ok_or("Page viewer not found")?;
    let url = viewer.url().map_err(|e| e.to_string())?;
    std::process::Command::new("open")
        .arg(url.as_str())
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
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

// ─── Answer saving & records ────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SaveResult {
    pub record_id: String,
    pub capture_status: String,
    pub chars: usize,
}

/// Capture the answer on the active page (or the user's selection there) and
/// store it with the query that page was opened for. Saving again updates the
/// same record. Must run off the main thread.
pub fn save_answer_blocking(app: &AppHandle) -> Result<SaveResult, String> {
    let (page_id, label, query_id, dest_id, dest_name) = {
        let mgr = app.state::<std::sync::Mutex<WebViewTabManager>>();
        let mgr = mgr.lock().map_err(|_| "lock")?;
        let p = mgr.get_active_page().ok_or(tr("沒有開啟中的頁面", "No active page"))?;
        (
            p.id.clone(),
            p.label.clone().ok_or(tr("頁面尚未載入", "Page is not loaded"))?,
            p.query_id.clone(),
            p.dest_id.clone(),
            p.dest_name.clone(),
        )
    };
    let capture: crate::records::Capture = crate::delivery::call(app, &label, "extract", &[])
        .ok_or("Couldn't read this page")?;
    if capture.text.trim().is_empty() && capture.markdown.trim().is_empty() {
        return Err(tr("找不到回答 — 請先選取要儲存的文字再試一次。", "No answer found — select the text you want to save, then try again.").into());
    }
    let store = app.try_state::<crate::records::RecordStore>().ok_or(tr("無法使用紀錄功能", "Records unavailable"))?;
    let existing = query_id.filter(|id| store.get(id).ok().flatten().is_some());
    let record_id = match existing {
        Some(id) => id,
        None => {
            let r = store
                .create_query(crate::records::NewQuery {
                    action_id: "manual".into(),
                    destination_id: dest_id,
                    destination_name: dest_name,
                    ..Default::default()
                })
                .map_err(|e| e.to_string())?;
            crate::lifecycle::link_query(app, &page_id, &r.id);
            r.id
        }
    };
    store.save_capture(&record_id, &capture).map_err(|e| e.to_string())?;
    if let Some(sidebar) = app.get_webview_window(crate::panel::SIDEBAR_LABEL) {
        let _ = sidebar.emit("records-changed", ());
    }
    if let Some(w) = app.get_webview_window("records-window") {
        let _ = w.emit("records-changed", ());
    }
    Ok(SaveResult { record_id, capture_status: capture.capture_status, chars: capture.text.chars().count() })
}

/// Shared by the sidebar button and the ⌘⇧E shortcut: save and report.
pub fn save_answer_and_notify(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let msg = match save_answer_blocking(&app) {
            Ok(r) => match r.capture_status.as_str() {
                "partial" => tr("已儲存（仍在生成中，完成後請再儲存一次）", "Saved (still generating — save again when it finishes)").to_string(),
                "manual_selection" => tr("已儲存選取內容", "Saved selection").to_string(),
                "unknown" => tr("已儲存（此網站無法判斷回答是否完整）", "Saved (completeness unknown on this site)").to_string(),
                _ => tr("已儲存回答", "Answer saved").to_string(),
            },
            Err(e) => e,
        };
        crate::lifecycle::notify(&app, &msg);
    });
}

#[tauri::command]
pub async fn save_answer(app: AppHandle) -> Result<SaveResult, String> {
    save_answer_blocking(&app)
}

#[tauri::command]
pub fn list_records(
    store: State<crate::records::RecordStore>,
    query: Option<String>,
    favorites_only: Option<bool>,
) -> Result<Vec<crate::records::Record>, String> {
    store
        .list(query.as_deref(), favorites_only.unwrap_or(false), 200)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_record(
    store: State<crate::records::RecordStore>,
    id: String,
    tags: Vec<String>,
    note: Option<String>,
    favorite: bool,
) -> Result<(), String> {
    store.update_user_fields(&id, &tags, note.as_deref(), favorite).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_record(store: State<crate::records::RecordStore>, id: String) -> Result<bool, String> {
    store.delete(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_record_attachment(store: State<crate::records::RecordStore>, id: String) -> Option<String> {
    let rel = store.get(&id).ok().flatten()?.attachment_path?;
    let data = std::fs::read(store.attachment_abs(&rel)?).ok()?;
    Some(format!("data:image/png;base64,{}", base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data)))
}

#[tauri::command]
pub fn copy_record_markdown(app: AppHandle, store: State<crate::records::RecordStore>, id: String) -> Result<(), String> {
    let r = store.get(&id).map_err(|e| e.to_string())?.ok_or(tr("找不到這筆紀錄", "Record not found"))?;
    crate::delivery::copy_to_clipboard(&app, &crate::records::to_markdown(&r));
    Ok(())
}

/// Write the record as Markdown into ~/Downloads and reveal it in Finder.
#[tauri::command]
pub fn export_record_markdown(store: State<crate::records::RecordStore>, id: String) -> Result<String, String> {
    let r = store.get(&id).map_err(|e| e.to_string())?.ok_or(tr("找不到這筆紀錄", "Record not found"))?;
    let home = std::env::var_os("HOME").ok_or("No HOME")?;
    let dir = std::path::PathBuf::from(home).join("Downloads");
    let _ = std::fs::create_dir_all(&dir);
    let stem: String = r
        .selection_text
        .as_deref()
        .unwrap_or(&r.destination_name)
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
        .take(40)
        .collect::<String>()
        .trim()
        .replace(' ', "-");
    let name = format!("peekabrowser-{}-{}.md", if stem.is_empty() { "query" } else { &stem }, &r.id[..8]);
    let path = dir.join(name);
    std::fs::write(&path, crate::records::to_markdown(&r)).map_err(|e| e.to_string())?;
    let _ = std::process::Command::new("open").arg("-R").arg(&path).spawn();
    Ok(path.to_string_lossy().into_owned())
}

/// Reopen a record's conversation in a new page of its destination.
#[tauri::command]
pub fn open_record(
    app: AppHandle,
    store: State<crate::records::RecordStore>,
    dest_manager: State<DestinationManager>,
    id: String,
) -> Result<(), String> {
    let r = store.get(&id).map_err(|e| e.to_string())?.ok_or(tr("找不到這筆紀錄", "Record not found"))?;
    let mut dest = dest_manager.get_by_id(&r.destination_id).ok_or(tr("這個目的地已不存在", "Destination no longer exists"))?;
    if let Some(u) = r.conversation_url.filter(|u| u.starts_with("http")) {
        dest.url = u;
    }
    crate::panel::show_panel(&app);
    let page = crate::lifecycle::open_new_page(&app, &dest)?;
    crate::lifecycle::link_query(&app, &page.id, &id);
    Ok(())
}

#[tauri::command]
pub fn open_records_window(app: AppHandle) {
    open_aux_window(&app, "records-window", "records.html", window_title("records-window").unwrap_or("Peekabrowser Records"), 720.0, 560.0);
}

fn open_aux_window(app: &AppHandle, label: &str, page: &str, title: &str, w: f64, h: f64) {
    use tauri::WebviewWindowBuilder;
    if let Some(win) = app.get_webview_window(label) {
        let _ = win.show();
        let _ = win.set_focus();
        return;
    }
    let (screen_w, screen_h) = crate::panel::get_primary_screen_size();
    let _ = WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::App(page.into()))
        .title(title)
        .inner_size(w, h)
        .position((screen_w - w) / 2.0, (screen_h - h) / 2.0)
        .resizable(true)
        .decorations(true)
        .always_on_top(true)
        .visible(true)
        .build();
}

// ─── App settings & diagnostics ─────────────────────────────────────────────

#[tauri::command]
pub fn get_app_settings(store: State<crate::app_settings::AppSettingsStore>) -> crate::app_settings::AppSettings {
    store.get()
}

#[tauri::command]
pub fn save_app_settings(
    app: AppHandle,
    store: State<crate::app_settings::AppSettingsStore>,
    settings: crate::app_settings::AppSettings,
) {
    let old_language = store.get().language;
    store.update(settings);
    crate::panel::hover_detector::sync_monitors(&app);
    crate::lifecycle::ensure_maintenance(&app);
    let language = store.get().language;
    if language != old_language {
        apply_language(&app, &language);
    }
}

/// Switch the interface language everywhere: Rust strings, menu bar, open windows.
fn apply_language(app: &AppHandle, language: &str) {
    crate::i18n::set_language(language);
    crate::tray::refresh_menu(app);
    for (label, window) in app.webview_windows() {
        if let Some(title) = window_title(&label) {
            let _ = window.set_title(title);
        }
    }
    let _ = app.emit("language-changed", language.to_string());
}

/// Localized native title for the app's own windows (None = leave as is).
pub fn window_title(label: &str) -> Option<&'static str> {
    use crate::i18n::tr;
    match label {
        "settings-window" => Some(tr("Peekabrowser 設定", "Peekabrowser Settings")),
        "records-window" => Some(tr("Peekabrowser 紀錄", "Peekabrowser Records")),
        "system-config" => Some(tr("快速建立", "Quick Create")),
        _ => None,
    }
}

/// Actual "Launch at login" state, read from the system (the LaunchAgent), not a stored flag.
#[tauri::command]
pub fn get_autostart(app: AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

/// Enable/disable launch at login; returns the state read back from the system.
#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    let launcher = app.autolaunch();
    if enabled {
        launcher.enable().map_err(|e| e.to_string())?;
    } else {
        launcher.disable().map_err(|e| e.to_string())?;
    }
    launcher.is_enabled().map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct Diagnostics {
    /// In-flight user work holding an App Nap activity token (0 = none held).
    activity_work: usize,
    loaded_pages: usize,
    total_pages: usize,
    system_glass: bool,
    accessibility_trusted: bool,
}

#[tauri::command]
pub fn get_diagnostics(tab_manager: State<std::sync::Mutex<WebViewTabManager>>) -> Diagnostics {
    let (loaded, total) = tab_manager
        .lock()
        .map(|m| (m.live_slot_count(), m.pages.len()))
        .unwrap_or((0, 0));
    Diagnostics {
        activity_work: crate::activity::active_count(),
        loaded_pages: loaded,
        total_pages: total,
        system_glass: crate::native::has_system_glass(),
        accessibility_trusted: crate::native::accessibility_trusted(),
    }
}

#[tauri::command]
pub fn get_material_kind() -> String {
    crate::panel::MATERIAL_KIND.lock().map(|g| g.to_string()).unwrap_or_default()
}

// ─── Updates ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
pub fn get_update_status() -> crate::updater::UpdateStatus {
    crate::updater::status()
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<crate::updater::UpdateInfo, String> {
    crate::updater::check(&app).map_err(|e| {
        log::warn!("updater: {}", e);
        e
    })
}

/// Downloads, verifies and installs; the app quits and relaunches on success.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    crate::updater::install(&app)
}

#[tauri::command]
pub fn open_release_page() -> Result<(), String> {
    let url = crate::updater::status()
        .info
        .map(|i| i.page_url)
        .filter(|u| u.starts_with("https://github.com/"))
        .unwrap_or_else(|| format!("https://github.com/{}/releases/latest", crate::updater::REPO));
    std::process::Command::new("open").arg(url).spawn().map_err(|e| e.to_string())?;
    Ok(())
}
