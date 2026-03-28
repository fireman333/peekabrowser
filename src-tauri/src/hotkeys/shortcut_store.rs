use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

/// Persisted shortcut configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutConfig {
    pub toggle_sidebar: String,
    pub screenshot: String,
    pub export: String,
}

impl Default for ShortcutConfig {
    fn default() -> Self {
        Self {
            toggle_sidebar: "Command+Shift+A".to_string(),
            screenshot: "Command+Shift+S".to_string(),
            export: "Command+Shift+E".to_string(),
        }
    }
}

pub struct ShortcutStore {
    config: Mutex<ShortcutConfig>,
    storage_path: PathBuf,
}

impl ShortcutStore {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let storage_path = app_data_dir.join("shortcuts.json");
        let config = Self::load_from_file(&storage_path).unwrap_or_default();
        Self {
            config: Mutex::new(config),
            storage_path,
        }
    }

    pub fn get(&self) -> ShortcutConfig {
        self.config.lock().unwrap().clone()
    }

    pub fn update(&self, config: ShortcutConfig) {
        *self.config.lock().unwrap() = config;
        self.save();
    }

    fn save(&self) {
        if let Ok(cfg) = self.config.lock() {
            if let Ok(json) = serde_json::to_string_pretty(&*cfg) {
                if let Some(parent) = self.storage_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&self.storage_path, json);
            }
        }
    }

    fn load_from_file(path: &PathBuf) -> Option<ShortcutConfig> {
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }
}

/// Parse a human-readable shortcut string like "Command+Shift+A" into Tauri types
pub fn parse_shortcut(
    s: &str,
) -> Option<(
    Option<tauri_plugin_global_shortcut::Modifiers>,
    tauri_plugin_global_shortcut::Code,
)> {
    use tauri_plugin_global_shortcut::{Code, Modifiers};

    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    if parts.is_empty() {
        return None;
    }

    let mut mods = Modifiers::empty();
    let key_part = parts.last()?;

    for &part in &parts[..parts.len() - 1] {
        match part.to_lowercase().as_str() {
            "command" | "cmd" | "super" | "⌘" => mods |= Modifiers::SUPER,
            "shift" | "⇧" => mods |= Modifiers::SHIFT,
            "alt" | "option" | "opt" | "⌥" => mods |= Modifiers::ALT,
            "control" | "ctrl" | "⌃" => mods |= Modifiers::CONTROL,
            _ => {}
        }
    }

    let code = match key_part.to_uppercase().as_str() {
        "A" => Code::KeyA,
        "B" => Code::KeyB,
        "C" => Code::KeyC,
        "D" => Code::KeyD,
        "E" => Code::KeyE,
        "F" => Code::KeyF,
        "G" => Code::KeyG,
        "H" => Code::KeyH,
        "I" => Code::KeyI,
        "J" => Code::KeyJ,
        "K" => Code::KeyK,
        "L" => Code::KeyL,
        "M" => Code::KeyM,
        "N" => Code::KeyN,
        "O" => Code::KeyO,
        "P" => Code::KeyP,
        "Q" => Code::KeyQ,
        "R" => Code::KeyR,
        "S" => Code::KeyS,
        "T" => Code::KeyT,
        "U" => Code::KeyU,
        "V" => Code::KeyV,
        "W" => Code::KeyW,
        "X" => Code::KeyX,
        "Y" => Code::KeyY,
        "Z" => Code::KeyZ,
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        "F1" => Code::F1,
        "F2" => Code::F2,
        "F3" => Code::F3,
        "F4" => Code::F4,
        "F5" => Code::F5,
        "F6" => Code::F6,
        "F7" => Code::F7,
        "F8" => Code::F8,
        "F9" => Code::F9,
        "F10" => Code::F10,
        "F11" => Code::F11,
        "F12" => Code::F12,
        "SPACE" | " " => Code::Space,
        "ENTER" | "RETURN" => Code::Enter,
        "TAB" => Code::Tab,
        "ESCAPE" | "ESC" => Code::Escape,
        "BACKSPACE" => Code::Backspace,
        "DELETE" => Code::Delete,
        "UP" | "ARROWUP" => Code::ArrowUp,
        "DOWN" | "ARROWDOWN" => Code::ArrowDown,
        "LEFT" | "ARROWLEFT" => Code::ArrowLeft,
        "RIGHT" | "ARROWRIGHT" => Code::ArrowRight,
        _ => return None,
    };

    let mod_opt = if mods.is_empty() { None } else { Some(mods) };
    Some((mod_opt, code))
}
