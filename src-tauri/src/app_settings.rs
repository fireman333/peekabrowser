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
    /// ⌘C ⌘C sends straight to the first destination instead of showing the picker.
    pub auto_send_first: bool,
    /// Check GitHub Releases for a new version at launch and once a day.
    pub auto_check_updates: bool,
    /// Install found updates automatically (only while the panel is hidden
    /// and nothing is generating); otherwise just notify.
    pub auto_install_updates: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            edge_hover_enabled: true,
            native_material: true,
            background_unload_secs: 300,
            auto_send_first: false,
            auto_check_updates: true,
            auto_install_updates: false,
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
        let settings = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self { settings: Mutex::new(settings), path }
    }

    pub fn get(&self) -> AppSettings {
        self.settings.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn update(&self, mut s: AppSettings) {
        s.background_unload_secs = s.background_unload_secs.clamp(60, 3600);
        if let Ok(json) = serde_json::to_string_pretty(&s) {
            if let Some(parent) = self.path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&self.path, json);
        }
        *self.settings.lock().unwrap_or_else(|e| e.into_inner()) = s;
    }
}
