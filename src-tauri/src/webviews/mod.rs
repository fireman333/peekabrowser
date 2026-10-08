//! Logical pages (what the sidebar lists) and their binding to native viewer
//! slots (NSPanel + WKWebView). A page can outlive its slot: when unloaded it
//! keeps id, destination, URL, title and query link, and is restored on click.
//!
//! Pure bookkeeping — no Tauri types — so it is unit-testable.

use serde::Serialize;
use std::time::{Duration, Instant};

/// Max logical pages listed in the sidebar.
pub const MAX_PAGES: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PageState {
    /// Shown in the viewer (or the page that will be shown when the panel opens).
    Active,
    /// Loaded in a slot but not shown.
    Background,
    /// No slot; restorable from `url`.
    Unloaded,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageInfo {
    pub id: String,
    pub dest_id: String,
    pub dest_name: String,
    pub dest_icon: String,
    /// Native viewer slot label while loaded.
    pub label: Option<String>,
    pub state: PageState,
    /// True while a Peekabrowser-initiated generation is running here.
    pub generating: bool,
    pub title: String,
    pub url: String,
    pub query_id: Option<String>,
    /// When this page last stopped being visible (first time only — repeated
    /// hide/switch events never push the deadline back).
    #[serde(skip)]
    pub hidden_since: Option<Instant>,
}

pub struct WebViewTabManager {
    pub pages: Vec<PageInfo>,
    pub active_page_id: Option<String>,
    /// Whether the panel is on screen (the active page is visible only then).
    panel_visible: bool,
    next_page: u64,
    next_slot: u64,
}

impl Default for WebViewTabManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WebViewTabManager {
    pub fn new() -> Self {
        Self {
            pages: Vec::new(),
            active_page_id: None,
            panel_visible: false,
            next_page: 0,
            next_slot: 0,
        }
    }

    /// A fresh slot label for a brand-new native window.
    pub fn new_slot_label(&mut self) -> String {
        let l = format!("page-{}", self.next_slot);
        self.next_slot += 1;
        l
    }

    /// Add a page bound to `label`. If the list exceeds [`MAX_PAGES`], the
    /// oldest page that is neither active nor busy is evicted and returned so
    /// the caller can release its slot (no silent `remove(0)`).
    pub fn create_page(
        &mut self,
        dest_id: &str,
        dest_name: &str,
        dest_icon: &str,
        label: &str,
        url: &str,
        is_busy: impl Fn(&str) -> bool,
    ) -> (PageInfo, Option<PageInfo>) {
        let id = format!("p-{}", self.next_page);
        self.next_page += 1;
        let page = PageInfo {
            id,
            dest_id: dest_id.to_string(),
            dest_name: dest_name.to_string(),
            dest_icon: dest_icon.to_string(),
            label: Some(label.to_string()),
            state: PageState::Background,
            generating: false,
            title: String::new(),
            url: url.to_string(),
            query_id: None,
            hidden_since: Some(Instant::now()),
        };
        self.pages.push(page.clone());

        let mut evicted = None;
        if self.pages.len() > MAX_PAGES {
            let active = self.active_page_id.clone();
            if let Some(pos) = self.pages.iter().position(|p| {
                p.id != page.id && Some(&p.id) != active.as_ref() && !is_busy(&p.id)
            }) {
                evicted = Some(self.pages.remove(pos));
            }
        }
        (page, evicted)
    }

    /// Make `page_id` the active page. The previously active page becomes
    /// Background and starts its idle clock (only if not already running).
    pub fn set_active(&mut self, page_id: &str) {
        let now = Instant::now();
        let visible = self.panel_visible;
        for p in self.pages.iter_mut() {
            if p.id == page_id {
                if p.label.is_some() {
                    p.state = PageState::Active;
                }
                if visible {
                    p.hidden_since = None;
                }
            } else if p.state == PageState::Active {
                p.state = PageState::Background;
                p.hidden_since.get_or_insert(now);
            }
        }
        self.active_page_id = Some(page_id.to_string());
    }

    /// Panel hidden: every loaded page (including the active one) is now hidden.
    pub fn on_panel_hidden(&mut self) {
        self.panel_visible = false;
        let now = Instant::now();
        for p in self.pages.iter_mut().filter(|p| p.label.is_some()) {
            p.hidden_since.get_or_insert(now);
        }
    }

    /// Panel shown: only the active page becomes visible again.
    pub fn on_panel_shown(&mut self) {
        self.panel_visible = true;
        if let Some(id) = self.active_page_id.clone() {
            if let Some(p) = self.pages.iter_mut().find(|p| p.id == id) {
                p.hidden_since = None;
            }
        }
    }

    fn is_active(&self, page_id: &str) -> bool {
        self.active_page_id.as_deref() == Some(page_id)
    }

    pub fn panel_visible(&self) -> bool {
        self.panel_visible
    }

    /// Loaded background pages hidden for at least `max_age` and not protected
    /// by `is_busy`. The active page stays loaded so reopening the panel is instant.
    pub fn due_for_unload(&self, now: Instant, max_age: Duration, is_busy: impl Fn(&str) -> bool) -> Vec<String> {
        self.pages
            .iter()
            .filter(|p| p.label.is_some() && !self.is_active(&p.id))
            .filter(|p| matches!(p.hidden_since, Some(t) if now.duration_since(t) >= max_age))
            .filter(|p| !is_busy(&p.id))
            .map(|p| p.id.clone())
            .collect()
    }

    /// Time until the next loaded, hidden page becomes due (for one-shot scheduling).
    pub fn next_due_in(&self, now: Instant, max_age: Duration, is_busy: impl Fn(&str) -> bool) -> Option<Duration> {
        self.pages
            .iter()
            .filter(|p| p.label.is_some() && !self.is_active(&p.id) && !is_busy(&p.id))
            .filter_map(|p| p.hidden_since)
            .map(|t| (t + max_age).saturating_duration_since(now))
            .min()
    }

    /// Oldest hidden, loaded, non-busy page — the one to give up its slot when
    /// the live-slot cap is reached.
    pub fn oldest_reclaimable(&self, is_busy: impl Fn(&str) -> bool) -> Option<String> {
        self.pages
            .iter()
            .filter(|p| p.label.is_some() && p.hidden_since.is_some() && !self.is_active(&p.id) && !is_busy(&p.id))
            .min_by_key(|p| p.hidden_since)
            .map(|p| p.id.clone())
    }

    /// Detach a page from its slot, keeping its metadata. Returns the freed label.
    pub fn unload(&mut self, page_id: &str) -> Option<String> {
        let p = self.pages.iter_mut().find(|p| p.id == page_id)?;
        let label = p.label.take()?;
        p.state = PageState::Unloaded;
        p.generating = false;
        p.hidden_since = None;
        Some(label)
    }

    /// Bind a (restored) page to a slot.
    pub fn attach(&mut self, page_id: &str, label: &str) {
        if let Some(p) = self.pages.iter_mut().find(|p| p.id == page_id) {
            p.label = Some(label.to_string());
            p.state = if self.active_page_id.as_deref() == Some(page_id) {
                PageState::Active
            } else {
                PageState::Background
            };
        }
    }

    pub fn live_slot_count(&self) -> usize {
        self.pages.iter().filter(|p| p.label.is_some()).count()
    }

    pub fn update_meta(&mut self, page_id: &str, title: Option<&str>, url: Option<&str>) {
        if let Some(p) = self.pages.iter_mut().find(|p| p.id == page_id) {
            if let Some(t) = title {
                p.title = t.chars().take(200).collect();
            }
            if let Some(u) = url {
                if u != "about:blank" && !u.is_empty() {
                    p.url = u.to_string();
                }
            }
        }
    }

    pub fn set_query(&mut self, page_id: &str, query_id: &str) {
        if let Some(p) = self.pages.iter_mut().find(|p| p.id == page_id) {
            p.query_id = Some(query_id.to_string());
        }
    }

    pub fn set_generating(&mut self, page_id: &str, generating: bool) {
        if let Some(p) = self.pages.iter_mut().find(|p| p.id == page_id) {
            p.generating = generating;
        }
    }

    pub fn get_active_page(&self) -> Option<&PageInfo> {
        self.active_page_id
            .as_ref()
            .and_then(|id| self.pages.iter().find(|p| p.id == *id))
    }

    pub fn get_all_pages(&self) -> Vec<PageInfo> {
        self.pages.clone()
    }

    pub fn get_last_page_for_dest(&self, dest_id: &str) -> Option<&PageInfo> {
        self.pages.iter().rev().find(|p| p.dest_id == dest_id)
    }

    pub fn get_page(&self, page_id: &str) -> Option<&PageInfo> {
        self.pages.iter().find(|p| p.id == page_id)
    }

    pub fn page_id_for_label(&self, label: &str) -> Option<String> {
        self.pages
            .iter()
            .find(|p| p.label.as_deref() == Some(label))
            .map(|p| p.id.clone())
    }

    /// Remove a page; the active page falls back to the most recent one.
    pub fn remove_page(&mut self, page_id: &str) -> Option<PageInfo> {
        let pos = self.pages.iter().position(|p| p.id == page_id)?;
        let removed = self.pages.remove(pos);
        if self.active_page_id.as_deref() == Some(page_id) {
            self.active_page_id = self.pages.last().map(|p| p.id.clone());
            if let Some(id) = self.active_page_id.clone() {
                self.set_active(&id);
            }
        }
        Some(removed)
    }

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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mgr_with(n: usize) -> WebViewTabManager {
        let mut m = WebViewTabManager::new();
        for _ in 0..n {
            let l = m.new_slot_label();
            let (p, _) = m.create_page("d", "D", "", &l, "https://x", |_| false);
            m.set_active(&p.id);
        }
        m
    }

    #[test]
    fn eviction_returns_page_and_skips_active_and_busy() {
        let mut m = mgr_with(MAX_PAGES);
        // Make the oldest page busy: it must be skipped.
        let oldest = m.pages[0].id.clone();
        let second = m.pages[1].id.clone();
        let l = m.new_slot_label();
        let (_, evicted) = m.create_page("d", "D", "", &l, "https://x", |id| id == oldest);
        let evicted = evicted.expect("must evict");
        assert_eq!(evicted.id, second);
        assert!(evicted.label.is_some(), "caller receives the slot to release");
        assert_eq!(m.pages.len(), MAX_PAGES);
    }

    #[test]
    fn hidden_clock_is_not_reset_by_repeated_hides() {
        let mut m = mgr_with(2);
        m.on_panel_shown();
        let first = m.pages[0].id.clone();
        let t0 = m.get_page(&first).unwrap().hidden_since.unwrap();
        std::thread::sleep(Duration::from_millis(5));
        m.on_panel_hidden();
        m.on_panel_shown();
        let second = m.pages[1].id.clone();
        m.set_active(&second);
        assert_eq!(m.get_page(&first).unwrap().hidden_since.unwrap(), t0);
    }

    #[test]
    fn unload_keeps_metadata_and_restores() {
        let mut m = mgr_with(2);
        let first = m.pages[0].id.clone();
        m.update_meta(&first, Some("Title"), Some("https://chat/c/1"));
        m.set_query(&first, "q1");
        let due = m.due_for_unload(Instant::now() + Duration::from_secs(1000), Duration::from_secs(300), |_| false);
        assert!(due.contains(&first));
        let label = m.unload(&first).unwrap();
        let p = m.get_page(&first).unwrap();
        assert_eq!(p.state, PageState::Unloaded);
        assert_eq!(p.url, "https://chat/c/1");
        assert_eq!(p.query_id.as_deref(), Some("q1"));
        assert_eq!(m.live_slot_count(), 1);
        m.set_active(&first);
        m.attach(&first, &label);
        assert_eq!(m.get_page(&first).unwrap().state, PageState::Active);
    }

    #[test]
    fn busy_pages_are_protected() {
        let m = mgr_with(3);
        let busy = m.pages[0].id.clone();
        let due = m.due_for_unload(Instant::now() + Duration::from_secs(1000), Duration::from_secs(1), |id| id == busy);
        assert!(!due.contains(&busy));
        assert_ne!(m.oldest_reclaimable(|id| id == busy).as_deref(), Some(busy.as_str()));
    }

    #[test]
    fn active_page_is_never_due_even_when_hidden() {
        let mut m = mgr_with(2);
        m.on_panel_shown();
        let active = m.active_page_id.clone().unwrap();
        m.on_panel_hidden();
        let later = Instant::now() + Duration::from_secs(1000);
        let due = m.due_for_unload(later, Duration::from_secs(1), |_| false);
        assert!(!due.contains(&active));
        assert_eq!(due.len(), 1);
        assert_ne!(m.oldest_reclaimable(|_| false).as_deref(), Some(active.as_str()));
        // With only the active page loaded, nothing is scheduled.
        let other = m.pages[0].id.clone();
        m.unload(&other);
        assert!(m.next_due_in(later, Duration::from_secs(1), |_| false).is_none());
    }

    #[test]
    fn about_blank_does_not_overwrite_url() {
        let mut m = mgr_with(1);
        let id = m.pages[0].id.clone();
        m.update_meta(&id, None, Some("about:blank"));
        assert_eq!(m.get_page(&id).unwrap().url, "https://x");
    }
}
