//! Animated highlight drawn around the text a three-finger tap captured
//! (or a small ring at the cursor when the text's bounds are unknown).
//! A transparent, click-through, non-activating panel; the animation lives
//! in highlight.html and is started from here with `eval`, so no IPC
//! permission is involved.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_nspanel::ManagerExt;

pub const LABEL: &str = "lookup-highlight";
/// Room around the box for the glow (points).
const PAD: f64 = 10.0;
/// Ring size when only the cursor position is known.
const DOT: f64 = 28.0;
/// Animation length (in 0.2 + hold + out 0.4 ≈ 1.0 s) plus a margin.
const VISIBLE_FOR: Duration = Duration::from_millis(1100);

static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Create the hidden overlay panel (called once at launch).
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let w = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("highlight.html".into()))
        .title("Peekabrowser Highlight")
        .inner_size(DOT + 2.0 * PAD, DOT + 2.0 * PAD)
        .position(-3000.0, -3000.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .build()?;
    super::setup_non_activating_panel(&w)?;
    let _ = w.set_ignore_cursor_events(true);
    Ok(())
}

/// Frame (x, y, w, h) of the overlay window and the shape to draw, in global
/// top-left points. Pure, for tests.
pub fn overlay_frame(bounds: Option<(f64, f64, f64, f64)>, cursor: (f64, f64)) -> ((f64, f64, f64, f64), &'static str) {
    match bounds.filter(|b| b.2 >= 2.0 && b.3 >= 2.0 && b.2 <= 4000.0 && b.3 <= 4000.0) {
        // Grow the text box slightly so the border doesn't sit on the glyphs.
        Some((x, y, w, h)) => ((x - 3.0 - PAD, y - 2.0 - PAD, w + 6.0 + 2.0 * PAD, h + 4.0 + 2.0 * PAD), "box"),
        None => ((cursor.0 - DOT / 2.0 - PAD, cursor.1 - DOT / 2.0 - PAD, DOT + 2.0 * PAD, DOT + 2.0 * PAD), "dot"),
    }
}

/// Play the highlight. Safe to call from any thread.
pub fn show(app: &AppHandle, bounds: Option<(f64, f64, f64, f64)>, cursor: (f64, f64)) {
    let ((x, y, w, h), kind) = overlay_frame(bounds, cursor);
    let gen = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(win) = app2.get_webview_window(LABEL) else { return };
        let _ = win.set_position(tauri::LogicalPosition::new(x, y));
        let _ = win.set_size(tauri::LogicalSize::new(w, h));
        if let Ok(p) = app2.get_webview_panel(LABEL) {
            p.order_front_regardless();
        }
        let _ = win.eval(&format!("window.__pbPlay && window.__pbPlay({:?}, {})", kind, PAD));
    });
    let app3 = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(VISIBLE_FOR);
        if GENERATION.load(Ordering::SeqCst) != gen {
            return; // a newer highlight owns the window
        }
        let app4 = app3.clone();
        let _ = app3.run_on_main_thread(move || {
            if let Ok(p) = app4.get_webview_panel(LABEL) {
                p.order_out(None);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_grows_around_text_with_glow_padding() {
        let ((x, y, w, h), kind) = overlay_frame(Some((100.0, 200.0, 40.0, 16.0)), (0.0, 0.0));
        assert_eq!(kind, "box");
        assert_eq!((x, y, w, h), (87.0, 188.0, 66.0, 40.0));
    }

    #[test]
    fn missing_or_absurd_bounds_fall_back_to_cursor_ring() {
        for b in [None, Some((0.0, 0.0, 0.0, 10.0)), Some((0.0, 0.0, 9000.0, 10.0))] {
            let ((x, y, w, h), kind) = overlay_frame(b, (500.0, 300.0));
            assert_eq!(kind, "dot");
            assert_eq!((x + w / 2.0, y + h / 2.0), (500.0, 300.0));
        }
    }
}
