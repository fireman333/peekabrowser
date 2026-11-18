//! "Sent" chip: when captured text (three-finger tap or ⌘C+C) goes straight to
//! a destination without the picker, a small capsule styled like the picker
//! rows pops up beside the pointer — destination icon, name and a snippet of
//! the text — then fades. Nothing is drawn when the picker opens (the picker
//! itself is the feedback). Text-box outlines were dropped: the captured
//! text's on-screen bounds are often slightly off, so the box looked skewed.
//! A transparent, click-through, non-activating panel; the content is set
//! from here with `eval`, so no IPC permission is involved.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_nspanel::ManagerExt;

pub const LABEL: &str = "lookup-highlight";
/// Overlay window size; the capsule sits inside with room for its shadow.
const WIN_W: f64 = 360.0;
const WIN_H: f64 = 84.0;
/// Capsule's top-left inside the window.
const CHIP_INSET: (f64, f64) = (16.0, 12.0);
/// Capsule's top-left relative to the pointer (below-right, clear of the cursor).
const CURSOR_OFFSET: (f64, f64) = (14.0, 20.0);
/// Fade in + hold + fade out (see highlight.css) plus a margin.
const VISIBLE_FOR: Duration = Duration::from_millis(1500);
const SNIPPET_CHARS: usize = 28;

static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Create the hidden overlay panel (called once at launch).
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let w = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("highlight.html".into()))
        .title("Peekabrowser Sent")
        .inner_size(WIN_W, WIN_H)
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

/// First line of the text, whitespace collapsed, cut to a short snippet.
pub fn snippet(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = line.chars().take(SNIPPET_CHARS).collect();
    if line.chars().count() > SNIPPET_CHARS {
        out.push('…');
    }
    out
}

/// Window origin for a pointer at `cursor`, kept inside `screen`
/// (x, y, w, h); flips to the pointer's left/top near the right/bottom edge.
/// Pure, for tests.
pub fn window_origin(cursor: (f64, f64), screen: (f64, f64, f64, f64)) -> (f64, f64) {
    let (sx, sy, sw, sh) = screen;
    let mut x = cursor.0 + CURSOR_OFFSET.0 - CHIP_INSET.0;
    let mut y = cursor.1 + CURSOR_OFFSET.1 - CHIP_INSET.1;
    if x + WIN_W > sx + sw {
        x = cursor.0 - CURSOR_OFFSET.0 - WIN_W + CHIP_INSET.0;
    }
    if y + WIN_H > sy + sh {
        y = cursor.1 - CURSOR_OFFSET.1 - WIN_H + CHIP_INSET.1;
    }
    (x.max(sx), y.max(sy))
}

/// Show the chip for text sent to `dest`. Safe from any thread; returns
/// immediately.
pub fn show_sent(app: &AppHandle, cursor: (f64, f64), dest: &crate::destinations::Destination, text: &str) {
    let screen = super::get_all_screens()
        .into_iter()
        .find(|s| cursor.0 >= s.x && cursor.0 < s.x + s.width && cursor.1 >= s.y && cursor.1 < s.y + s.height)
        .map(|s| (s.x, s.y, s.width, s.height))
        .unwrap_or((cursor.0 - 2000.0, cursor.1 - 2000.0, 4000.0, 4000.0));
    let (x, y) = window_origin(cursor, screen);
    let flipped = x + CHIP_INSET.0 < cursor.0;
    let payload = serde_json::json!({
        "name": dest.name,
        "icon": dest.icon,
        "url": dest.url,
        "snippet": snippet(text),
        "alignRight": flipped,
    })
    .to_string();
    let gen = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(win) = app2.get_webview_window(LABEL) else { return };
        let _ = win.set_position(tauri::LogicalPosition::new(x, y));
        let _ = win.set_size(tauri::LogicalSize::new(WIN_W, WIN_H));
        if let Ok(p) = app2.get_webview_panel(LABEL) {
            p.order_front_regardless();
        }
        let _ = win.eval(&format!("window.__pbSent && window.__pbSent({})", payload));
    });
    let app3 = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(VISIBLE_FOR);
        if GENERATION.load(Ordering::SeqCst) != gen {
            return; // a newer chip owns the window
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

    const SCREEN: (f64, f64, f64, f64) = (0.0, 0.0, 1440.0, 900.0);

    #[test]
    fn chip_sits_below_right_of_the_pointer() {
        let (x, y) = window_origin((500.0, 300.0), SCREEN);
        assert_eq!((x + CHIP_INSET.0, y + CHIP_INSET.1), (514.0, 320.0));
    }

    #[test]
    fn chip_flips_near_the_right_and_bottom_edges() {
        let (x, y) = window_origin((1420.0, 890.0), SCREEN);
        assert!(x + WIN_W <= 1440.0 && y + WIN_H <= 900.0);
        assert!(x + CHIP_INSET.0 < 1420.0);
    }

    #[test]
    fn snippet_collapses_whitespace_and_truncates() {
        assert_eq!(snippet("  hello\n  world "), "hello world");
        assert_eq!(snippet(&"字".repeat(40)), format!("{}…", "字".repeat(SNIPPET_CHARS)));
    }
}
