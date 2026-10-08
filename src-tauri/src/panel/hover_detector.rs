//! Edge reveal and auto-hide, driven by mouse events instead of polling.
//!
//! - An NSEvent global monitor (+ local monitor for our own windows) reports
//!   mouse movement. Mouse monitors need no Accessibility permission.
//! - Entering the left-edge zone starts a one-shot 300 ms dwell timer;
//!   leaving cancels it. A cursor resting at the edge needs no further events.
//! - Auto-hide checks run on movement while the panel is visible, with a
//!   one-shot confirm delay; the manual-show fallback is a one-shot timer.
//! - Screen geometry is cached and refreshed when the display configuration
//!   changes.
//! - Monitors are installed only while needed: always if edge reveal is on,
//!   otherwise only while the panel is visible.
//! - If a monitor can't be installed, a slow fallback poller is used.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// Flag: panel is pinned — auto-hide is disabled entirely.
static PINNED: AtomicBool = AtomicBool::new(false);
/// Flag: panel was shown manually (Cmd+C+C, screenshot, tray, shortcut).
/// When true, panel won't auto-hide until cursor visits it then leaves.
static MANUAL_SHOW_ACTIVE: AtomicBool = AtomicBool::new(false);
/// Flag: cursor has entered the panel area at least once since manual show.
static CURSOR_HAS_VISITED: AtomicBool = AtomicBool::new(false);
/// Bumped on each manual show; the fallback timer only acts on its own show.
static MANUAL_SHOW_GEN: AtomicU64 = AtomicU64::new(0);

static NEAR_EDGE: AtomicBool = AtomicBool::new(false);
static EDGE_GEN: AtomicU64 = AtomicU64::new(0);
static HIDE_PENDING: AtomicBool = AtomicBool::new(false);

static MONITORS: Mutex<Vec<usize>> = Mutex::new(Vec::new());
static FALLBACK_RUNNING: AtomicBool = AtomicBool::new(false);
static SCREEN_OBSERVER: AtomicBool = AtomicBool::new(false);
static SCREENS: Mutex<Vec<super::ScreenRect>> = Mutex::new(Vec::new());

/// Absolute fallback: even if cursor never visits, hide after this many seconds.
const FALLBACK_TIMEOUT: Duration = Duration::from_secs(60);
const DWELL: Duration = Duration::from_millis(300);
const EDGE_ZONE_PX: f64 = 3.0;
const LEAVE_PADDING: f64 = 60.0;
const HIDE_CONFIRM: Duration = Duration::from_millis(150);

pub fn mark_manual_show(app: &AppHandle) {
    MANUAL_SHOW_ACTIVE.store(true, Ordering::Relaxed);
    CURSOR_HAS_VISITED.store(false, Ordering::Relaxed);
    let gen = MANUAL_SHOW_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FALLBACK_TIMEOUT);
        if MANUAL_SHOW_GEN.load(Ordering::SeqCst) == gen
            && MANUAL_SHOW_ACTIVE.load(Ordering::Relaxed)
            && !CURSOR_HAS_VISITED.load(Ordering::Relaxed)
            && !PINNED.load(Ordering::Relaxed)
        {
            let app2 = app.clone();
            let _ = app.run_on_main_thread(move || {
                if super::is_panel_visible(&app2) {
                    super::hide_panel(&app2);
                }
            });
        }
    });
}

/// Clear the manual show state (called when panel is hidden by any means)
pub fn clear_manual_show() {
    MANUAL_SHOW_ACTIVE.store(false, Ordering::Relaxed);
    CURSOR_HAS_VISITED.store(false, Ordering::Relaxed);
}

/// Toggle pin state. Returns new pinned state.
pub fn toggle_pin() -> bool {
    !PINNED.fetch_xor(true, Ordering::Relaxed)
}

pub fn is_pinned() -> bool {
    PINNED.load(Ordering::Relaxed)
}

fn edge_enabled(app: &AppHandle) -> bool {
    app.try_state::<crate::app_settings::AppSettingsStore>()
        .map(|s| s.get().edge_hover_enabled)
        .unwrap_or(true)
}

fn cached_screens() -> Vec<super::ScreenRect> {
    let mut g = SCREENS.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_empty() {
        *g = super::get_all_screens();
    }
    g.clone()
}

fn invalidate_screens() {
    if let Ok(mut g) = SCREENS.lock() {
        g.clear();
    }
}

fn edge_screen(cx: f64, cy: f64) -> Option<super::ScreenRect> {
    cached_screens()
        .into_iter()
        .find(|s| cy >= s.y && cy < s.y + s.height && cx >= s.x && cx <= s.x + EDGE_ZONE_PX)
}

/// Cursor inside the panel (sidebar + viewer) plus a margin.
fn cursor_in_panel(cx: f64, cy: f64) -> bool {
    let (left, top, right, bottom) = super::panel_bounds();
    const RIGHT_PAD: f64 = 80.0;
    const BOTTOM_PAD: f64 = 150.0;
    const TOP_PAD: f64 = 40.0;
    cx >= left && cx <= right + RIGHT_PAD && cy >= top - TOP_PAD && cy <= bottom + BOTTOM_PAD
}

fn mouse_button_down() -> bool {
    #[cfg(target_os = "macos")]
    unsafe {
        use objc::{msg_send, sel, sel_impl};
        let cls = objc::runtime::Class::get("NSEvent").unwrap();
        let buttons: usize = msg_send![cls, pressedMouseButtons];
        buttons != 0
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Handle one cursor observation. Cheap; safe from any thread.
fn on_cursor(app: &AppHandle, cx: f64, cy: f64) {
    let visible = super::is_panel_visible(app);

    // ── Edge reveal ──
    if !visible && edge_enabled(app) {
        match edge_screen(cx, cy) {
            Some(screen) => {
                if !NEAR_EDGE.swap(true, Ordering::SeqCst) {
                    let gen = EDGE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
                    let app = app.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(DWELL);
                        if EDGE_GEN.load(Ordering::SeqCst) != gen {
                            return;
                        }
                        let app2 = app.clone();
                        let _ = app.run_on_main_thread(move || {
                            let (x, y) = super::get_cursor_topleft_pos();
                            if EDGE_GEN.load(Ordering::SeqCst) != gen || edge_screen(x, y).is_none() {
                                return;
                            }
                            if super::is_panel_visible(&app2) {
                                return;
                            }
                            if let Ok(mut g) = super::CURRENT_SCREEN_X.lock() { *g = screen.x; }
                            if let Ok(mut g) = super::CURRENT_SCREEN_Y.lock() { *g = screen.y; }
                            if let Ok(mut g) = super::CURRENT_SCREEN_W.lock() { *g = screen.width; }
                            if let Ok(mut g) = super::CURRENT_SCREEN_H.lock() { *g = screen.height; }
                            super::show_panel_from_edge(&app2);
                        });
                    });
                }
            }
            None => {
                if NEAR_EDGE.swap(false, Ordering::SeqCst) {
                    EDGE_GEN.fetch_add(1, Ordering::SeqCst); // cancel pending dwell
                }
            }
        }
        return;
    }
    NEAR_EDGE.store(false, Ordering::SeqCst);

    // ── Auto-hide ──
    if !visible || PINNED.load(Ordering::Relaxed) {
        return;
    }
    let should_hide = if MANUAL_SHOW_ACTIVE.load(Ordering::Relaxed) {
        if cursor_in_panel(cx, cy) {
            CURSOR_HAS_VISITED.store(true, Ordering::Relaxed);
            false
        } else {
            CURSOR_HAS_VISITED.load(Ordering::Relaxed)
        }
    } else {
        let (left, _, right, _) = super::panel_bounds();
        cx > right + LEAVE_PADDING || cx < left - LEAVE_PADDING
    };
    if should_hide && !HIDE_PENDING.swap(true, Ordering::SeqCst) {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(HIDE_CONFIRM);
            let app2 = app.clone();
            let _ = app.run_on_main_thread(move || {
                HIDE_PENDING.store(false, Ordering::SeqCst);
                if PINNED.load(Ordering::Relaxed) || !super::is_panel_visible(&app2) || mouse_button_down() {
                    return; // pinned, already hidden, or dragging/selecting
                }
                let (x, y) = super::get_cursor_topleft_pos();
                let still_out = if MANUAL_SHOW_ACTIVE.load(Ordering::Relaxed) {
                    !cursor_in_panel(x, y)
                } else {
                    let (left, _, right, _) = super::panel_bounds();
                    x > right + LEAVE_PADDING || x < left - LEAVE_PADDING
                };
                if still_out {
                    clear_manual_show();
                    super::hide_panel(&app2);
                }
            });
        });
    }
}

/// Start detection (call once on the main thread during setup).
pub fn start_hover_detector(app: AppHandle) {
    install_screen_observer();
    sync_monitors(&app);
}

/// Install or remove the mouse monitors to match current needs.
pub fn sync_monitors(app: &AppHandle) {
    let needed = edge_enabled(app) || super::is_panel_visible(app);
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let installed = MONITORS.lock().map(|m| !m.is_empty()).unwrap_or(false);
        if needed && !installed && !FALLBACK_RUNNING.load(Ordering::SeqCst) {
            if !install_monitors(&app2) {
                log::warn!("hover: event monitors unavailable, using fallback poller");
                start_fallback(app2.clone());
            }
        } else if !needed && installed {
            remove_monitors();
        }
    });
}

#[cfg(target_os = "macos")]
fn install_monitors(app: &AppHandle) -> bool {
    use block2::RcBlock;
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};

    // NSEventMaskMouseMoved | LeftMouseDragged | RightMouseDragged | OtherMouseDragged
    const MASK: u64 = (1 << 5) | (1 << 6) | (1 << 7) | (1 << 27);
    unsafe {
        let Some(cls) = AnyClass::get(c"NSEvent") else { return false };
        let app_g = app.clone();
        let global = RcBlock::new(move |_ev: *mut AnyObject| {
            let (x, y) = super::get_cursor_topleft_pos();
            on_cursor(&app_g, x, y);
        });
        let app_l = app.clone();
        let local = RcBlock::new(move |ev: *mut AnyObject| -> *mut AnyObject {
            let (x, y) = super::get_cursor_topleft_pos();
            on_cursor(&app_l, x, y);
            ev
        });
        let g: *mut AnyObject = msg_send![cls, addGlobalMonitorForEventsMatchingMask: MASK, handler: &*global];
        if g.is_null() {
            return false;
        }
        let l: *mut AnyObject = msg_send![cls, addLocalMonitorForEventsMatchingMask: MASK, handler: &*local];
        let mut m = MONITORS.lock().unwrap_or_else(|e| e.into_inner());
        let g: *mut AnyObject = msg_send![g, retain];
        m.push(g as usize);
        if !l.is_null() {
            let l: *mut AnyObject = msg_send![l, retain];
            m.push(l as usize);
        }
        log::info!("hover: event monitors installed");
        true
    }
}

#[cfg(target_os = "macos")]
fn remove_monitors() {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    let mut m = MONITORS.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        let Some(cls) = AnyClass::get(c"NSEvent") else { return };
        for ptr in m.drain(..) {
            let obj = ptr as *mut AnyObject;
            let _: () = msg_send![cls, removeMonitor: obj];
            let _: () = msg_send![obj, release];
        }
    }
    NEAR_EDGE.store(false, Ordering::SeqCst);
    EDGE_GEN.fetch_add(1, Ordering::SeqCst);
    log::info!("hover: event monitors removed");
}

#[cfg(target_os = "macos")]
fn install_screen_observer() {
    use block2::RcBlock;
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::NSString;
    if SCREEN_OBSERVER.swap(true, Ordering::SeqCst) {
        return;
    }
    unsafe {
        let Some(cls) = AnyClass::get(c"NSNotificationCenter") else { return };
        let center: *mut AnyObject = msg_send![cls, defaultCenter];
        let name = NSString::from_str("NSApplicationDidChangeScreenParametersNotification");
        let block = RcBlock::new(|_n: *mut AnyObject| invalidate_screens());
        let nil: *mut AnyObject = std::ptr::null_mut();
        let token: *mut AnyObject =
            msg_send![center, addObserverForName: &*name, object: nil, queue: nil, usingBlock: &*block];
        // Observer lives for the app's lifetime.
        let _: *mut AnyObject = msg_send![token, retain];
    }
}

#[cfg(not(target_os = "macos"))]
fn install_monitors(_app: &AppHandle) -> bool {
    false
}
#[cfg(not(target_os = "macos"))]
fn remove_monitors() {}
#[cfg(not(target_os = "macos"))]
fn install_screen_observer() {}

/// Last resort when monitors can't be installed: a slow poller (100 ms) that
/// feeds the same logic. Stops when monitoring is no longer needed.
fn start_fallback(app: AppHandle) {
    if FALLBACK_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(100));
            if !(edge_enabled(&app) || super::is_panel_visible(&app)) {
                break;
            }
            let (x, y) = super::get_cursor_topleft_pos();
            on_cursor(&app, x, y);
        }
        FALLBACK_RUNNING.store(false, Ordering::SeqCst);
    });
}
