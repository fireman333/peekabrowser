//! Small, focused AppKit / WebKit bridges that Tauri does not expose directly.
//!
//! Everything here is on-demand: no timers, no observers left running.

use tauri::WebviewWindow;

/// Evaluate JavaScript in a webview and receive the (stringified) result.
///
/// The script should evaluate to a string (use `JSON.stringify(...)`); any
/// other result type is reported as `None`. The callback runs on the main
/// thread, so keep it short and hand heavy work to another thread.
pub fn eval_with_result<F>(window: &WebviewWindow, js: &str, callback: F)
where
    F: FnOnce(Option<String>) + Send + 'static,
{
    #[cfg(target_os = "macos")]
    {
        let js = js.to_string();
        let res = window.with_webview(move |wv| unsafe {
            use block2::RcBlock;
            use objc2::runtime::AnyObject;
            use objc2::{msg_send, ClassType};
            use objc2_foundation::NSString;
            use std::sync::Mutex;

            let webview = wv.inner() as *mut AnyObject;
            if webview.is_null() {
                callback(None);
                return;
            }
            let cb = Mutex::new(Some(callback));
            let block = RcBlock::new(move |result: *mut AnyObject, _err: *mut AnyObject| {
                let value = if result.is_null() {
                    None
                } else {
                    let is_string: bool = msg_send![result, isKindOfClass: NSString::class()];
                    if is_string {
                        Some((*(result as *const NSString)).to_string())
                    } else {
                        None
                    }
                };
                if let Some(f) = cb.lock().ok().and_then(|mut g| g.take()) {
                    f(value);
                }
            });
            let script = NSString::from_str(&js);
            let _: () = msg_send![webview, evaluateJavaScript: &*script, completionHandler: &*block];
        });
        if let Err(e) = res {
            log::warn!("eval_with_result: with_webview failed: {}", e);
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, js);
        callback(None);
    }
}

/// Blocking variant of [`eval_with_result`] for background threads.
/// Never call this from the main thread — it would deadlock.
pub fn eval_blocking(window: &WebviewWindow, js: &str, timeout: std::time::Duration) -> Option<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    eval_with_result(window, js, move |v| {
        let _ = tx.send(v);
    });
    rx.recv_timeout(timeout).ok().flatten()
}

/// Suspend or resume all media playback in a webview via WebKit's public API
/// (macOS 12+). Does not touch the page's JavaScript.
pub fn set_media_suspended(window: &WebviewWindow, suspended: bool) {
    #[cfg(target_os = "macos")]
    {
        let _ = window.with_webview(move |wv| unsafe {
            use objc2::msg_send;
            use objc2::runtime::{AnyObject, Bool, Sel};
            let webview = wv.inner() as *mut AnyObject;
            if webview.is_null() {
                return;
            }
            let sel = Sel::register(c"setAllMediaPlaybackSuspended:completionHandler:");
            let responds: bool = msg_send![webview, respondsToSelector: sel];
            if !responds {
                return;
            }
            let null_block: *const block2::Block<dyn Fn()> = std::ptr::null();
            let _: () = msg_send![webview, setAllMediaPlaybackSuspended: Bool::new(suspended), completionHandler: null_block];
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, suspended);
    }
}

/// Localized name of the frontmost application (the app the user was reading
/// in when a query was triggered). Our panels are non-activating, so this is
/// still the source app at trigger time.
pub fn frontmost_app_name() -> Option<String> {
    #[cfg(target_os = "macos")]
    unsafe {
        use objc2::msg_send;
        use objc2::runtime::{AnyClass, AnyObject};
        use objc2_foundation::NSString;
        let cls = AnyClass::get(c"NSWorkspace")?;
        let ws: *mut AnyObject = msg_send![cls, sharedWorkspace];
        if ws.is_null() {
            return None;
        }
        let app: *mut AnyObject = msg_send![ws, frontmostApplication];
        if app.is_null() {
            return None;
        }
        let name: *mut AnyObject = msg_send![app, localizedName];
        if name.is_null() {
            return None;
        }
        let s = (*(name as *const NSString)).to_string();
        if s == "Peekabrowser" {
            None
        } else {
            Some(s)
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Whether this process is trusted for Accessibility (needed for global key
/// monitoring). Never prompts.
pub fn accessibility_trusted() -> bool {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "ApplicationServices", kind = "framework")]
        extern "C" {
            fn AXIsProcessTrusted() -> bool;
        }
        unsafe { AXIsProcessTrusted() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// True when the system Liquid Glass view (macOS 26+) is available.
pub fn has_system_glass() -> bool {
    #[cfg(target_os = "macos")]
    {
        objc2::runtime::AnyClass::get(c"NSGlassEffectView").is_some()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Insert an `NSGlassEffectView` behind the window's web content.
/// Returns false when the class is unavailable (pre-macOS 26) so the caller
/// can fall back to `NSVisualEffectView` vibrancy instead of stacking both.
pub fn install_system_glass(window: &WebviewWindow, corner_radius: f64) -> bool {
    #[cfg(target_os = "macos")]
    unsafe {
        use objc2::msg_send;
        use objc2::runtime::{AnyClass, AnyObject};
        use objc2_foundation::{NSPoint, NSRect, NSSize};

        let Some(cls) = AnyClass::get(c"NSGlassEffectView") else {
            return false;
        };
        let Ok(ns_window) = window.ns_window() else {
            return false;
        };
        let ns_window = ns_window as *mut AnyObject;
        let content: *mut AnyObject = msg_send![ns_window, contentView];
        if content.is_null() {
            return false;
        }
        let bounds: NSRect = msg_send![content, bounds];
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(bounds.size.width, bounds.size.height));
        let glass: *mut AnyObject = msg_send![cls, alloc];
        let glass: *mut AnyObject = msg_send![glass, initWithFrame: frame];
        if glass.is_null() {
            return false;
        }
        // NSViewWidthSizable | NSViewHeightSizable
        let _: () = msg_send![glass, setAutoresizingMask: 18usize];
        let radius_sel = objc2::runtime::Sel::register(c"setCornerRadius:");
        let has_radius: bool = msg_send![glass, respondsToSelector: radius_sel];
        if has_radius {
            let _: () = msg_send![glass, setCornerRadius: corner_radius];
        }
        // NSWindowBelow = -1: keep the web view (and its controls) on top.
        let nil: *mut AnyObject = std::ptr::null_mut();
        let _: () = msg_send![content, addSubview: glass, positioned: -1isize, relativeTo: nil];
        let _: () = msg_send![glass, release];
        true
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, corner_radius);
        false
    }
}

/// Which corners of a window to round (combined as a bit set).
pub mod corners {
    // CACornerMask in AppKit's unflipped layer coordinates (MinY = bottom).
    pub const BOTTOM_LEFT: usize = 1;
    pub const BOTTOM_RIGHT: usize = 2;
    pub const TOP_LEFT: usize = 4;
    pub const TOP_RIGHT: usize = 8;
    pub const LEFT: usize = TOP_LEFT | BOTTOM_LEFT;
    pub const RIGHT: usize = TOP_RIGHT | BOTTOM_RIGHT;
    pub const ALL: usize = LEFT | RIGHT;
}

/// Continuous-curve rounded corners for a borderless window: the content view's
/// layer clips everything inside (web view and any material view), and the
/// window becomes non-opaque so the shadow follows the rounded shape.
pub fn set_rounded_corners(window: &WebviewWindow, radius: f64, mask: usize) {
    #[cfg(target_os = "macos")]
    unsafe {
        use objc2::msg_send;
        use objc2::runtime::{AnyClass, AnyObject, Bool, Sel};
        use objc2_foundation::NSString;

        let Ok(ns_window) = window.ns_window() else { return };
        let ns_window = ns_window as *mut AnyObject;
        let content: *mut AnyObject = msg_send![ns_window, contentView];
        if content.is_null() {
            return;
        }
        if let Some(color_cls) = AnyClass::get(c"NSColor") {
            let clear: *mut AnyObject = msg_send![color_cls, clearColor];
            let _: () = msg_send![ns_window, setOpaque: Bool::NO];
            let _: () = msg_send![ns_window, setBackgroundColor: clear];
        }
        let _: () = msg_send![content, setWantsLayer: Bool::YES];
        let layer: *mut AnyObject = msg_send![content, layer];
        if layer.is_null() {
            return;
        }
        let _: () = msg_send![layer, setCornerRadius: radius];
        let _: () = msg_send![layer, setMasksToBounds: Bool::YES];
        let _: () = msg_send![layer, setMaskedCorners: mask];
        // Continuous ("squircle") curve like system windows (macOS 10.15+ API).
        let curve_sel = Sel::register(c"setCornerCurve:");
        let has_curve: bool = msg_send![layer, respondsToSelector: curve_sel];
        if has_curve {
            let continuous = NSString::from_str("continuous");
            let _: () = msg_send![layer, setCornerCurve: &*continuous];
        }
        let _: () = msg_send![ns_window, setHasShadow: Bool::YES];
        let _: () = msg_send![ns_window, invalidateShadow];
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, radius, mask);
    }
}
