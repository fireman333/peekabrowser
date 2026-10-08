//! Check for and install new versions from GitHub Releases.
//!
//! Flow: query the latest release → pick the DMG for this Mac's architecture
//! → download over HTTPS → verify SHA-256 against the digest GitHub publishes
//! for the asset (falls back to the release's SHA256SUMS.txt) → mount, check
//! the bundle identifier and version → stage a copy → a small helper swaps the
//! app bundle after we quit and relaunches it.
//!
//! Uses the system `curl`, `shasum`, `hdiutil` and `ditto`; no extra crates.
//! Checks are one-shot (at launch, then once a day) — no polling.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub const REPO: &str = "fireman333/peekabrowser";
pub const BUNDLE_ID: &str = "com.peekabrowser.sidebar";
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, Serialize, Default)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub available: bool,
    pub notes: String,
    pub page_url: String,
    #[serde(skip)]
    pub asset_url: Option<String>,
    #[serde(skip)]
    pub asset_name: Option<String>,
    #[serde(skip)]
    pub digest: Option<String>,
    #[serde(skip)]
    pub sums_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct UpdateStatus {
    /// idle | checking | up_to_date | available | downloading | installing | error
    pub state: String,
    pub message: String,
    pub info: Option<UpdateInfo>,
}

static STATUS: Mutex<Option<UpdateStatus>> = Mutex::new(None);
static BUSY: AtomicBool = AtomicBool::new(false);

pub fn status() -> UpdateStatus {
    STATUS
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or(UpdateStatus { state: "idle".into(), ..Default::default() })
}

fn set_status(app: &AppHandle, state: &str, message: &str, info: Option<UpdateInfo>) {
    let info = info.or_else(|| status().info);
    let st = UpdateStatus { state: state.into(), message: message.into(), info };
    if let Ok(mut g) = STATUS.lock() {
        *g = Some(st.clone());
    }
    for label in [crate::panel::SIDEBAR_LABEL, "settings-window"] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.emit("update-status", &st);
        }
    }
}

// ─── Pure helpers (unit-tested) ─────────────────────────────────────────────

/// Parse "v1.2.3" / "1.2.3" (pre-release suffixes ignored) into a comparable tuple.
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim().trim_start_matches('v');
    let core = s.split(|c| c == '-' || c == '+').next()?;
    let mut it = core.split('.').map(|p| p.parse::<u64>());
    let major = it.next()?.ok()?;
    let minor = it.next().unwrap_or(Ok(0)).ok()?;
    let patch = it.next().unwrap_or(Ok(0)).ok()?;
    Some((major, minor, patch))
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// Architecture suffix used in DMG names by the release workflow.
pub fn arch_suffix() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x64"
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

/// Turn a GitHub "latest release" JSON document into update info.
pub fn info_from_release_json(json: &str, current: &str, arch: &str) -> Result<UpdateInfo, String> {
    let r: Release = serde_json::from_str(json).map_err(|e| format!("Unexpected release data: {}", e))?;
    if r.draft || r.prerelease {
        return Err("Latest release is not a stable release".into());
    }
    let latest = r.tag_name.trim_start_matches('v').to_string();
    let wanted = format!("Peekabrowser_{}_{}.dmg", latest, arch);
    let asset = r.assets.iter().find(|a| a.name == wanted);
    let sums = r.assets.iter().find(|a| a.name == "SHA256SUMS.txt");
    let trusted = |u: &str| u.starts_with(&format!("https://github.com/{}/releases/download/", REPO));
    Ok(UpdateInfo {
        current: current.to_string(),
        available: is_newer(&latest, current),
        latest,
        notes: r.body,
        page_url: r.html_url,
        asset_url: asset.map(|a| a.browser_download_url.clone()).filter(|u| trusted(u)),
        asset_name: asset.map(|a| a.name.clone()),
        digest: asset
            .and_then(|a| a.digest.clone())
            .and_then(|d| d.strip_prefix("sha256:").map(|h| h.to_lowercase())),
        sums_url: sums.map(|a| a.browser_download_url.clone()).filter(|u| trusted(u)),
    })
}

/// Find `name`'s hash in a `shasum -a 256` listing.
pub fn digest_from_sums(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut parts = l.split_whitespace();
        let hash = parts.next()?;
        let file = parts.next()?.trim_start_matches('*');
        (file == name && hash.len() == 64).then(|| hash.to_lowercase())
    })
}

/// The `.app` bundle containing `exe`, if it's a normal, replaceable install.
pub fn bundle_path_for(exe: &Path) -> Result<PathBuf, String> {
    let bundle = exe
        .ancestors()
        .find(|p| p.extension().map(|e| e == "app").unwrap_or(false))
        .ok_or("Not running from an app bundle")?
        .to_path_buf();
    let s = bundle.to_string_lossy();
    if s.contains("/AppTranslocation/") {
        return Err("macOS is running the app from a temporary location. Move Peekabrowser to Applications (or run Install.command) first.".into());
    }
    if s.starts_with("/Volumes/") {
        return Err("Peekabrowser is running from the disk image. Copy it to Applications first.".into());
    }
    Ok(bundle)
}

// ─── System calls ───────────────────────────────────────────────────────────

fn curl(args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("/usr/bin/curl")
        .args(["-fsSL", "--proto", "=https", "--max-time", "600", "-H", "User-Agent: Peekabrowser-Updater"])
        .args(args)
        .output()
        .map_err(|e| format!("curl failed to start: {}", e))?;
    if !out.status.success() {
        return Err(format!("Network error: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(out.stdout)
}

fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(cmd).args(args).output().map_err(|e| format!("{} failed: {}", cmd, e))?;
    if !out.status.success() {
        return Err(format!("{} failed: {}", cmd, String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn check(app: &AppHandle) -> Result<UpdateInfo, String> {
    set_status(app, "checking", "Checking for updates…", None);
    let url = format!("https://api.github.com/repos/{}/releases/latest", REPO);
    let body = curl(&["-H", "Accept: application/vnd.github+json", &url])?;
    let info = info_from_release_json(&String::from_utf8_lossy(&body), env!("CARGO_PKG_VERSION"), arch_suffix())?;
    if info.available {
        set_status(app, "available", &format!("Version {} is available", info.latest), Some(info.clone()));
    } else {
        set_status(app, "up_to_date", "Peekabrowser is up to date", Some(info.clone()));
    }
    Ok(info)
}

/// Download, verify, stage, then quit and let the helper swap and relaunch.
pub fn install(app: &AppHandle) -> Result<(), String> {
    if BUSY.swap(true, Ordering::SeqCst) {
        return Err("An update is already in progress".into());
    }
    let res = install_inner(app);
    BUSY.store(false, Ordering::SeqCst);
    if let Err(e) = &res {
        set_status(app, "error", e, None);
    }
    res
}

fn install_inner(app: &AppHandle) -> Result<(), String> {
    let info = match status().info.filter(|i| i.available) {
        Some(i) => i,
        None => check(app)?,
    };
    if !info.available {
        return Err("Already up to date".into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let bundle = bundle_path_for(&exe)?;
    let parent = bundle.parent().ok_or("Bad bundle path")?;
    let probe = parent.join(".peekabrowser-update-probe");
    std::fs::write(&probe, b"").map_err(|_| {
        format!("No permission to replace the app in {}. Download it from the release page instead.", parent.display())
    })?;
    let _ = std::fs::remove_file(&probe);

    let url = info.asset_url.clone().ok_or("This release has no download for your Mac")?;
    let name = info.asset_name.clone().unwrap_or_default();
    let expected = match info.digest.clone() {
        Some(d) => d,
        None => {
            let sums_url = info.sums_url.clone().ok_or("Release has no checksum to verify against")?;
            let sums = curl(&[&sums_url])?;
            digest_from_sums(&String::from_utf8_lossy(&sums), &name).ok_or("Checksum for the download not found")?
        }
    };

    let work = std::env::temp_dir().join(format!("peekabrowser-update-{}", info.latest));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let dmg = work.join(&name);
    let dmg_s = dmg.to_string_lossy().into_owned();

    set_status(app, "downloading", &format!("Downloading {}…", info.latest), None);
    curl(&["-o", &dmg_s, &url])?;

    let actual = run("/usr/bin/shasum", &["-a", "256", &dmg_s])?
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_lowercase();
    if actual != expected {
        let _ = std::fs::remove_dir_all(&work);
        return Err("Downloaded file failed checksum verification — update cancelled.".into());
    }

    set_status(app, "installing", "Preparing update…", None);
    let mnt = work.join("mnt");
    let mnt_s = mnt.to_string_lossy().into_owned();
    run("/usr/bin/hdiutil", &["attach", "-nobrowse", "-readonly", "-noautoopen", "-mountpoint", &mnt_s, &dmg_s])?;
    let staged = work.join("Peekabrowser.app");
    let staged_s = staged.to_string_lossy().into_owned();
    let copy = (|| {
        let src = mnt.join("Peekabrowser.app");
        let plist = src.join("Contents/Info.plist");
        let plist_s = plist.to_string_lossy().into_owned();
        let id = run("/usr/libexec/PlistBuddy", &["-c", "Print :CFBundleIdentifier", &plist_s])?;
        if id.trim() != BUNDLE_ID {
            return Err(format!("Unexpected app in update ({})", id.trim()));
        }
        let ver = run("/usr/libexec/PlistBuddy", &["-c", "Print :CFBundleShortVersionString", &plist_s])?;
        if ver.trim() != info.latest {
            return Err(format!("Update contains version {} instead of {}", ver.trim(), info.latest));
        }
        run("/usr/bin/ditto", &[&src.to_string_lossy(), &staged_s])?;
        Ok(())
    })();
    let _ = run("/usr/bin/hdiutil", &["detach", "-quiet", &mnt_s]);
    copy?;
    let _ = run("/usr/bin/xattr", &["-cr", &staged_s]);
    run("/usr/bin/codesign", &["--verify", "--deep", &staged_s]).map_err(|_| "Update failed its code-signature check".to_string())?;

    // Helper: wait for us to exit, swap bundles (keeping a backup until the
    // new one is in place), relaunch.
    let script = work.join("swap.sh");
    let backup = work.join("previous.app");
    let sh = format!(
        r#"#!/bin/sh
while kill -0 {pid} 2>/dev/null; do sleep 0.3; done
mv "{bundle}" "{backup}" || exit 1
if mv "{staged}" "{bundle}"; then
  rm -rf "{backup}"
else
  mv "{backup}" "{bundle}"
fi
open "{bundle}"
"#,
        pid = std::process::id(),
        bundle = bundle.display(),
        backup = backup.display(),
        staged = staged.display(),
    );
    std::fs::write(&script, sh).map_err(|e| e.to_string())?;
    Command::new("/bin/sh")
        .arg(&script)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Couldn't start the installer: {}", e))?;

    set_status(app, "installing", "Restarting into the new version…", None);
    log::info!("updater: restarting into {}", info.latest);
    std::thread::sleep(Duration::from_millis(300));
    std::process::exit(0);
}

fn settings(app: &AppHandle) -> crate::app_settings::AppSettings {
    app.try_state::<crate::app_settings::AppSettingsStore>()
        .map(|s| s.get())
        .unwrap_or_default()
}

/// One check shortly after launch, then once a day (single sleeping thread).
pub fn start_background_checks(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK_DELAY);
        loop {
            let s = settings(&app);
            if s.auto_check_updates {
                match check(&app) {
                    Ok(info) if info.available => {
                        let idle = crate::activity::active_count() == 0 && !crate::panel::is_panel_visible(&app);
                        if s.auto_install_updates && idle {
                            let _ = install(&app);
                        } else {
                            crate::lifecycle::notify(
                                &app,
                                &format!("Peekabrowser {} is available — open Settings to update.", info.latest),
                            );
                        }
                    }
                    Ok(_) => {}
                    Err(e) => log::info!("updater: check failed: {}", e),
                }
            }
            std::thread::sleep(CHECK_INTERVAL);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_ordering() {
        assert!(is_newer("v2.0.0", "1.9.0"));
        assert!(is_newer("1.10.0", "1.9.3"));
        assert!(!is_newer("v1.8.0", "1.8.0"));
        assert!(!is_newer("1.7.9", "1.8.0"));
        assert!(!is_newer("garbage", "1.0.0"));
        assert_eq!(parse_version("v2.1.0-beta.1"), Some((2, 1, 0)));
    }

    const JSON: &str = r#"{"tag_name":"v2.0.0","body":"notes","html_url":"https://github.com/fireman333/peekabrowser/releases/tag/v2.0.0","draft":false,"prerelease":false,
      "assets":[
        {"name":"Peekabrowser_2.0.0_aarch64.dmg","browser_download_url":"https://github.com/fireman333/peekabrowser/releases/download/v2.0.0/Peekabrowser_2.0.0_aarch64.dmg","digest":"sha256:ABCDEF"},
        {"name":"Peekabrowser_2.0.0_x64.dmg","browser_download_url":"https://evil.example/x.dmg"},
        {"name":"SHA256SUMS.txt","browser_download_url":"https://github.com/fireman333/peekabrowser/releases/download/v2.0.0/SHA256SUMS.txt"}]}"#;

    #[test]
    fn picks_asset_for_arch_and_digest() {
        let i = info_from_release_json(JSON, "1.9.0", "aarch64").unwrap();
        assert!(i.available);
        assert_eq!(i.latest, "2.0.0");
        assert_eq!(i.digest.as_deref(), Some("abcdef"));
        assert!(i.asset_url.unwrap().ends_with("_aarch64.dmg"));
    }

    #[test]
    fn rejects_untrusted_download_host() {
        let i = info_from_release_json(JSON, "1.9.0", "x64").unwrap();
        assert!(i.asset_url.is_none());
    }

    #[test]
    fn not_available_when_current() {
        let i = info_from_release_json(JSON, "2.0.0", "aarch64").unwrap();
        assert!(!i.available);
    }

    #[test]
    fn sums_lookup() {
        let h = "a".repeat(64);
        let sums = format!("{}  Peekabrowser_2.0.0_x64.dmg\n{}  other.dmg\n", h, "b".repeat(64));
        assert_eq!(digest_from_sums(&sums, "Peekabrowser_2.0.0_x64.dmg"), Some(h));
        assert_eq!(digest_from_sums(&sums, "missing.dmg"), None);
    }

    #[test]
    fn bundle_detection() {
        let p = Path::new("/Applications/Peekabrowser.app/Contents/MacOS/peekabrowser");
        assert_eq!(bundle_path_for(p).unwrap(), PathBuf::from("/Applications/Peekabrowser.app"));
        assert!(bundle_path_for(Path::new("/Volumes/Peekabrowser 2.0.0/Peekabrowser.app/Contents/MacOS/x")).is_err());
        assert!(bundle_path_for(Path::new("/private/var/folders/x/AppTranslocation/ABC/d/Peekabrowser.app/Contents/MacOS/x")).is_err());
        assert!(bundle_path_for(Path::new("/usr/local/bin/peekabrowser")).is_err());
    }
}
