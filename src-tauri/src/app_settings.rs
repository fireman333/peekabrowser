//! General app preferences, persisted next to destinations/shortcuts in
//! `~/Library/Application Support/com.peekabrowser.app/settings.json`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// Reveal the sidebar when the cursor rests on the left screen edge.
    /// Turning it off removes the global mouse monitor while the panel is hidden.
    pub edge_hover_enabled: bool,
    /// Use native window material (Liquid Glass on macOS 26+, vibrancy on 13–15)
    /// behind the sidebar and picker. Off = solid colors.
    pub native_material: bool,
    /// Seconds a background page may stay loaded before it is unloaded
    /// (metadata is kept so it can be restored with one click).
    pub background_unload_secs: u64,
    /// Legacy (≤2.0.6): ⌘C ⌘C sends straight to the first destination.
    /// Migrated into `text_from_other_apps = "fixed"`; kept so old files parse.
    pub auto_send_first: bool,
    /// Where text captured from other apps (⌘C ⌘C, three-finger tap) goes:
    /// "last_used" | "fixed" | "choose" (show the picker).
    pub text_from_other_apps: String,
    /// Destination id used when `text_from_other_apps == "fixed"`
    /// (None = the first destination).
    pub fixed_destination_id: Option<String>,
    /// Destination most recently picked; used by "last_used".
    pub last_used_destination_id: Option<String>,
    /// Press the site's send button after filling in the text.
    pub auto_submit: bool,
    /// Three-finger trackpad tap looks up the text under the cursor.
    pub three_finger_lookup: bool,
    /// Check GitHub Releases for a new version at launch and once a day.
    pub auto_check_updates: bool,
    /// Install found updates automatically (only while the panel is hidden
    /// and nothing is generating); otherwise just notify.
    pub auto_install_updates: bool,
    /// Interface language: "zh-TW" (default) or "en".
    pub language: String,
    /// The one-time permission cleanup for the switch to Developer ID signing
    /// (v2.0.5) has run.
    pub permissions_migrated: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            edge_hover_enabled: true,
            native_material: true,
            background_unload_secs: 300,
            auto_send_first: false,
            text_from_other_apps: "choose".to_string(),
            fixed_destination_id: None,
            last_used_destination_id: None,
            auto_submit: true,
            three_finger_lookup: false,
            auto_check_updates: true,
            auto_install_updates: false,
            language: "zh-TW".to_string(),
            permissions_migrated: false,
        }
    }
}

pub struct AppSettingsStore {
    settings: Mutex<AppSettings>,
    path: PathBuf,
}

impl AppSettingsStore {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let path = app_data_dir.join("settings.json");
        let raw = std::fs::read_to_string(&path).ok();
        let mut settings: AppSettings = raw
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        // Migrate ≤2.0.6 `auto_send_first` into the three-way choice.
        let had_new_key = raw.as_deref().is_some_and(|s| s.contains("\"text_from_other_apps\""));
        if !had_new_key && settings.auto_send_first {
            settings.text_from_other_apps = "fixed".to_string();
        }
        settings.text_from_other_apps = normalize_mode(&settings.text_from_other_apps).to_string();
        Self { settings: Mutex::new(settings), path }
    }

    pub fn get(&self) -> AppSettings {
        self.settings.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn update(&self, mut s: AppSettings) {
        s.background_unload_secs = s.background_unload_secs.clamp(60, 3600);
        s.language = crate::i18n::normalize(&s.language).to_string();
        s.text_from_other_apps = normalize_mode(&s.text_from_other_apps).to_string();
        s.auto_send_first = s.text_from_other_apps == "fixed";
        if let Ok(json) = serde_json::to_string_pretty(&s) {
            if let Some(parent) = self.path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&self.path, json);
        }
        *self.settings.lock().unwrap_or_else(|e| e.into_inner()) = s;
    }
}

/// Clamp an unknown "text from other apps" mode to "choose".
pub fn normalize_mode(m: &str) -> &'static str {
    match m {
        "last_used" => "last_used",
        "fixed" => "fixed",
        _ => "choose",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_mode_falls_back_to_choose() {
        assert_eq!(normalize_mode("fixed"), "fixed");
        assert_eq!(normalize_mode("last_used"), "last_used");
        assert_eq!(normalize_mode("bogus"), "choose");
    }

    #[test]
    fn legacy_auto_send_first_migrates_to_fixed() {
        let dir = std::env::temp_dir().join(format!("pb-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), r#"{"auto_send_first":true}"#).unwrap();
        let store = AppSettingsStore::new(dir.clone());
        assert_eq!(store.get().text_from_other_apps, "fixed");
        assert!(store.get().auto_submit);
        assert!(!store.get().three_finger_lookup);
        let _ = std::fs::remove_dir_all(dir);
    }
}
