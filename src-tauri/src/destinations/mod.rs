pub mod catalog;
pub mod defaults;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Destination {
    pub id: String,
    pub name: String,
    pub url: String,
    pub icon: String,
    pub order: usize,
    #[serde(default)]
    pub clip_prompt: String,
    /// Search address with `{q}`: the query opens the results page directly
    /// instead of being typed into the site.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_template: Option<String>,
    /// Catalogue entry this destination was added from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_id: Option<String>,
}

pub struct DestinationManager {
    pub destinations: Mutex<Vec<Destination>>,
    storage_path: PathBuf,
}

impl DestinationManager {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let storage_path = app_data_dir.join("destinations.json");
        let destinations = Self::load_from_file(&storage_path)
            .unwrap_or_else(|| defaults::default_destinations());
        Self {
            destinations: Mutex::new(destinations),
            storage_path,
        }
    }

    pub fn get_all(&self) -> Vec<Destination> {
        let mut dests = self.destinations.lock().unwrap().clone();
        dests.sort_by_key(|d| d.order);
        dests
    }

    pub fn add(&self, dest: Destination) {
        {
            let mut dests = self.destinations.lock().unwrap();
            dests.push(dest);
        }
        self.save();
    }

    pub fn remove(&self, id: &str) {
        {
            let mut dests = self.destinations.lock().unwrap();
            dests.retain(|d| d.id != id);
        }
        self.save();
    }

    pub fn reorder(&self, ordered_ids: Vec<String>) {
        {
            let mut dests = self.destinations.lock().unwrap();
            for (i, id) in ordered_ids.iter().enumerate() {
                if let Some(dest) = dests.iter_mut().find(|d| &d.id == id) {
                    dest.order = i;
                }
            }
        }
        self.save();
    }

    pub fn update(
        &self,
        id: &str,
        name: String,
        url: String,
        icon: String,
        clip_prompt: String,
    ) -> Option<Destination> {
        let updated = {
            let mut dests = self.destinations.lock().unwrap();
            if let Some(dest) = dests.iter_mut().find(|d| d.id == id) {
                dest.name = name;
                // A template stays unless the address itself changed; an
                // address with {q} becomes the new template.
                if url.contains("{q}") {
                    let (home, template) = catalog::split_custom(&url);
                    dest.url = home;
                    dest.url_template = template;
                } else {
                    if url != dest.url {
                        dest.url_template = None;
                    }
                    dest.url = url;
                }
                dest.icon = icon;
                dest.clip_prompt = clip_prompt;
                Some(dest.clone())
            } else {
                None
            }
        };
        if updated.is_some() {
            self.save();
        }
        updated
    }

    pub fn get_by_id(&self, id: &str) -> Option<Destination> {
        self.destinations
            .lock()
            .unwrap()
            .iter()
            .find(|d| d.id == id)
            .cloned()
    }

    /// Save destinations to JSON file
    fn save(&self) {
        if let Ok(dests) = self.destinations.lock() {
            if let Ok(json) = serde_json::to_string_pretty(&*dests) {
                // Ensure parent directory exists
                if let Some(parent) = self.storage_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&self.storage_path, json);
            }
        }
    }

    /// Load destinations from JSON file
    fn load_from_file(path: &PathBuf) -> Option<Vec<Destination>> {
        let data = std::fs::read_to_string(path).ok()?;
        let dests: Vec<Destination> = serde_json::from_str(&data).ok()?;
        if dests.is_empty() {
            None // Fall back to defaults if file is empty
        } else {
            Some(dests)
        }
    }
}
