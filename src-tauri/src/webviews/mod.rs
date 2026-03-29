use serde::Serialize;

/// Represents an open page (a webview tab)
#[derive(Clone, Serialize)]
pub struct PageInfo {
    pub id: String,
    pub dest_id: String,
    pub dest_name: String,
    pub dest_icon: String,
    pub label: String,
}

pub struct WebViewTabManager {
    pub pages: Vec<PageInfo>,
    pub active_page_id: Option<String>,
    next_index: u32,
}

impl WebViewTabManager {
    pub fn new() -> Self {
        Self {
            pages: Vec::new(),
            active_page_id: None,
            next_index: 0,
        }
    }

    /// Create a new page and return its info
    pub fn create_page(&mut self, dest_id: &str, dest_name: &str, dest_icon: &str) -> PageInfo {
        let idx = self.next_index;
        self.next_index += 1;
        let label = format!("page-{}", idx);
        self.create_page_with_label(dest_id, dest_name, dest_icon, &label)
    }

    /// Create a new page with a specific label (used when reusing a recycled window)
    pub fn create_page_with_label(
        &mut self,
        dest_id: &str,
        dest_name: &str,
        dest_icon: &str,
        label: &str,
    ) -> PageInfo {
        let page = PageInfo {
            id: label.to_string(),
            dest_id: dest_id.to_string(),
            dest_name: dest_name.to_string(),
            dest_icon: dest_icon.to_string(),
            label: label.to_string(),
        };

        self.pages.push(page.clone());

        // Limit to 20 pages max — remove oldest if exceeded
        if self.pages.len() > 20 {
            self.pages.remove(0);
        }

        page
    }

    /// Set the active page
    pub fn set_active(&mut self, page_id: &str) {
        self.active_page_id = Some(page_id.to_string());
    }

    /// Get the active page info
    pub fn get_active_page(&self) -> Option<&PageInfo> {
        self.active_page_id
            .as_ref()
            .and_then(|id| self.pages.iter().find(|p| p.id == *id))
    }

    /// Get all pages
    pub fn get_all_pages(&self) -> Vec<PageInfo> {
        self.pages.clone()
    }

    /// Get the last page for a destination (most recent)
    pub fn get_last_page_for_dest(&self, dest_id: &str) -> Option<&PageInfo> {
        self.pages.iter().rev().find(|p| p.dest_id == dest_id)
    }

    /// Remove a page by ID, returns removed page
    pub fn remove_page(&mut self, page_id: &str) -> Option<PageInfo> {
        if let Some(pos) = self.pages.iter().position(|p| p.id == page_id) {
            let removed = self.pages.remove(pos);
            if self.active_page_id.as_deref() == Some(page_id) {
                self.active_page_id = self.pages.last().map(|p| p.id.clone());
            }
            Some(removed)
        } else {
            None
        }
    }

    /// Remove all pages for a destination
    pub fn remove_pages_for_dest(&mut self, dest_id: &str) -> Vec<PageInfo> {
        let (removed, kept): (Vec<_>, Vec<_>) =
            self.pages.drain(..).partition(|p| p.dest_id == dest_id);
        self.pages = kept;
        if let Some(ref active_id) = self.active_page_id {
            if !self.pages.iter().any(|p| &p.id == active_id) {
                self.active_page_id = self.pages.last().map(|p| p.id.clone());
            }
        }
        removed
    }

    /// Get a page by ID
    pub fn get_page(&self, page_id: &str) -> Option<&PageInfo> {
        self.pages.iter().find(|p| p.id == page_id)
    }
}
