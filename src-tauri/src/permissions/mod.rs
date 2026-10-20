/// Check if Accessibility permission is granted (needed for CGEventTap)
pub fn has_accessibility_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let result = Command::new("osascript")
            .args(["-e", "tell application \"System Events\" to keystroke \"\""])
            .output();
        result.map(|o| o.status.success()).unwrap_or(false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Check if Screen Recording permission is granted
pub fn has_screen_capture_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        // CGPreflightScreenCaptureAccess() returns true if permission is already granted
        extern "C" {
            fn CGPreflightScreenCaptureAccess() -> bool;
        }
        unsafe { CGPreflightScreenCaptureAccess() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Request Screen Recording permission (shows system dialog if not granted)
pub fn request_screen_capture_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        extern "C" {
            fn CGRequestScreenCaptureAccess() -> bool;
        }
        unsafe { CGRequestScreenCaptureAccess() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Open the Accessibility settings pane
pub fn open_accessibility_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();
    }
}

/// Open the Screen Recording settings pane
pub fn open_screen_recording_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            .spawn();
    }
}

/// Drop this app's TCC entry for `service` so macOS asks again. An entry left by
/// an older signing identity (ad-hoc builds before v2.0.5) still looks checked in
/// System Settings but no longer matches the app, so the grant silently fails.
fn reset_tcc(service: &str, bundle_id: &str) {
    match std::process::Command::new("/usr/bin/tccutil")
        .args(["reset", service, bundle_id])
        .output()
    {
        Ok(o) if o.status.success() => log::info!("tcc: reset {} for {}", service, bundle_id),
        Ok(o) => log::warn!("tcc: reset {} failed: {}", service, String::from_utf8_lossy(&o.stderr).trim()),
        Err(e) => log::warn!("tcc: tccutil failed: {}", e),
    }
}

/// Ask for Accessibility again: clear a stale entry, show the system prompt and
/// open the pane, so the user only has to switch Peekabrowser on.
/// No-op while the grant works, so a valid entry is never wiped.
pub fn regrant_accessibility(bundle_id: &str) {
    if crate::native::accessibility_trusted() {
        return;
    }
    reset_tcc("Accessibility", bundle_id);
    crate::native::prompt_accessibility();
    open_accessibility_settings();
}

/// Clear a stale Screen Recording entry without prompting; the next screenshot
/// makes macOS ask again. No-op while the grant works.
pub fn clear_stale_screen_capture(bundle_id: &str) {
    if !has_screen_capture_permission() {
        reset_tcc("ScreenCapture", bundle_id);
    }
}
