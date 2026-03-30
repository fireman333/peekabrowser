use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tauri::AppHandle;

static HOVER_THREAD_RUNNING: AtomicBool = AtomicBool::new(false);

/// Flag: panel is pinned — auto-hide is disabled entirely.
/// Only manual toggle (⌘⇧A) can hide the panel.
static PINNED: AtomicBool = AtomicBool::new(false);

/// Flag: panel was shown manually (Cmd+C+C, screenshot, tray, shortcut).
/// When true, panel won't auto-hide until cursor visits it then leaves.
static MANUAL_SHOW_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Flag: cursor has entered the panel area at least once since manual show.
static CURSOR_HAS_VISITED: AtomicBool = AtomicBool::new(false);

/// Timestamp (secs) of last manual show — used for absolute fallback timeout.
static MANUAL_SHOW_TS: AtomicU64 = AtomicU64::new(0);

/// Absolute fallback: even if cursor never visits, hide after this many seconds.
const FALLBACK_TIMEOUT_SECS: u64 = 60;

pub fn mark_manual_show() {
    MANUAL_SHOW_ACTIVE.store(true, Ordering::Relaxed);
    CURSOR_HAS_VISITED.store(false, Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    MANUAL_SHOW_TS.store(now, Ordering::Relaxed);
}

/// Clear the manual show state (called when panel is hidden by any means)
pub fn clear_manual_show() {
    MANUAL_SHOW_ACTIVE.store(false, Ordering::Relaxed);
    CURSOR_HAS_VISITED.store(false, Ordering::Relaxed);
}

/// Toggle pin state. Returns new pinned state.
pub fn toggle_pin() -> bool {
    let was_pinned = PINNED.load(Ordering::Relaxed);
    PINNED.store(!was_pinned, Ordering::Relaxed);
    !was_pinned
}

/// Check if panel is pinned
pub fn is_pinned() -> bool {
    PINNED.load(Ordering::Relaxed)
}

/// Check if we're in "manual show, waiting for visit-then-leave" mode
fn is_manual_show_active() -> bool {
    MANUAL_SHOW_ACTIVE.load(Ordering::Relaxed)
}

/// Check if the absolute fallback timeout has expired
fn is_fallback_expired() -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    now.saturating_sub(MANUAL_SHOW_TS.load(Ordering::Relaxed)) >= FALLBACK_TIMEOUT_SECS
}

/// Start the mouse edge hover detector on a background thread
pub fn start_hover_detector(app: AppHandle) {
    if HOVER_THREAD_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }

    std::thread::spawn(move || {
        hover_loop(app);
        HOVER_THREAD_RUNNING.store(false, Ordering::SeqCst);
    });
}

/// Check if cursor is near the left edge of any screen.
/// Returns Some(screen) if near edge, None otherwise.
fn near_any_screen_left_edge(cx: f64, cy: f64, edge_zone: f64) -> Option<super::ScreenRect> {
    let screens = super::get_all_screens();
    for s in screens {
        // Cursor must be within this screen's vertical range
        if cy >= s.y && cy < s.y + s.height {
            // And within edge_zone of this screen's left edge
            if cx >= s.x && cx <= s.x + edge_zone {
                return Some(s);
            }
        }
    }
    None
}

/// Check if cursor is inside the panel bounds (sidebar + viewer area)
/// Uses actual panel bounds set by show_panel_inner for accuracy on all screens.
fn cursor_in_panel(cx: f64, cy: f64) -> bool {
    let (left, top, right, bottom) = super::panel_bounds();

    const RIGHT_PAD: f64 = 80.0;
    const BOTTOM_PAD: f64 = 150.0;
    const TOP_PAD: f64 = 40.0;

    cx >= left && cx <= right + RIGHT_PAD
        && cy >= top - TOP_PAD
        && cy <= bottom + BOTTOM_PAD
}

fn hover_loop(app: AppHandle) {
    let mut dwell_start: Option<Instant> = None;
    let mut was_near_edge = false;
    const DWELL_DURATION: Duration = Duration::from_millis(300);
    const EDGE_ZONE_PX: f64 = 3.0;
    const POLL_MS: u64 = 16;
    // Extra margin beyond the viewer right edge before we hide
    const LEAVE_PADDING: f64 = 60.0;

    loop {
        std::thread::sleep(Duration::from_millis(POLL_MS));

        let (cx, cy) = get_cursor_pos_topleft();

        // Check if cursor is near the left edge of ANY screen
        let near_edge = near_any_screen_left_edge(cx, cy, EDGE_ZONE_PX);
        let is_near_edge = near_edge.is_some();

        // ── Edge hover: show panel when cursor dwells at left edge ──
        if let Some(screen) = near_edge {
            if !was_near_edge {
                dwell_start = Some(Instant::now());
                was_near_edge = true;
            } else if let Some(start) = dwell_start {
                if start.elapsed() >= DWELL_DURATION && !super::is_panel_visible(&app) {
                    // Update stored screen to the one the cursor is on, then show
                    let screen_clone = screen.clone();
                    let app2 = app.clone();
                    let _ = app.run_on_main_thread(move || {
                        if let Ok(mut g) = super::CURRENT_SCREEN_X.lock() { *g = screen_clone.x; }
                        if let Ok(mut g) = super::CURRENT_SCREEN_Y.lock() { *g = screen_clone.y; }
                        if let Ok(mut g) = super::CURRENT_SCREEN_W.lock() { *g = screen_clone.width; }
                        if let Ok(mut g) = super::CURRENT_SCREEN_H.lock() { *g = screen_clone.height; }
                        super::show_panel_from_edge(&app2);
                    });
                    dwell_start = None;
                }
            }
        } else {
            was_near_edge = false;
            dwell_start = None;
        }

        // ── Auto-hide logic (runs regardless of near_edge, skip if pinned) ──
        if super::is_panel_visible(&app) && !PINNED.load(Ordering::Relaxed) {
            let in_panel = cursor_in_panel(cx, cy);

            if is_manual_show_active() {
                // Manual show mode: wait for cursor to visit, then leave
                if in_panel {
                    CURSOR_HAS_VISITED.store(true, Ordering::Relaxed);
                } else if CURSOR_HAS_VISITED.load(Ordering::Relaxed) {
                    // Cursor visited and now left — allow hiding
                    std::thread::sleep(Duration::from_millis(150));
                    let (cx2, cy2) = get_cursor_pos_topleft();
                    if !cursor_in_panel(cx2, cy2) {
                        clear_manual_show();
                        let app2 = app.clone();
                        let _ = app.run_on_main_thread(move || {
                            super::hide_panel(&app2);
                        });
                    }
                } else if is_fallback_expired() {
                    clear_manual_show();
                    let app2 = app.clone();
                    let _ = app.run_on_main_thread(move || {
                        super::hide_panel(&app2);
                    });
                }
            } else if !is_near_edge {
                // Normal mode (edge hover): hide when cursor leaves panel area
                // Only check when NOT near edge (so edge dwell doesn't cause immediate hide)
                let (left, _, right, _) = super::panel_bounds();

                if cx > right + LEAVE_PADDING || cx < left - LEAVE_PADDING {
                    std::thread::sleep(Duration::from_millis(150));
                    let (cx2, _) = get_cursor_pos_topleft();
                    if cx2 > right + LEAVE_PADDING || cx2 < left - LEAVE_PADDING {
                        let app2 = app.clone();
                        let _ = app.run_on_main_thread(move || {
                            super::hide_panel(&app2);
                        });
                    }
                }
            }
        }
    }
}

/// Get the current cursor position in top-left screen coordinates
fn get_cursor_pos_topleft() -> (f64, f64) {
    #[cfg(target_os = "macos")]
    unsafe {
        use cocoa::appkit::NSScreen;
        use cocoa::base::nil;
        use cocoa::foundation::NSPoint;
        use objc::{msg_send, sel, sel_impl};

        let cls = objc::runtime::Class::get("NSEvent").unwrap();
        let location: NSPoint = msg_send![cls, mouseLocation];

        // Convert from macOS bottom-left to top-left using primary screen height
        let screen = NSScreen::mainScreen(nil);
        let primary_h = if screen.is_null() {
            900.0
        } else {
            NSScreen::frame(screen).size.height
        };

        return (location.x, primary_h - location.y);
    }
    #[cfg(not(target_os = "macos"))]
    {
        (999.0, 999.0)
    }
}
