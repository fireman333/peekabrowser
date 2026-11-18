//! Animated highlight drawn around the text a three-finger tap captured.
//! When the text's bounds are unknown, a word-sized marker pill is drawn at
//! the cursor instead, so the feedback still reads as "this text was taken".
//! A transparent, click-through, non-activating panel; the animation lives
//! in highlight.html and is started from here with `eval`, so no IPC
//! permission is involved.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_nspanel::ManagerExt;

pub const LABEL: &str = "lookup-highlight";
/// Room around the box for the pop overshoot, ripple and glow (points).
const PAD: f64 = 18.0;
/// Marker pill drawn when only the cursor position is known.
const PILL_H: f64 = 26.0;
const PILL_MIN_W: f64 = 56.0;
const PILL_MAX_W: f64 = 240.0;
/// Animation length (≈ 0.56 s, see highlight.css) plus a margin.
const VISIBLE_FOR: Duration = Duration::from_millis(700);

static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Create the hidden overlay panel (called once at launch).
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let w = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("highlight.html".into()))
        .title("Peekabrowser Highlight")
        .inner_size(PILL_MIN_W + 2.0 * PAD, PILL_H + 2.0 * PAD)
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

/// Rough on-screen width of the captured text (first line only), used to
/// size the pill: ~8 pt per narrow character, ~15 pt per wide (CJK) one.
pub fn pill_width(text: &str) -> f64 {
    let line = text.trim().lines().next().unwrap_or("");
    let est: f64 = line.chars().map(|c| if (c as u32) >= 0x2E80 { 15.0 } else { 8.0 }).sum();
    (est + 16.0).clamp(PILL_MIN_W, PILL_MAX_W)
}

/// Frame (x, y, w, h) of the overlay window and the shape to draw, in global
/// top-left points. Pure, for tests.
pub fn overlay_frame(
    bounds: Option<(f64, f64, f64, f64)>,
    cursor: (f64, f64),
    text: &str,
) -> ((f64, f64, f64, f64), &'static str) {
    match bounds.filter(|b| b.2 >= 2.0 && b.3 >= 2.0 && b.2 <= 4000.0 && b.3 <= 4000.0) {
        // Grow the text box slightly so the border doesn't sit on the glyphs.
        Some((x, y, w, h)) => ((x - 4.0 - PAD, y - 3.0 - PAD, w + 8.0 + 2.0 * PAD, h + 6.0 + 2.0 * PAD), "box"),
        None => {
            let w = pill_width(text);
            ((cursor.0 - w / 2.0 - PAD, cursor.1 - PILL_H / 2.0 - PAD, w + 2.0 * PAD, PILL_H + 2.0 * PAD), "pill")
        }
    }
}

/// Play the highlight. `text` only sizes the fallback pill. Safe to call
/// from any thread; returns immediately.
pub fn show(app: &AppHandle, bounds: Option<(f64, f64, f64, f64)>, cursor: (f64, f64), text: &str) {
    let ((x, y, w, h), kind) = overlay_frame(bounds, cursor, text);
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
        let ((x, y, w, h), kind) = overlay_frame(Some((100.0, 200.0, 40.0, 16.0)), (0.0, 0.0), "word");
        assert_eq!(kind, "box");
        assert_eq!((x, y, w, h), (78.0, 179.0, 84.0, 58.0));
    }

    #[test]
    fn missing_or_absurd_bounds_fall_back_to_pill_at_cursor() {
        for b in [None, Some((0.0, 0.0, 0.0, 10.0)), Some((0.0, 0.0, 9000.0, 10.0))] {
            let ((x, y, w, h), kind) = overlay_frame(b, (500.0, 300.0), "lookup");
            assert_eq!(kind, "pill");
            assert_eq!((x + w / 2.0, y + h / 2.0), (500.0, 300.0));
            assert_eq!(h, PILL_H + 2.0 * PAD);
            assert!(w > h, "pill is wider than tall");
        }
    }

    #[test]
    fn pill_width_follows_text_and_is_clamped() {
        assert_eq!(pill_width(""), PILL_MIN_W);
        assert_eq!(pill_width("a"), PILL_MIN_W);
        assert_eq!(pill_width("highlight"), 9.0 * 8.0 + 16.0);
        assert_eq!(pill_width("醫學名詞"), 4.0 * 15.0 + 16.0);
        assert_eq!(pill_width(&"long ".repeat(80)), PILL_MAX_W);
        // Only the first line counts.
        assert_eq!(pill_width("highlight\nand a much longer second line of text"), 88.0);
    }
}
