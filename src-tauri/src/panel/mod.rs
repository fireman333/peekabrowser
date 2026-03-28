pub mod hover_detector;

use std::sync::Mutex;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_nspanel::{ManagerExt, WebviewWindowExt};

/// Remembered viewer width — persists across show/hide cycles
static CURRENT_VIEWER_WIDTH: Mutex<Option<f64>> = Mutex::new(None);
/// Remembered height ratio — persists across show/hide cycles
static CURRENT_HEIGHT_RATIO: Mutex<Option<f64>> = Mutex::new(None);

/// Active page viewer label — the one currently shown
static ACTIVE_PAGE_LABEL: Mutex<Option<String>> = Mutex::new(None);
/// All page viewer labels — for hiding all at once
static ALL_PAGE_LABELS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Current screen origin (top-left coords) where the panel is displayed
static CURRENT_SCREEN_X: Mutex<f64> = Mutex::new(0.0);
static CURRENT_SCREEN_Y: Mutex<f64> = Mutex::new(0.0);
static CURRENT_SCREEN_W: Mutex<f64> = Mutex::new(1440.0);
static CURRENT_SCREEN_H: Mutex<f64> = Mutex::new(900.0);

/// Safari user agent — matches the real WKWebView engine (AppleWebKit/605.1.15).
/// Using Chrome UA causes Google to detect a UA/engine mismatch and block login.
pub const CHROME_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.3 Safari/605.1.15";

/// Initialization script injected into every page viewer.
/// - Hides Tauri/WKWebView fingerprints that Google uses to detect embedded browsers
/// - Intercepts window.open() so OAuth popups navigate in-place
const BROWSER_COMPAT_SCRIPT: &str = r#"
(function() {
    // 1. Hide Tauri's WKWebView IPC fingerprint.
    //    Google checks window.webkit.messageHandlers to detect embedded WebViews.
    //    Save a private ref for Tauri, then make it invisible to page scripts.
    if (window.webkit && window.webkit.messageHandlers) {
        try {
            var _tauriHandlers = window.webkit.messageHandlers;
            Object.defineProperty(window.webkit, 'messageHandlers', {
                get: function() { return _tauriHandlers; },
                enumerable: false,
                configurable: true
            });
        } catch(e) {}
    }

    // 2. Intercept window.open — WKWebView blocks popups by default.
    //    Skip override on OAuth domain pages (they need native window.open behavior).
    //    On regular pages, redirect popup navigations in-place.
    var _oauthHosts = ['accounts.google.com', 'accounts.google.co.jp', 'accounts.google.co.uk',
        'login.microsoftonline.com', 'appleid.apple.com', 'auth0.com',
        'login.yahoo.com', 'id.apple.com', 'myaccount.google.com',
        'gemini.google.com'];
    var _isOAuthPage = false;
    try {
        for (var d of _oauthHosts) {
            if (location.hostname === d || location.hostname.endsWith('.' + d)) {
                _isOAuthPage = true; break;
            }
        }
    } catch(e) {}

    if (!_isOAuthPage) {
        var _origOpen = window.open;
        window.open = function(url, target, features) {
            if (!url || typeof url !== 'string' || url.length === 0) {
                if (_origOpen) {
                    try { return _origOpen.call(window, url, target, features); } catch(e) {}
                }
                return null;
            }
            // For any URL, try original window.open first
            if (_origOpen) {
                try {
                    var w = _origOpen.call(window, url, target, features);
                    if (w) return w;
                } catch(e) {}
            }
            // Popup blocked — navigate in-place as fallback
            window.location.href = url;
            return window;
        };
    }

    // 3. Override navigator.webdriver (bot detection)
    try {
        Object.defineProperty(navigator, 'webdriver', {
            get: function() { return false; },
            configurable: true
        });
    } catch(e) {}

    // 4. Patch Notification API (some sites check this)
    if (!window.Notification) {
        window.Notification = { permission: 'default', requestPermission: function() { return Promise.resolve('default'); } };
    }

    // 5. Override navigator.plugins to look like Safari (not empty like embedded WKWebView)
    try {
        Object.defineProperty(navigator, 'plugins', {
            get: function() {
                return [
                    { name: 'WebKit built-in PDF', filename: 'WebKit.framework/WebKit', description: 'Portable Document Format' }
                ];
            },
            configurable: true
        });
        Object.defineProperty(navigator, 'mimeTypes', {
            get: function() {
                return [
                    { type: 'application/pdf', suffixes: 'pdf', description: 'Portable Document Format', enabledPlugin: navigator.plugins[0] }
                ];
            },
            configurable: true
        });
    } catch(e) {}

    // 6. Handle Cmd+R (reload) and Cmd+W (close tab) in page viewer
    //    NSPanel is non-activating so the sidebar keydown listener won't fire here.
    document.addEventListener('keydown', function(e) {
        if (e.metaKey && e.key === 'r') {
            e.preventDefault();
            location.reload();
        } else if (e.metaKey && e.key === 'w') {
            e.preventDefault();
            // Use Tauri IPC to close this page viewer
            try {
                var internals = window.__TAURI_INTERNALS__;
                if (internals && internals.invoke) {
                    var label = internals.metadata && internals.metadata.currentWebview
                        ? internals.metadata.currentWebview.label : '';
                    if (label) {
                        internals.invoke('close_page', { pageId: label });
                    }
                }
            } catch(err) { console.log('close_page err', err); }
        }
    });
})();
"#;

/// Get the current viewer width (remembered or default M = 1/2 screen)
pub fn get_viewer_width() -> f64 {
    if let Ok(guard) = CURRENT_VIEWER_WIDTH.lock() {
        if let Some(w) = *guard {
            return w;
        }
    }
    let (screen_w, _) = current_screen_size();
    (screen_w / 2.0).max(300.0) - TAB_BAR_WIDTH
}

/// Set the remembered viewer width
pub fn set_viewer_width_value(w: f64) {
    if let Ok(mut guard) = CURRENT_VIEWER_WIDTH.lock() {
        *guard = Some(w);
    }
}

/// Set the remembered height ratio
pub fn set_height_ratio(r: f64) {
    if let Ok(mut guard) = CURRENT_HEIGHT_RATIO.lock() {
        *guard = Some(r);
    }
}

/// Get the current height ratio (remembered or default)
fn get_height_ratio() -> f64 {
    if let Ok(guard) = CURRENT_HEIGHT_RATIO.lock() {
        if let Some(r) = *guard {
            return r;
        }
    }
    PANEL_HEIGHT_RATIO
}

pub const SIDEBAR_LABEL: &str = "sidebar";
pub const PICKER_LABEL: &str = "picker";

const PICKER_WIDTH: f64 = 210.0;
const PICKER_HEIGHT: f64 = 260.0;

/// Width of the tab bar (sidebar UI panel)
pub const TAB_BAR_WIDTH: f64 = 56.0;
/// Default viewer width (medium)
pub const VIEWER_WIDTH: f64 = 340.0;
/// Total sidebar width used for off-screen positioning
pub const TOTAL_WIDTH: f64 = TAB_BAR_WIDTH + VIEWER_WIDTH;

/// Default panel height ratio (M preset = 70%)
const PANEL_HEIGHT_RATIO: f64 = 0.70;

/// NSPanel style: non-activating floating
const NS_NON_ACTIVATING_PANEL_MASK: i32 = 128;
const NS_FLOATING_WINDOW_LEVEL: i32 = 3;

/// Returns (panel_height, panel_y_from_top) in logical pixels, using current screen
pub fn panel_geometry() -> (f64, f64) {
    let (_, screen_height) = current_screen_size();
    let screen_y = CURRENT_SCREEN_Y.lock().map(|g| *g).unwrap_or(0.0);
    let ratio = get_height_ratio();
    let panel_height = screen_height * ratio;
    let panel_y = screen_y + (screen_height - panel_height) / 2.0;
    (panel_height, panel_y)
}

// ─── Page viewer management ─────────────────────────────────────────────

/// Set the active page label (called when switching pages)
pub fn set_active_page_label(label: &str) {
    if let Ok(mut guard) = ACTIVE_PAGE_LABEL.lock() {
        *guard = Some(label.to_string());
    }
}

/// Get the active page label
pub fn get_active_page_label() -> Option<String> {
    if let Ok(guard) = ACTIVE_PAGE_LABEL.lock() {
        guard.clone()
    } else {
        None
    }
}

/// Register a page label (track it for hide-all)
pub fn register_page_label(label: &str) {
    if let Ok(mut guard) = ALL_PAGE_LABELS.lock() {
        if !guard.contains(&label.to_string()) {
            guard.push(label.to_string());
        }
    }
}

/// Unregister a page label
pub fn unregister_page_label(label: &str) {
    if let Ok(mut guard) = ALL_PAGE_LABELS.lock() {
        guard.retain(|l| l != label);
    }
    // If this was the active page, clear it
    if let Ok(mut guard) = ACTIVE_PAGE_LABEL.lock() {
        if guard.as_deref() == Some(label) {
            *guard = None;
        }
    }
}

/// Create a new page viewer NSPanel with Safari UA
pub fn create_page_panel(app: &AppHandle, label: &str, url: &str) -> tauri::Result<()> {
    let sx = current_screen_x();
    let (panel_height, panel_y) = panel_geometry();
    let viewer_width = get_viewer_width();

    let webview_url = url
        .parse::<url::Url>()
        .map(WebviewUrl::External)
        .unwrap_or(WebviewUrl::App("index.html".into()));

    let viewer = WebviewWindowBuilder::new(app, label, webview_url)
        .title("Peekabrowser Page")
        .user_agent(CHROME_UA)
        .initialization_script(BROWSER_COMPAT_SCRIPT)
        .inner_size(viewer_width, panel_height)
        .position(sx + TAB_BAR_WIDTH, panel_y)
        .decorations(false)
        .transparent(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(true)
        .build()?;

    setup_panel(&viewer)?;
    register_page_label(label);

    log::info!("Created page panel: {} -> {}", label, url);
    Ok(())
}

/// Show a specific page viewer (and hide all others)
pub fn show_page_viewer(app: &AppHandle, label: &str) {
    let sx = current_screen_x();
    let (panel_height, panel_y) = panel_geometry();
    let viewer_width = get_viewer_width();

    // Hide all other page viewers
    if let Ok(guard) = ALL_PAGE_LABELS.lock() {
        for other in guard.iter() {
            if other != label {
                if let Ok(p) = app.get_webview_panel(other) {
                    p.order_out(None);
                }
                if let Some(w) = app.get_webview_window(other) {
                    let _ = w.set_position(tauri::LogicalPosition::new(-9999.0, 0.0));
                }
            }
        }
    }

    // Show the target page viewer
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.set_position(tauri::LogicalPosition::new(sx + TAB_BAR_WIDTH, panel_y));
        let _ = w.set_size(tauri::LogicalSize::new(viewer_width, panel_height));
        let _ = w.show();
    }
    if let Ok(p) = app.get_webview_panel(label) {
        p.show();
    }

    set_active_page_label(label);
}

/// Destroy a page viewer panel.
/// IMPORTANT: We do NOT call w.destroy() because destroying an NSPanel-wrapped
/// WebviewWindow causes a native crash (SIGABRT in WKWebView teardown).
/// Instead, we hide it off-screen and navigate to about:blank to free resources.
pub fn destroy_page_panel(app: &AppHandle, label: &str) {
    unregister_page_label(label);

    // Hide the panel
    if let Ok(p) = app.get_webview_panel(label) {
        p.order_out(None);
    }
    // Move off-screen and blank the webview to release page resources
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.set_position(tauri::LogicalPosition::new(-9999.0, -9999.0));
        let _ = w.set_size(tauri::LogicalSize::new(1.0, 1.0));
        let _ = w.eval("window.stop(); document.documentElement.innerHTML = '';");
        let _ = w.hide();
    }
}

// ─── Sidebar panel creation ─────────────────────────────────────────────

/// Create the sidebar panel (tab bar only, no viewer — viewers are created as pages)
pub fn create_sidebar_panel(app: &AppHandle) -> tauri::Result<()> {
    let (panel_height, panel_y) = panel_geometry();

    let sidebar = WebviewWindowBuilder::new(
        app,
        SIDEBAR_LABEL,
        WebviewUrl::App("index.html".into()),
    )
    .title("Peekabrowser")
    .inner_size(TAB_BAR_WIDTH, panel_height)
    .position(-(TOTAL_WIDTH + 100.0), panel_y)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .build()?;

    setup_panel(&sidebar)?;

    log::info!(
        "Sidebar panel created: tab bar {}px, height {}px at y={}",
        TAB_BAR_WIDTH,
        panel_height,
        panel_y
    );
    Ok(())
}

/// Panel that can become key (accepts keyboard input)
fn setup_panel(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    let panel = window.to_panel()?;
    panel.set_level(NS_FLOATING_WINDOW_LEVEL);
    panel.set_style_mask(0);

    use tauri_nspanel::cocoa::appkit::NSWindowCollectionBehavior;
    panel.set_collection_behaviour(
        NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary,
    );
    Ok(())
}

/// Non-activating panel — used for the picker popup
fn setup_non_activating_panel(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    let panel = window.to_panel()?;
    panel.set_level(NS_FLOATING_WINDOW_LEVEL);
    panel.set_style_mask(NS_NON_ACTIVATING_PANEL_MASK);

    use tauri_nspanel::cocoa::appkit::NSWindowCollectionBehavior;
    panel.set_collection_behaviour(
        NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary,
    );
    Ok(())
}

/// Get the primary screen size
pub fn get_primary_screen_size() -> (f64, f64) {
    #[cfg(target_os = "macos")]
    unsafe {
        use cocoa::appkit::NSScreen;
        use cocoa::base::nil;
        let screen = NSScreen::mainScreen(nil);
        if screen.is_null() {
            return (1440.0, 900.0);
        }
        let frame = NSScreen::frame(screen);
        (frame.size.width, frame.size.height)
    }
    #[cfg(not(target_os = "macos"))]
    {
        (1440.0, 900.0)
    }
}

/// Screen info: origin in top-left global coords + dimensions
#[derive(Clone, Debug)]
pub struct ScreenRect {
    pub x: f64,      // left edge in global top-left coords
    pub y: f64,      // top edge in global top-left coords
    pub width: f64,
    pub height: f64,
}

/// Get all connected screens (origin in top-left global coordinates)
pub fn get_all_screens() -> Vec<ScreenRect> {
    #[cfg(target_os = "macos")]
    unsafe {
        use cocoa::appkit::NSScreen;
        use cocoa::base::nil;
        use objc::{msg_send, sel, sel_impl};

        let screens: *mut objc::runtime::Object = msg_send![objc::runtime::Class::get("NSScreen").unwrap(), screens];
        let count: usize = msg_send![screens, count];
        if count == 0 {
            return vec![ScreenRect { x: 0.0, y: 0.0, width: 1440.0, height: 900.0 }];
        }

        // Get primary screen height for coordinate conversion (bottom-left → top-left)
        let primary: *mut objc::runtime::Object = msg_send![screens, objectAtIndex: 0usize];
        let primary_frame = NSScreen::frame(primary);
        let primary_h = primary_frame.size.height;

        let mut result = Vec::with_capacity(count);
        for i in 0..count {
            let screen: *mut objc::runtime::Object = msg_send![screens, objectAtIndex: i];
            let frame = NSScreen::frame(screen);
            // Convert macOS bottom-left origin to top-left origin
            let top_y = primary_h - frame.origin.y - frame.size.height;
            result.push(ScreenRect {
                x: frame.origin.x,
                y: top_y,
                width: frame.size.width,
                height: frame.size.height,
            });
        }
        result
    }
    #[cfg(not(target_os = "macos"))]
    {
        vec![ScreenRect { x: 0.0, y: 0.0, width: 1440.0, height: 900.0 }]
    }
}

/// Find which screen the cursor is on (using global top-left coords)
pub fn get_screen_at_cursor() -> ScreenRect {
    let (cx, cy) = get_cursor_topleft_pos();
    let screens = get_all_screens();
    for s in &screens {
        if cx >= s.x && cx < s.x + s.width && cy >= s.y && cy < s.y + s.height {
            return s.clone();
        }
    }
    // Fallback: primary screen (first)
    screens.into_iter().next().unwrap_or(ScreenRect { x: 0.0, y: 0.0, width: 1440.0, height: 900.0 })
}

/// Update the stored current screen to the one the cursor is on
pub fn update_current_screen() {
    let screen = get_screen_at_cursor();
    if let Ok(mut g) = CURRENT_SCREEN_X.lock() { *g = screen.x; }
    if let Ok(mut g) = CURRENT_SCREEN_Y.lock() { *g = screen.y; }
    if let Ok(mut g) = CURRENT_SCREEN_W.lock() { *g = screen.width; }
    if let Ok(mut g) = CURRENT_SCREEN_H.lock() { *g = screen.height; }
}

/// Get the stored current screen origin X
pub fn current_screen_x() -> f64 {
    CURRENT_SCREEN_X.lock().map(|g| *g).unwrap_or(0.0)
}

/// Get stored current screen dimensions
pub fn current_screen_size() -> (f64, f64) {
    let w = CURRENT_SCREEN_W.lock().map(|g| *g).unwrap_or(1440.0);
    let h = CURRENT_SCREEN_H.lock().map(|g| *g).unwrap_or(900.0);
    (w, h)
}

/// Show sidebar + active page viewer on the screen where the cursor is
pub fn show_panel(app: &AppHandle) {
    hover_detector::mark_manual_show();
    update_current_screen();
    let sx = current_screen_x();
    let (panel_height, panel_y) = panel_geometry();
    let viewer_width = get_viewer_width();

    // Show sidebar on current screen's left edge
    if let Some(w) = app.get_webview_window(SIDEBAR_LABEL) {
        let _ = w.set_position(tauri::LogicalPosition::new(sx, panel_y));
        let _ = w.set_size(tauri::LogicalSize::new(TAB_BAR_WIDTH, panel_height));
        let _ = w.show();
    }
    if let Ok(p) = app.get_webview_panel(SIDEBAR_LABEL) {
        p.show();
    }

    // Show active page viewer (if any)
    if let Some(label) = get_active_page_label() {
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.set_position(tauri::LogicalPosition::new(sx + TAB_BAR_WIDTH, panel_y));
            let _ = w.set_size(tauri::LogicalSize::new(viewer_width, panel_height));
            let _ = w.show();
        }
        if let Ok(p) = app.get_webview_panel(&label) {
            p.show();
        }
    }
}

/// Hide sidebar + all page viewers
pub fn hide_panel(app: &AppHandle) {
    // Hide sidebar
    if let Ok(p) = app.get_webview_panel(SIDEBAR_LABEL) {
        p.order_out(None);
    }
    if let Some(w) = app.get_webview_window(SIDEBAR_LABEL) {
        let _ = w.set_position(tauri::LogicalPosition::new(-9999.0, 0.0));
    }

    // Hide all page viewers
    if let Ok(guard) = ALL_PAGE_LABELS.lock() {
        for label in guard.iter() {
            if let Ok(p) = app.get_webview_panel(label) {
                p.order_out(None);
            }
            if let Some(w) = app.get_webview_window(label) {
                let _ = w.set_position(tauri::LogicalPosition::new(-9999.0, 0.0));
            }
        }
    }
}

/// Toggle panels
pub fn toggle_panel(app: &AppHandle) {
    if is_panel_visible(app) {
        hide_panel(app);
    } else {
        show_panel(app);
    }
}

/// Check if sidebar is visible (not hidden off-screen)
pub fn is_panel_visible(app: &AppHandle) -> bool {
    if let Some(w) = app.get_webview_window(SIDEBAR_LABEL) {
        if let Ok(pos) = w.outer_position() {
            // Hidden panels are at x=-9999; visible ones are at a real screen position
            return pos.x > -5000;
        }
        return w.is_visible().unwrap_or(false);
    }
    false
}

/// Navigate the active page viewer to a URL
pub fn navigate_active_viewer(app: &AppHandle, url: &str) -> tauri::Result<()> {
    if let Some(label) = get_active_page_label() {
        if let Some(viewer) = app.get_webview_window(&label) {
            let parsed_url = url.parse::<url::Url>().map_err(|_| tauri::Error::AssetNotFound(url.to_string()))?;
            viewer.navigate(parsed_url)?;
        }
    }
    Ok(())
}

/// Create the floating destination picker popup
pub fn create_picker_panel(app: &AppHandle) -> tauri::Result<()> {
    let picker = WebviewWindowBuilder::new(
        app,
        PICKER_LABEL,
        WebviewUrl::App("picker.html".into()),
    )
    .title("Peekabrowser Picker")
    .inner_size(PICKER_WIDTH, PICKER_HEIGHT)
    .position(-3000.0, -3000.0)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .build()?;

    setup_non_activating_panel(&picker)?;
    Ok(())
}

/// Show the picker popup near cursor (works on any screen)
pub fn show_picker(app: &AppHandle, cursor_x: f64, cursor_y: f64) {
    use tauri::Emitter;
    // Find which screen the cursor is on for correct boundary clamping
    let screens = get_all_screens();
    let cursor_screen = screens.iter()
        .find(|s| cursor_x >= s.x && cursor_x < s.x + s.width
                && cursor_y >= s.y && cursor_y < s.y + s.height)
        .cloned()
        .unwrap_or(ScreenRect { x: 0.0, y: 0.0, width: 1440.0, height: 900.0 });

    let screen_right = cursor_screen.x + cursor_screen.width;
    let screen_top = cursor_screen.y;
    let screen_bottom = cursor_screen.y + cursor_screen.height;

    let x = if cursor_x + PICKER_WIDTH + 20.0 > screen_right {
        cursor_x - PICKER_WIDTH - 10.0
    } else {
        cursor_x + 14.0
    };
    let y = (cursor_y - PICKER_HEIGHT / 2.0)
        .max(screen_top + 10.0)
        .min(screen_bottom - PICKER_HEIGHT - 10.0);

    if let Some(w) = app.get_webview_window(PICKER_LABEL) {
        let _ = w.set_position(tauri::LogicalPosition::new(x, y));
        let _ = w.show();
        let w2 = w.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(120));
            let _ = w2.emit("show-picker", ());
        });
    }
    if let Ok(p) = app.get_webview_panel(PICKER_LABEL) {
        p.show();
    }
}

/// Hide the picker popup
pub fn hide_picker(app: &AppHandle) {
    if let Ok(p) = app.get_webview_panel(PICKER_LABEL) {
        p.order_out(None);
    }
    if let Some(w) = app.get_webview_window(PICKER_LABEL) {
        let _ = w.hide();
        let _ = w.set_position(tauri::LogicalPosition::new(-3000.0, -3000.0));
    }
}

/// Get cursor position in top-left global screen coordinates
pub fn get_cursor_topleft_pos() -> (f64, f64) {
    #[cfg(target_os = "macos")]
    unsafe {
        use cocoa::appkit::NSScreen;
        use cocoa::base::nil;
        use cocoa::foundation::NSPoint;
        use objc::{msg_send, sel, sel_impl};
        let cls = objc::runtime::Class::get("NSEvent").unwrap();
        let location: NSPoint = msg_send![cls, mouseLocation];
        // Use primary screen height for coordinate conversion
        let screen = NSScreen::mainScreen(nil);
        let primary_h = if screen.is_null() {
            900.0
        } else {
            NSScreen::frame(screen).size.height
        };
        (location.x, primary_h - location.y)
    }
    #[cfg(not(target_os = "macos"))]
    {
        (400.0, 300.0)
    }
}

/// Resize both sidebar and active page viewer (width + height)
pub fn resize_panels(app: &AppHandle, viewer_width: f64) {
    let sx = current_screen_x();
    let (panel_height, panel_y) = panel_geometry();

    if let Some(w) = app.get_webview_window(SIDEBAR_LABEL) {
        let _ = w.set_size(tauri::LogicalSize::new(TAB_BAR_WIDTH, panel_height));
        let _ = w.set_position(tauri::LogicalPosition::new(sx, panel_y));
    }
    if let Some(label) = get_active_page_label() {
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.set_size(tauri::LogicalSize::new(viewer_width, panel_height));
            let _ = w.set_position(tauri::LogicalPosition::new(sx + TAB_BAR_WIDTH, panel_y));
        }
    }
}
