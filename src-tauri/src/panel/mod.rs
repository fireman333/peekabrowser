pub mod hover_detector;
pub mod lookup_highlight;

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

/// Pool of free viewer slots (windows on about:blank), reused before any new
/// window is created. See `lifecycle` for the slot cap.
static RECYCLED_POOL: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Slot label -> generation. Bumped every time a slot is (re)assigned or
/// recycled, so background work for a previous page can detect staleness.
static SLOT_GENERATIONS: Mutex<Option<std::collections::HashMap<String, u64>>> = Mutex::new(None);
static NEXT_SLOT_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Current screen origin (top-left coords) where the panel is displayed
static CURRENT_SCREEN_X: Mutex<f64> = Mutex::new(0.0);
static CURRENT_SCREEN_Y: Mutex<f64> = Mutex::new(0.0);
static CURRENT_SCREEN_W: Mutex<f64> = Mutex::new(1440.0);
static CURRENT_SCREEN_H: Mutex<f64> = Mutex::new(900.0);

/// Actual panel bounds (set in show_panel_inner, read by hover_detector)
/// These represent the real position where the panel was placed.
static PANEL_BOUNDS_LEFT: Mutex<f64> = Mutex::new(0.0);
static PANEL_BOUNDS_TOP: Mutex<f64> = Mutex::new(0.0);
static PANEL_BOUNDS_RIGHT: Mutex<f64> = Mutex::new(0.0);
static PANEL_BOUNDS_BOTTOM: Mutex<f64> = Mutex::new(0.0);

/// Safari user agent — matches the real WKWebView engine (AppleWebKit/605.1.15).
/// Using Chrome UA causes Google to detect a UA/engine mismatch and block login.
pub const CHROME_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.3 Safari/605.1.15";

/// Initialization script injected into every page viewer.
/// - Hides Tauri/WKWebView fingerprints that Google uses to detect embedded browsers
/// - Intercepts window.open() so OAuth popups navigate in-place
/// - On Gemini pages: SKIP browser environment modifications to avoid breaking
///   the streaming channel (Gemini detects overrides and drops SSE connections)
const BROWSER_COMPAT_SCRIPT: &str = r#"
(function() {
    var _isGemini = false;
    try { _isGemini = location.hostname.includes('gemini.google.com'); } catch(e) {}

    // ─── Keyboard shortcuts (always active) ───
    function _peekInvoke(cmd, args) {
        try {
            var internals = window.__TAURI_INTERNALS__;
            if (internals && internals.invoke) internals.invoke(cmd, args || {});
        } catch(err) {}
    }
    document.addEventListener('keydown', function(e) {
        var _k = (e.key || '').toLowerCase();
        // Tab model: ⌘T new tab, ⌘⇧T reopen closed tab, ⌘⇧[ / ⌘⇧] previous / next tab.
        if (e.metaKey && e.shiftKey && !e.altKey && !e.ctrlKey) {
            if (_k === 't') {
                e.preventDefault();
                _peekInvoke('reopen_closed_tab');
            } else if (e.code === 'BracketLeft') {
                e.preventDefault();
                _peekInvoke('cycle_tab', { delta: -1 });
            } else if (e.code === 'BracketRight') {
                e.preventDefault();
                _peekInvoke('cycle_tab', { delta: 1 });
            }
            return;
        }
        if (e.metaKey && !e.altKey && !e.ctrlKey && _k === 't') {
            e.preventDefault();
            _peekInvoke('new_tab_for_active');
            return;
        }
        if (e.metaKey && e.key === 'r') {
            e.preventDefault();
            location.reload();
        } else if (e.metaKey && e.key === 'w') {
            e.preventDefault();
            try {
                var internals = window.__TAURI_INTERNALS__;
                if (internals && internals.invoke) {
                    var label = internals.metadata && internals.metadata.currentWebview
                        ? internals.metadata.currentWebview.label : '';
                    if (label) internals.invoke('close_page', { pageId: label });
                }
            } catch(err) {}
        } else if (e.metaKey && e.key === '[') {
            e.preventDefault();
            history.back();
        } else if (e.metaKey && e.key === ']') {
            e.preventDefault();
            history.forward();
        } else if (e.metaKey && e.key === 'n') {
            e.preventDefault();
            try {
                var internals = window.__TAURI_INTERNALS__;
                if (internals && internals.invoke) {
                    internals.invoke('new_tab_for_active', {});
                }
            } catch(err) {}
        }
    });

    // ─── On Gemini: hide embedded browser fingerprints ───
    if (_isGemini) {
        // 1. Hide __TAURI_INTERNALS__ — Gemini may detect this foreign object
        //    and degrade streaming. Keep a private ref for our keyboard shortcuts.
        try {
            var _tauriRef = window.__TAURI_INTERNALS__;
            if (_tauriRef) {
                Object.defineProperty(window, '__TAURI_INTERNALS__', {
                    get: function() { return _tauriRef; },
                    enumerable: false,
                    configurable: true
                });
            }
        } catch(e) {}

        // 2. Hide webkit.messageHandlers (WKWebView detection)
        if (window.webkit && window.webkit.messageHandlers) {
            try {
                var _handlers = window.webkit.messageHandlers;
                Object.defineProperty(window.webkit, 'messageHandlers', {
                    get: function() { return _handlers; },
                    enumerable: false,
                    configurable: true
                });
            } catch(e) {}
        }

        // No resident health check / keep-alive / auto-stop here: generation is
        // watched only while a Peekabrowser query is running (see delivery.rs),
        // and long reasoning is never auto-stopped or reloaded.
        return;
    }

    // 1. Hide Tauri's WKWebView IPC fingerprint.
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
    var _oauthHosts = ['accounts.google.com', 'accounts.google.co.jp', 'accounts.google.co.uk',
        'login.microsoftonline.com', 'appleid.apple.com', 'auth0.com',
        'login.yahoo.com', 'id.apple.com', 'myaccount.google.com'];
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
            if (_origOpen) {
                try {
                    var w = _origOpen.call(window, url, target, features);
                    if (w) return w;
                } catch(e) {}
            }
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

    // 4. Patch Notification API
    if (!window.Notification) {
        window.Notification = { permission: 'default', requestPermission: function() { return Promise.resolve('default'); } };
    }

    // 5. Override navigator.plugins to look like Safari
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
})();
"#;

/// Run before a slot goes back to the pool: stop loading and release media.
/// Navigating to about:blank afterwards frees the page itself. Site storage
/// (service workers, Cache API, cookies) is shared with other pages of the
/// same origin and is deliberately left alone.
const RELEASE_SCRIPT: &str = r#"
(function() {
    try { window.stop(); } catch(e) {}
    try {
        document.querySelectorAll('video, audio').forEach(function(el) {
            el.pause(); el.removeAttribute('src'); el.load();
        });
    } catch(e) {}
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

/// Corner radius matching recent macOS window corners (continuous curve).
/// Sidebar + viewer read as one rounded panel; the picker is rounded all round.
pub const PANEL_CORNER_RADIUS: f64 = 18.0;
pub const PICKER_CORNER_RADIUS: f64 = 16.0;

/// Sidebar alone → all corners; with a viewer beside it → only its left corners.
pub fn update_sidebar_corners(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(SIDEBAR_LABEL) {
        let mask = if get_active_page_label().is_some() {
            crate::native::corners::LEFT
        } else {
            crate::native::corners::ALL
        };
        crate::native::set_rounded_corners(&w, PANEL_CORNER_RADIUS, mask);
    }
}
pub const PICKER_LABEL: &str = "picker";

// ─── Top chrome (address bar, navigation, tab row) ──────────────────────
//
// A separate NSPanel above the active page window, same x and width. The
// page window moves down and shrinks by the chrome's height, so no web
// content is covered. Every path that shows, hides, moves or resizes the
// page also lays out the chrome (`sync_chrome`).

pub const CHROME_LABEL: &str = "chrome";
/// Row height (address capsule / nav cluster / tab row).
const CHROME_ROW: f64 = 44.0;
const CHROME_GAP: f64 = 6.0;
const CHROME_MARGIN: f64 = 8.0;

/// Whether the tab row (second chrome row) is shown.
static CHROME_TAB_ROW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// While set, chrome layout is frozen (a tab close is swapping pages, so the
/// brief "no active page" moment must not hide the chrome).
static CHROME_HOLD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn hold_chrome(hold: bool) {
    CHROME_HOLD.store(hold, std::sync::atomic::Ordering::SeqCst);
}

/// Whether the chrome panel is currently ordered in.
static CHROME_SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Viewer slots currently loading (page-load Started without Finished).
static LOADING_SLOTS: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Per slot: token of its latest page-load Started (tokens only grow).
static LOAD_TOKENS: Mutex<Option<std::collections::HashMap<String, u64>>> = Mutex::new(None);
static NEXT_LOAD_TOKEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
/// Last chosen width preset ("short" | "medium" | "long").
static WIDTH_PRESET: Mutex<&'static str> = Mutex::new("medium");

pub fn chrome_height() -> f64 {
    let rows = if CHROME_TAB_ROW.load(std::sync::atomic::Ordering::SeqCst) {
        2.0 * CHROME_ROW + CHROME_GAP
    } else {
        CHROME_ROW
    };
    rows + 2.0 * CHROME_MARGIN
}

pub fn set_width_preset(preset: &str) {
    let p = match preset {
        "short" => "short",
        "long" => "long",
        "custom" => "custom",
        _ => "medium",
    };
    if let Ok(mut g) = WIDTH_PRESET.lock() {
        *g = p;
    }
}

pub fn width_preset() -> &'static str {
    WIDTH_PRESET.lock().map(|g| *g).unwrap_or("medium")
}

pub fn set_slot_loading(label: &str, loading: bool) {
    if let Ok(mut g) = LOADING_SLOTS.lock() {
        g.retain(|l| l != label);
        if loading {
            g.push(label.to_string());
        }
    }
}

pub fn is_slot_loading(label: &str) -> bool {
    LOADING_SLOTS.lock().map(|g| g.iter().any(|l| l == label)).unwrap_or(false)
}

/// A page load started in `label`: mark it loading and return a token that
/// identifies this load (see [`clear_stale_loading`]).
pub fn slot_load_started(label: &str) -> u64 {
    let token = NEXT_LOAD_TOKEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    if let Ok(mut g) = LOAD_TOKENS.lock() {
        g.get_or_insert_with(std::collections::HashMap::new).insert(label.to_string(), token);
    }
    set_slot_loading(label, true);
    token
}

/// Token of the latest load started in `label` (0 if none yet).
pub fn last_load_token(label: &str) -> u64 {
    LOAD_TOKENS.lock().ok().and_then(|g| g.as_ref()?.get(label).copied()).unwrap_or(0)
}

/// wry reports no load failures, so a load that never finishes would keep
/// the loading line forever. Clears it if `token` is still the latest load
/// of `label` and it is still loading; returns whether it did.
pub fn clear_stale_loading(label: &str, token: u64) -> bool {
    if last_load_token(label) != token || !is_slot_loading(label) {
        return false;
    }
    set_slot_loading(label, false);
    true
}

/// Frame of the active page window: right of the rail, below the chrome.
fn page_frame() -> (f64, f64, f64, f64) {
    let sx = current_screen_x();
    let (panel_height, panel_y) = panel_geometry();
    let ch = chrome_height();
    (sx + TAB_BAR_WIDTH, panel_y + ch, get_viewer_width(), (panel_height - ch).max(100.0))
}

/// Set the tab-row flag without relaying out, before a page is shown (so the
/// page is placed at its final frame at once).
pub fn prime_chrome_tab_row(show: bool) {
    CHROME_TAB_ROW.store(show, std::sync::atomic::Ordering::SeqCst);
}

/// Show or hide the tab row. May be called from any thread with the tab
/// manager locked, so the relayout is queued on the main thread.
pub fn set_chrome_tab_row(app: &AppHandle, show: bool) {
    let was = CHROME_TAB_ROW.swap(show, std::sync::atomic::Ordering::SeqCst);
    if was != show {
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || {
            sync_chrome(&app2);
            if let Some(label) = get_active_page_label() {
                if let Some(w) = app2.get_webview_window(&label) {
                    let (x, y, vw, h) = page_frame();
                    let _ = w.set_position(tauri::LogicalPosition::new(x, y));
                    let _ = w.set_size(tauri::LogicalSize::new(vw, h));
                }
            }
        });
    }
}

/// Place the chrome above the active page and order it in, or order it out
/// when the panel is hidden or no page is shown. Never takes key status, so
/// keyboard focus stays in the page.
pub fn sync_chrome(app: &AppHandle) {
    use std::sync::atomic::Ordering;
    if CHROME_HOLD.load(Ordering::SeqCst) {
        return;
    }
    let want = is_panel_visible(app) && get_active_page_label().is_some();
    if want {
        let sx = current_screen_x();
        let (_, panel_y) = panel_geometry();
        // The cached flag can go stale (AppKit may order the panel out on its
        // own, e.g. while the key page window is hidden during ⌘W), so also
        // trust the window's real visibility.
        let mut visible = false;
        if let Some(w) = app.get_webview_window(CHROME_LABEL) {
            let _ = w.set_position(tauri::LogicalPosition::new(sx + TAB_BAR_WIDTH, panel_y));
            let _ = w.set_size(tauri::LogicalSize::new(get_viewer_width(), chrome_height()));
            visible = w.is_visible().unwrap_or(false);
        }
        let was_shown = CHROME_SHOWN.swap(true, Ordering::SeqCst);
        if !was_shown || !visible {
            if was_shown {
                log::info!("chrome: panel was ordered out behind our back; ordering it in again");
            }
            if let Ok(p) = app.get_webview_panel(CHROME_LABEL) {
                p.order_front_regardless();
            }
        }
        sync_grip(app, true);
    } else {
        if CHROME_SHOWN.swap(false, Ordering::SeqCst) {
            if let Ok(p) = app.get_webview_panel(CHROME_LABEL) {
                p.order_out(None);
            }
        }
        sync_grip(app, false);
    }
}

// ─── Resize grip (iPadOS-style arc on the bottom-right corner) ──────────

pub const GRIP_LABEL: &str = "grip";
/// Grip window size; the page corner sits GRIP_INSET points into it, so the
/// arc hugs the corner from outside.
const GRIP_SIZE: f64 = 36.0;
const GRIP_INSET: f64 = 22.0;
const MIN_VIEWER_WIDTH: f64 = 320.0;
const MIN_PANEL_HEIGHT: f64 = 320.0;

/// Top of the panel as a fraction of the screen height once the user has
/// dragged the grip (None = vertically centred, as the S/M/L presets are).
static CURRENT_TOP_RATIO: Mutex<Option<f64>> = Mutex::new(None);
/// (viewer width, panel height, panel top) when the grip drag began.
static GRIP_DRAG_START: Mutex<Option<(f64, f64, f64)>> = Mutex::new(None);

pub fn set_top_ratio(r: Option<f64>) {
    if let Ok(mut g) = CURRENT_TOP_RATIO.lock() {
        *g = r;
    }
}

fn top_ratio() -> Option<f64> {
    CURRENT_TOP_RATIO.lock().ok().and_then(|g| *g)
}

fn sync_grip(app: &AppHandle, show: bool) {
    let Ok(p) = app.get_webview_panel(GRIP_LABEL) else { return };
    if !show {
        p.order_out(None);
        return;
    }
    let (px, _, vw, _) = page_frame();
    let (panel_height, panel_y) = panel_geometry();
    if let Some(w) = app.get_webview_window(GRIP_LABEL) {
        let _ = w.set_position(tauri::LogicalPosition::new(
            px + vw - GRIP_INSET,
            panel_y + panel_height - GRIP_INSET,
        ));
        if !w.is_visible().unwrap_or(false) {
            p.order_front_regardless();
        }
    }
}

/// Cursor within this distance of the page's bottom-right corner reveals the
/// grip (iPadOS shows its arc only when you reach for the corner).
const GRIP_REVEAL_DIST: f64 = 44.0;
static GRIP_NEAR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn grip_is_near(cursor: (f64, f64), corner: (f64, f64)) -> bool {
    (cursor.0 - corner.0).abs() <= GRIP_REVEAL_DIST && (cursor.1 - corner.1).abs() <= GRIP_REVEAL_DIST
}

/// Called with every cursor observation while the panel is shown; tells the
/// grip to fade its arc in or out when the cursor crosses the reveal zone.
/// A drag keeps it shown.
pub fn note_cursor_for_grip(app: &AppHandle, cx: f64, cy: f64) {
    use std::sync::atomic::Ordering;
    use tauri::Emitter;
    let (_, _, right, bottom) = panel_bounds();
    let dragging = GRIP_DRAG_START.lock().map(|g| g.is_some()).unwrap_or(false);
    let near = dragging || (get_active_page_label().is_some() && grip_is_near((cx, cy), (right, bottom)));
    if GRIP_NEAR.swap(near, Ordering::SeqCst) != near {
        let _ = app.emit_to(GRIP_LABEL, "grip-near", near);
    }
}

/// Create the hidden resize grip panel (called once at launch).
pub fn create_grip_panel(app: &AppHandle) -> tauri::Result<()> {
    let w = WebviewWindowBuilder::new(app, GRIP_LABEL, WebviewUrl::App("grip.html".into()))
        .title("Peekabrowser Resize")
        .inner_size(GRIP_SIZE, GRIP_SIZE)
        .position(-3000.0, -3000.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .build()?;
    setup_non_activating_panel(&w)?;
    // Above the page windows (same floating level otherwise).
    if let Ok(p) = app.get_webview_panel(GRIP_LABEL) {
        p.set_level(NS_FLOATING_WINDOW_LEVEL + 1);
    }
    Ok(())
}

/// Grip drag began: remember the frame it is measured from.
pub fn grip_drag_start() {
    let (h, y) = panel_geometry();
    if let Ok(mut g) = GRIP_DRAG_START.lock() {
        *g = Some((get_viewer_width(), h, y));
    }
}

pub fn grip_drag_end() {
    if let Ok(mut g) = GRIP_DRAG_START.lock() {
        *g = None;
    }
}

/// New (viewer width, panel height) for a drag of (dx, dy) from `start`,
/// keeping the top-left fixed and the panel on screen. Pure, for tests.
pub fn grip_size_for(
    start: (f64, f64, f64),
    dx: f64,
    dy: f64,
    screen: (f64, f64, f64, f64),
) -> (f64, f64) {
    let (w0, h0, top) = start;
    let (sx, sy, sw, sh) = screen;
    let max_w = (sx + sw - (sx + TAB_BAR_WIDTH) - 8.0).max(MIN_VIEWER_WIDTH);
    let max_h = (sy + sh - top - 4.0).max(MIN_PANEL_HEIGHT);
    ((w0 + dx).clamp(MIN_VIEWER_WIDTH, max_w), (h0 + dy).clamp(MIN_PANEL_HEIGHT, max_h))
}

/// Grip moved by (dx, dy) points since the drag began.
pub fn grip_drag_move(app: &AppHandle, dx: f64, dy: f64) {
    let Some(start) = GRIP_DRAG_START.lock().ok().and_then(|g| *g) else { return };
    let (sw, sh) = current_screen_size();
    let sx = current_screen_x();
    let sy = CURRENT_SCREEN_Y.lock().map(|g| *g).unwrap_or(0.0);
    let (w, h) = grip_size_for(start, dx, dy, (sx, sy, sw, sh));
    set_width_preset("custom");
    set_viewer_width_value(w);
    set_height_ratio(h / sh);
    set_top_ratio(Some((start.2 - sy) / sh));
    resize_panels(app, w);
}

/// Create the top chrome panel (hidden until a page is shown).
pub fn create_chrome_panel(app: &AppHandle) -> tauri::Result<()> {
    let chrome = WebviewWindowBuilder::new(app, CHROME_LABEL, WebviewUrl::App("chrome.html".into()))
        .title("Peekabrowser Toolbar")
        .inner_size(VIEWER_WIDTH, chrome_height())
        .position(-(TOTAL_WIDTH + 100.0), 0.0)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build()?;
    setup_panel(&chrome)?;
    apply_material(app, &chrome, 0.0);
    crate::native::set_rounded_corners(&chrome, PANEL_CORNER_RADIUS, crate::native::corners::TOP_RIGHT);
    Ok(())
}

const PICKER_WIDTH: f64 = 248.0;
const PICKER_HEIGHT: f64 = 420.0;

/// Width of the rail (sidebar UI panel), iPad "medium" rail.
pub const TAB_BAR_WIDTH: f64 = 70.0;
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
    let panel_y = match top_ratio() {
        Some(t) => screen_y + (t * screen_height).clamp(0.0, (screen_height - panel_height).max(0.0)),
        None => screen_y + (screen_height - panel_height) / 2.0,
    };
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
    if let Ok(mut guard) = ACTIVE_PAGE_LABEL.lock() {
        if guard.as_deref() == Some(label) {
            *guard = None;
        }
    }
}

fn bump_slot_generation(label: &str) -> u64 {
    let g = NEXT_SLOT_GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    if let Ok(mut guard) = SLOT_GENERATIONS.lock() {
        guard.get_or_insert_with(std::collections::HashMap::new).insert(label.to_string(), g);
    }
    g
}

/// Current generation of a live slot (None once it is recycled).
pub fn slot_generation(label: &str) -> Option<u64> {
    if !ALL_PAGE_LABELS.lock().map(|g| g.iter().any(|l| l == label)).unwrap_or(false) {
        return None;
    }
    SLOT_GENERATIONS.lock().ok()?.as_ref()?.get(label).copied()
}

/// Create a brand-new viewer window (only when no pooled slot is available).
pub fn create_viewer_window(app: &AppHandle, label: &str, url: &str) -> tauri::Result<()> {
    let (x, y, viewer_width, page_height) = page_frame();

    let webview_url = url
        .parse::<url::Url>()
        .map(WebviewUrl::External)
        .unwrap_or(WebviewUrl::App("index.html".into()));

    let viewer = WebviewWindowBuilder::new(app, label, webview_url)
        .title("Peekabrowser Page")
        .user_agent(CHROME_UA)
        .initialization_script(BROWSER_COMPAT_SCRIPT)
        .inner_size(viewer_width, page_height)
        .position(x, y)
        .on_page_load(|w, payload| {
            let loading = matches!(payload.event(), tauri::webview::PageLoadEvent::Started);
            crate::lifecycle::on_slot_load(w.app_handle(), w.label(), loading, payload.url().as_str());
        })
        .decorations(false)
        .transparent(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .devtools(true)
        .build()?;

    setup_panel(&viewer)?;
    // The chrome sits on top, so only the bottom-right corner is rounded.
    crate::native::set_rounded_corners(&viewer, PANEL_CORNER_RADIUS, crate::native::corners::BOTTOM_RIGHT);
    register_page_label(label);
    bump_slot_generation(label);

    log::info!("Created viewer slot: {} -> {}", label, url);
    Ok(())
}

/// Point a pooled slot at a new URL. Showing it is up to `show_page_viewer`.
pub fn navigate_slot(app: &AppHandle, label: &str, url: &str) -> tauri::Result<()> {
    let w = app
        .get_webview_window(label)
        .ok_or_else(|| tauri::Error::AssetNotFound(format!("slot {} missing", label)))?;
    let parsed = url
        .parse::<url::Url>()
        .map_err(|e| tauri::Error::AssetNotFound(format!("Invalid URL: {} ({})", url, e)))?;
    register_page_label(label);
    bump_slot_generation(label);
    crate::native::set_media_suspended(&w, false);
    w.navigate(parsed)?;
    log::info!("Reusing viewer slot '{}' -> {}", label, url);
    Ok(())
}

/// Show a specific page viewer and hide all others.
pub fn show_page_viewer(app: &AppHandle, label: &str) {
    let (x, y, viewer_width, page_height) = page_frame();

    // order_out keeps the WebView in place, so WebKit doesn't drop network
    // connections (e.g. Gemini's stream). Occluded windows get throttled
    // rAF from WebKit itself; we only suspend media via the public API.
    let others: Vec<String> = ALL_PAGE_LABELS
        .lock()
        .map(|g| g.iter().filter(|l| l.as_str() != label).cloned().collect())
        .unwrap_or_default();
    for other in &others {
        if let Ok(p) = app.get_webview_panel(other) {
            p.order_out(None);
        }
        if let Some(w) = app.get_webview_window(other) {
            crate::native::set_media_suspended(&w, true);
        }
    }

    set_active_page_label(label);
    // Chrome first (without key status), then the page, which becomes key.
    sync_chrome(app);
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.set_position(tauri::LogicalPosition::new(x, y));
        let _ = w.set_size(tauri::LogicalSize::new(viewer_width, page_height));
        crate::native::set_media_suspended(&w, false);
        if is_panel_visible(app) {
            let _ = w.show();
        }
    }
    if is_panel_visible(app) {
        if let Ok(p) = app.get_webview_panel(label) {
            p.show();
        }
    }

    update_sidebar_corners(app);
}

/// Return a viewer slot to the pool.
///
/// IMPORTANT: We CANNOT call `w.destroy()` or `w.close()` on NSPanel-wrapped
/// WebviewWindows — doing so triggers a native crash (SIGABRT) in tao's
/// `control_flow_end_handler` during WKWebView teardown. Instead slots are
/// recycled: hidden, media released, navigated to about:blank (which frees
/// the page's DOM/JS heap/connections), and reused by the next page. The
/// lifecycle module caps live slots and always reuses pooled ones first, so
/// the number of native windows stays bounded.
pub fn recycle_slot(app: &AppHandle, label: &str) {
    unregister_page_label(label);
    update_sidebar_corners(app);
    sync_chrome(app);
    set_slot_loading(label, false);
    // Any pending work for the previous page now sees a stale generation.
    bump_slot_generation(label);

    if let Ok(p) = app.get_webview_panel(label) {
        p.order_out(None);
    }
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.hide();
        let _ = w.eval(RELEASE_SCRIPT);
        crate::native::set_media_suspended(&w, true);
        if let Ok(url) = "about:blank".parse::<url::Url>() {
            let _ = w.navigate(url);
        }
    }
    if let Ok(mut pool) = RECYCLED_POOL.lock() {
        if !pool.iter().any(|l| l == label) {
            pool.push(label.to_string());
        }
        log::info!("Recycled slot {} (pool size {})", label, pool.len());
    }
}

/// Pop a free slot label from the pool, if any.
pub fn pop_recycled_label() -> Option<String> {
    RECYCLED_POOL.lock().ok()?.pop()
}

/// Check if a page is in the process of closing.
/// (Always false — windows are never destroyed. Kept for lib.rs on_window_event.)
pub fn is_page_closing(_label: &str) -> bool {
    false
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
    apply_material(app, &sidebar, 0.0);
    crate::native::set_rounded_corners(&sidebar, PANEL_CORNER_RADIUS, crate::native::corners::ALL);

    log::info!(
        "Sidebar panel created: tab bar {}px, height {}px at y={}",
        TAB_BAR_WIDTH,
        panel_height,
        panel_y
    );
    Ok(())
}

/// Native material behind transparent chrome windows: system Liquid Glass on
/// macOS 26+, NSVisualEffectView vibrancy on 13–15 (never both). The page
/// marks which one is active so CSS can drop its solid background.
fn apply_material(app: &AppHandle, window: &tauri::WebviewWindow, radius: f64) {
    let enabled = app
        .try_state::<crate::app_settings::AppSettingsStore>()
        .map(|s| s.get().native_material)
        .unwrap_or(true);
    if !enabled {
        return;
    }
    let kind = if crate::native::install_system_glass(window, radius) {
        "glass"
    } else {
        use tauri::utils::config::WindowEffectsConfig;
        use tauri::window::{Effect, EffectState};
        let effect = if window.label() == PICKER_LABEL { Effect::Popover } else { Effect::Sidebar };
        let cfg = WindowEffectsConfig {
            effects: vec![effect],
            state: Some(EffectState::FollowsWindowActiveState),
            radius: if radius > 0.0 { Some(radius) } else { None },
            color: None,
        };
        match window.set_effects(cfg) {
            Ok(_) => "vibrancy",
            Err(e) => {
                log::warn!("vibrancy unavailable: {}", e);
                return;
            }
        }
    };
    let _ = window.eval(&format!("document.documentElement.dataset.material = '{}';", kind));
    MATERIAL_KIND.lock().map(|mut g| *g = kind).ok();
}

/// "glass", "vibrancy" or "" — re-announced to pages after they load.
pub static MATERIAL_KIND: Mutex<&'static str> = Mutex::new("");

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

/// Record where the panel is (read by the hover detector's auto-hide rule).
fn store_panel_bounds(sx: f64, panel_y: f64, viewer_width: f64, panel_height: f64) {
    if let Ok(mut g) = PANEL_BOUNDS_LEFT.lock() { *g = sx; }
    if let Ok(mut g) = PANEL_BOUNDS_TOP.lock() { *g = panel_y; }
    if let Ok(mut g) = PANEL_BOUNDS_RIGHT.lock() { *g = sx + TAB_BAR_WIDTH + viewer_width; }
    if let Ok(mut g) = PANEL_BOUNDS_BOTTOM.lock() { *g = panel_y + panel_height; }
}

/// Get actual panel bounds (left, top, right, bottom) set by show_panel_inner
/// and resize_panels
pub fn panel_bounds() -> (f64, f64, f64, f64) {
    let left = PANEL_BOUNDS_LEFT.lock().map(|g| *g).unwrap_or(0.0);
    let top = PANEL_BOUNDS_TOP.lock().map(|g| *g).unwrap_or(0.0);
    let right = PANEL_BOUNDS_RIGHT.lock().map(|g| *g).unwrap_or(0.0);
    let bottom = PANEL_BOUNDS_BOTTOM.lock().map(|g| *g).unwrap_or(0.0);
    (left, top, right, bottom)
}

/// Show sidebar from edge hover — does NOT set manual show mode,
/// so hover detector uses simple "cursor leaves → hide" logic.
pub fn show_panel_from_edge(app: &AppHandle) {
    // Don't mark as manual show — edge hover uses normal hide logic
    update_current_screen();
    show_panel_inner(app);
}

/// Show sidebar + active page viewer (manual trigger — sets manual show mode)
pub fn show_panel(app: &AppHandle) {
    hover_detector::mark_manual_show(app);
    update_current_screen();
    show_panel_inner(app);
}

/// Whether the panel is shown. Tracked explicitly instead of inferred from the
/// window position (the sidebar starts at a negative x that looked "visible").
static PANEL_SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Shared panel display logic (used by both show_panel and show_panel_from_edge)
fn show_panel_inner(app: &AppHandle) {
    PANEL_SHOWN.store(true, std::sync::atomic::Ordering::SeqCst);
    hover_detector::note_shown();
    let sx = current_screen_x();
    let (panel_height, panel_y) = panel_geometry();
    let viewer_width = get_viewer_width();
    let (px, py, _, page_height) = page_frame();

    // Store actual panel bounds for hover detector
    store_panel_bounds(sx, panel_y, viewer_width, panel_height);

    // Show sidebar on current screen's left edge
    if let Some(w) = app.get_webview_window(SIDEBAR_LABEL) {
        let _ = w.set_position(tauri::LogicalPosition::new(sx, panel_y));
        let _ = w.set_size(tauri::LogicalSize::new(TAB_BAR_WIDTH, panel_height));
        let _ = w.show();
    }
    if let Ok(p) = app.get_webview_panel(SIDEBAR_LABEL) {
        p.show();
    }

    // Chrome above the page (no key status), then the page itself.
    sync_chrome(app);

    // Show active page viewer (if any) and thaw it
    if let Some(label) = get_active_page_label() {
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.set_position(tauri::LogicalPosition::new(px, py));
            let _ = w.set_size(tauri::LogicalSize::new(viewer_width, page_height));
            let _ = w.show();
            crate::native::set_media_suspended(&w, false);
        }
        if let Ok(p) = app.get_webview_panel(&label) {
            p.show();
        }
    }

    crate::lifecycle::on_panel_shown(app);
    hover_detector::sync_monitors(app);
}

/// Hide sidebar + all page viewers, freezing all tabs to save memory/CPU
pub fn hide_panel(app: &AppHandle) {
    PANEL_SHOWN.store(false, std::sync::atomic::Ordering::SeqCst);
    // Clear manual show state so next show starts fresh
    hover_detector::clear_manual_show();

    // Hide sidebar
    if let Ok(p) = app.get_webview_panel(SIDEBAR_LABEL) {
        p.order_out(None);
    }
    if let Some(w) = app.get_webview_window(SIDEBAR_LABEL) {
        let _ = w.set_position(tauri::LogicalPosition::new(-9999.0, 0.0));
    }
    sync_chrome(app);

    // Hide all page viewers via order_out (don't move off-screen —
    // moving to -9999 causes WebKit to kill streaming connections).
    // Generation keeps running; only media is suspended.
    let labels: Vec<String> = ALL_PAGE_LABELS.lock().map(|g| g.clone()).unwrap_or_default();
    for label in &labels {
        if let Ok(p) = app.get_webview_panel(label) {
            p.order_out(None);
        }
        if let Some(w) = app.get_webview_window(label) {
            crate::native::set_media_suspended(&w, true);
        }
    }

    crate::lifecycle::on_panel_hidden(app);
    hover_detector::sync_monitors(app);
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
pub fn is_panel_visible(_app: &AppHandle) -> bool {
    PANEL_SHOWN.load(std::sync::atomic::Ordering::SeqCst)
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
    apply_material(app, &picker, PICKER_CORNER_RADIUS);
    crate::native::set_rounded_corners(&picker, PICKER_CORNER_RADIUS, crate::native::corners::ALL);
    Ok(())
}

/// Show the picker popup near cursor (works on any screen)
pub fn show_picker(app: &AppHandle, cursor_x: f64, cursor_y: f64) {
    use tauri::Emitter;
    crate::picker_keys::set_picker_visible(true);
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

    *PICKER_ANCHOR.lock().unwrap_or_else(|e| e.into_inner()) = Some((x, y, cursor_screen.clone()));
    if let Some(w) = app.get_webview_window(PICKER_LABEL) {
        // Back to the base height (a previous showing may have expanded prompts).
        let _ = w.set_size(tauri::LogicalSize::new(PICKER_WIDTH, PICKER_HEIGHT));
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

/// Where the picker was placed by `show_picker` (x, y, its screen), so a
/// resize can grow it downward and move it up only as far as the screen needs.
static PICKER_ANCHOR: std::sync::Mutex<Option<(f64, f64, ScreenRect)>> = std::sync::Mutex::new(None);

/// Picker height for `extra` points of expanded prompt rows, capped to the screen.
fn picker_height(extra: f64, screen_height: f64) -> f64 {
    let max_h = (screen_height - 20.0).max(PICKER_HEIGHT);
    (PICKER_HEIGHT + extra.max(0.0)).min(max_h)
}

/// Grow (or shrink back) the picker to fit expanded prompt rows. It keeps its
/// top edge unless the bottom would leave the screen.
pub fn resize_picker(app: &AppHandle, extra: f64) {
    let Some((x, y, screen)) = PICKER_ANCHOR.lock().unwrap_or_else(|e| e.into_inner()).clone() else { return };
    let Some(w) = app.get_webview_window(PICKER_LABEL) else { return };
    let h = picker_height(extra, screen.height);
    let bottom = screen.y + screen.height - 10.0;
    let top = y.min(bottom - h).max(screen.y + 10.0);
    let _ = w.set_size(tauri::LogicalSize::new(PICKER_WIDTH, h));
    let _ = w.set_position(tauri::LogicalPosition::new(x, top));
}

/// Hide the picker popup
pub fn hide_picker(app: &AppHandle) {
    crate::picker_keys::set_picker_visible(false);
    if let Ok(p) = app.get_webview_panel(PICKER_LABEL) {
        p.order_out(None);
    }
    if let Some(w) = app.get_webview_window(PICKER_LABEL) {
        let _ = w.hide();
        let _ = w.set_position(tauri::LogicalPosition::new(-3000.0, -3000.0));
    }
}

/// Get cursor position in top-left global screen coordinates.
/// Converts with the *primary* screen's height (screens[0]); `mainScreen` is
/// the screen of the key window and gives wrong results on multi-monitor setups.
pub fn get_cursor_topleft_pos() -> (f64, f64) {
    #[cfg(target_os = "macos")]
    unsafe {
        use cocoa::appkit::NSScreen;
        use cocoa::foundation::NSPoint;
        use objc::{msg_send, sel, sel_impl};
        let cls = objc::runtime::Class::get("NSEvent").unwrap();
        let location: NSPoint = msg_send![cls, mouseLocation];
        let screens: *mut objc::runtime::Object =
            msg_send![objc::runtime::Class::get("NSScreen").unwrap(), screens];
        let count: usize = msg_send![screens, count];
        let primary_h = if count == 0 {
            900.0
        } else {
            let primary: *mut objc::runtime::Object = msg_send![screens, objectAtIndex: 0usize];
            NSScreen::frame(primary).size.height
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
    // Keep the hover detector's idea of the panel in step with its new size.
    store_panel_bounds(sx, panel_y, viewer_width, panel_height);

    if let Some(w) = app.get_webview_window(SIDEBAR_LABEL) {
        let _ = w.set_size(tauri::LogicalSize::new(TAB_BAR_WIDTH, panel_height));
        let _ = w.set_position(tauri::LogicalPosition::new(sx, panel_y));
    }
    sync_chrome(app);
    if let Some(label) = get_active_page_label() {
        if let Some(w) = app.get_webview_window(&label) {
            let (x, y, _, page_height) = page_frame();
            let _ = w.set_size(tauri::LogicalSize::new(viewer_width, page_height));
            let _ = w.set_position(tauri::LogicalPosition::new(x, y));
        }
    }
}

#[cfg(test)]
mod grip_tests {
    use super::*;

    const SCREEN: (f64, f64, f64, f64) = (0.0, 0.0, 1440.0, 900.0);

    #[test]
    fn grip_follows_the_drag() {
        assert_eq!(grip_size_for((600.0, 600.0, 150.0), 40.0, 30.0, SCREEN), (640.0, 630.0));
        assert_eq!(grip_size_for((600.0, 600.0, 150.0), -40.0, -30.0, SCREEN), (560.0, 570.0));
    }

    #[test]
    fn grip_respects_minimums_and_screen_edges() {
        let (w, h) = grip_size_for((600.0, 600.0, 150.0), -2000.0, -2000.0, SCREEN);
        assert_eq!((w, h), (MIN_VIEWER_WIDTH, MIN_PANEL_HEIGHT));
        let (w, h) = grip_size_for((600.0, 600.0, 150.0), 5000.0, 5000.0, SCREEN);
        assert_eq!(w, 1440.0 - TAB_BAR_WIDTH - 8.0);
        assert_eq!(h, 900.0 - 150.0 - 4.0);
    }
}

#[cfg(test)]
mod grip_reveal_tests {
    use super::*;

    #[test]
    fn grip_reveals_only_near_the_corner() {
        assert!(grip_is_near((990.0, 790.0), (1000.0, 800.0)));
        assert!(grip_is_near((1030.0, 830.0), (1000.0, 800.0)));
        assert!(!grip_is_near((900.0, 800.0), (1000.0, 800.0)));
        assert!(!grip_is_near((1000.0, 700.0), (1000.0, 800.0)));
    }
}

#[cfg(test)]
mod picker_size_tests {
    use super::*;

    #[test]
    fn picker_grows_with_prompts_and_stays_on_screen() {
        assert_eq!(picker_height(0.0, 900.0), PICKER_HEIGHT);
        assert_eq!(picker_height(-50.0, 900.0), PICKER_HEIGHT);
        assert_eq!(picker_height(120.0, 900.0), PICKER_HEIGHT + 120.0);
        assert_eq!(picker_height(5000.0, 900.0), 880.0);
    }
}
