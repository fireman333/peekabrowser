//! Logical pages (what the sidebar lists) and their binding to native viewer
//! slots (NSPanel + WKWebView). A page can outlive its slot: when unloaded it
//! keeps id, destination, URL, title and query link, and is restored on click.
//!
//! Pure bookkeeping — no Tauri types — so it is unit-testable.

use serde::Serialize;
use std::time::{Duration, Instant};

/// Max tabs per destination (iPad tab model). Queries past the cap close the
/// least-recently-viewed eligible tab; manual new tabs are refused.
pub const MAX_PAGES_PER_DEST: usize = 20;
/// How many closed tabs can be reopened (⌘⇧T).
pub const REOPEN_STACK_SIZE: usize = 10;

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
    /// A generation finished while this page was not on screen.
    pub unseen: bool,
    /// Monotonic "last viewed" stamp (higher = more recent), for LRU choice.
    pub last_viewed: u64,
    /// When this page last stopped being visible (first time only — repeated
    /// hide/switch events never push the deadline back).
    #[serde(skip)]
    pub hidden_since: Option<Instant>,
}

/// A closed tab that can be reopened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosedPage {
    pub dest_id: String,
    pub url: String,
    pub title: String,
    pub query_id: Option<String>,
}

impl From<&PageInfo> for ClosedPage {
    fn from(p: &PageInfo) -> Self {
        Self { dest_id: p.dest_id.clone(), url: p.url.clone(), title: p.title.clone(), query_id: p.query_id.clone() }
    }
}

pub struct WebViewTabManager {
    pub pages: Vec<PageInfo>,
    /// Recently closed tabs, newest last.
    closed: Vec<ClosedPage>,
    view_clock: u64,
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
            closed: Vec::new(),
            view_clock: 0,
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

    /// Number of tabs open for a destination.
    pub fn count_for_dest(&self, dest_id: &str) -> usize {
        self.pages.iter().filter(|p| p.dest_id == dest_id).count()
    }

    /// Whether `page_id` is the only tab of its destination (it can't be closed).
    pub fn is_last_in_dest(&self, page_id: &str) -> bool {
        self.get_page(page_id).map(|p| self.count_for_dest(&p.dest_id) <= 1).unwrap_or(false)
    }

    /// Whether a manual new tab (＋ / ⌘T / ⌘N) is allowed for `dest_id`.
    pub fn can_open_manual(&self, dest_id: &str) -> bool {
        self.count_for_dest(dest_id) < MAX_PAGES_PER_DEST
    }

    /// Least-recently-viewed tab of `dest_id` that may be closed automatically:
    /// not active, not generating, not `except`, and not protected by `is_busy`.
    pub fn lru_evictable(&self, dest_id: &str, except: &str, is_busy: impl Fn(&str) -> bool) -> Option<String> {
        self.pages
            .iter()
            .filter(|p| p.dest_id == dest_id && p.id != except && !self.is_active(&p.id) && !p.generating && !is_busy(&p.id))
            .min_by_key(|p| p.last_viewed)
            .map(|p| p.id.clone())
    }

    /// Add a page bound to `label`. If its destination now has more than
    /// [`MAX_PAGES_PER_DEST`] tabs, the least-recently-viewed eligible tab is
    /// removed, pushed to the reopen stack and returned so the caller can
    /// release its slot.
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
        self.view_clock += 1;
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
            unseen: false,
            last_viewed: self.view_clock,
            hidden_since: Some(Instant::now()),
        };
        self.pages.push(page.clone());

        let mut evicted = None;
        if self.count_for_dest(dest_id) > MAX_PAGES_PER_DEST {
            if let Some(victim) = self.lru_evictable(dest_id, &page.id, is_busy) {
                if let Some(pos) = self.pages.iter().position(|p| p.id == victim) {
                    let removed = self.pages.remove(pos);
                    self.push_closed(&removed);
                    evicted = Some(removed);
                }
            }
        }
        (page, evicted)
    }

    /// Remember a closed tab for "reopen closed tab" (bounded stack).
    pub fn push_closed(&mut self, page: &PageInfo) {
        if page.url.is_empty() || page.url == "about:blank" {
            return;
        }
        self.push_closed_entry(ClosedPage::from(page));
    }

    /// Push an entry back (e.g. a reopen that was refused at the cap).
    pub fn push_closed_entry(&mut self, entry: ClosedPage) {
        self.closed.push(entry);
        if self.closed.len() > REOPEN_STACK_SIZE {
            let extra = self.closed.len() - REOPEN_STACK_SIZE;
            self.closed.drain(..extra);
        }
    }

    pub fn pop_closed(&mut self) -> Option<ClosedPage> {
        self.closed.pop()
    }

    pub fn closed_count(&self) -> usize {
        self.closed.len()
    }

    /// Tabs of the active page's destination, in order.
    pub fn active_dest_pages(&self) -> Vec<&PageInfo> {
        match self.get_active_page() {
            Some(a) => self.pages.iter().filter(|p| p.dest_id == a.dest_id).collect(),
            None => Vec::new(),
        }
    }

    /// The tab `delta` steps from the active one within its destination (wraps).
    pub fn cycle_target(&self, delta: i32) -> Option<String> {
        let tabs = self.active_dest_pages();
        if tabs.len() < 2 {
            return None;
        }
        let cur = tabs.iter().position(|p| self.is_active(&p.id))? as i32;
        let n = tabs.len() as i32;
        let idx = (cur + delta).rem_euclid(n) as usize;
        Some(tabs[idx].id.clone())
    }

    /// Ids of the other tabs in `page_id`'s destination (all, or only those to its right).
    pub fn sibling_ids(&self, page_id: &str, right_only: bool) -> Vec<String> {
        let Some(dest) = self.get_page(page_id).map(|p| p.dest_id.clone()) else { return Vec::new() };
        let tabs: Vec<&PageInfo> = self.pages.iter().filter(|p| p.dest_id == dest).collect();
        let pos = tabs.iter().position(|p| p.id == page_id).unwrap_or(0);
        tabs.iter()
            .enumerate()
            .filter(|(i, p)| p.id != page_id && (!right_only || *i > pos))
            .map(|(_, p)| p.id.clone())
            .collect()
    }

    /// Most recently viewed tab of a destination (destination icon click).
    pub fn last_viewed_for_dest(&self, dest_id: &str) -> Option<&PageInfo> {
        self.pages.iter().filter(|p| p.dest_id == dest_id).max_by_key(|p| p.last_viewed)
    }

    /// Make `page_id` the active page. The previously active page becomes
    /// Background and starts its idle clock (only if not already running).
    pub fn set_active(&mut self, page_id: &str) {
        let now = Instant::now();
        let visible = self.panel_visible;
        self.view_clock += 1;
        let clock = self.view_clock;
        for p in self.pages.iter_mut() {
            if p.id == page_id {
                if p.label.is_some() {
                    p.state = PageState::Active;
                }
                p.last_viewed = clock;
                if visible {
                    p.hidden_since = None;
                    p.unseen = false;
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
                p.unseen = false;
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
        let on_screen = self.panel_visible && self.is_active(page_id);
        if let Some(p) = self.pages.iter_mut().find(|p| p.id == page_id) {
            if p.generating && !generating && !on_screen {
                p.unseen = true;
            }
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

    pub fn get_page(&self, page_id: &str) -> Option<&PageInfo> {
        self.pages.iter().find(|p| p.id == page_id)
    }

    pub fn page_id_for_label(&self, label: &str) -> Option<String> {
        self.pages
            .iter()
            .find(|p| p.label.as_deref() == Some(label))
            .map(|p| p.id.clone())
    }

    /// Remove a page; when it was active, selection moves to its neighbour in
    /// the same destination (right, else left), else the most recently viewed tab.
    pub fn remove_page(&mut self, page_id: &str) -> Option<PageInfo> {
        let pos = self.pages.iter().position(|p| p.id == page_id)?;
        let dest = self.pages[pos].dest_id.clone();
        let removed = self.pages.remove(pos);
        if self.active_page_id.as_deref() == Some(page_id) {
            let same_dest = self.pages[pos..]
                .iter()
                .find(|p| p.dest_id == dest)
                .or_else(|| self.pages[..pos].iter().rev().find(|p| p.dest_id == dest))
                .map(|p| p.id.clone());
            self.active_page_id = same_dest
                .or_else(|| self.pages.iter().max_by_key(|p| p.last_viewed).map(|p| p.id.clone()));
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
                self.active_page_id = self.pages.iter().max_by_key(|p| p.last_viewed).map(|p| p.id.clone());
                if let Some(id) = self.active_page_id.clone() {
                    self.set_active(&id);
                }
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

    fn open(m: &mut WebViewTabManager, dest: &str) -> String {
        let l = m.new_slot_label();
        let (p, ev) = m.create_page(dest, "D", "", &l, &format!("https://{}/{}", dest, l), |_| false);
        assert!(ev.is_none());
        p.id
    }

    #[test]
    fn eviction_returns_page_and_skips_active_and_busy() {
        let mut m = mgr_with(MAX_PAGES_PER_DEST);
        // Make the oldest page busy: it must be skipped.
        let oldest = m.pages[0].id.clone();
        let second = m.pages[1].id.clone();
        let l = m.new_slot_label();
        let (_, evicted) = m.create_page("d", "D", "", &l, "https://x", |id| id == oldest);
        let evicted = evicted.expect("must evict");
        assert_eq!(evicted.id, second);
        assert!(evicted.label.is_some(), "caller receives the slot to release");
        assert_eq!(m.pages.len(), MAX_PAGES_PER_DEST);
        assert_eq!(m.closed_count(), 1, "evicted tab is reopenable");
    }

    #[test]
    fn cap_is_per_destination() {
        let mut m = mgr_with(MAX_PAGES_PER_DEST);
        assert!(!m.can_open_manual("d"));
        assert!(m.can_open_manual("other"));
        for _ in 0..5 {
            open(&mut m, "other");
        }
        assert_eq!(m.count_for_dest("d"), MAX_PAGES_PER_DEST);
        assert_eq!(m.count_for_dest("other"), 5);
        assert_eq!(m.closed_count(), 0);
    }

    #[test]
    fn eviction_picks_least_recently_viewed_not_oldest() {
        let mut m = mgr_with(MAX_PAGES_PER_DEST);
        let first = m.pages[0].id.clone();
        let second = m.pages[1].id.clone();
        let third = m.pages[2].id.clone();
        // Viewing the first tab again makes the second one the LRU.
        m.set_active(&first);
        let last = m.pages.last().unwrap().id.clone();
        m.set_active(&last);
        // Generating tabs are never auto-closed.
        m.set_generating(&second, true);
        let l = m.new_slot_label();
        let (_, evicted) = m.create_page("d", "D", "", &l, "https://y", |_| false);
        assert_eq!(evicted.unwrap().id, third);
    }

    #[test]
    fn reopen_stack_is_bounded_and_lifo() {
        let mut m = WebViewTabManager::new();
        let mut ids = Vec::new();
        for _ in 0..(REOPEN_STACK_SIZE + 3) {
            ids.push(open(&mut m, "d"));
        }
        for id in &ids {
            let p = m.remove_page(id).unwrap();
            m.push_closed(&p);
        }
        assert_eq!(m.closed_count(), REOPEN_STACK_SIZE);
        let top = m.pop_closed().unwrap();
        assert!(top.url.ends_with(&format!("page-{}", REOPEN_STACK_SIZE + 2)));
        assert_eq!(top.dest_id, "d");
        assert_eq!(m.closed_count(), REOPEN_STACK_SIZE - 1);
    }

    #[test]
    fn last_tab_of_destination_is_protected_and_blank_tabs_are_not_reopenable() {
        let mut m = WebViewTabManager::new();
        let a1 = open(&mut m, "a");
        let b1 = open(&mut m, "b");
        assert!(m.is_last_in_dest(&a1));
        let a2 = open(&mut m, "a");
        assert!(!m.is_last_in_dest(&a1));
        assert!(m.is_last_in_dest(&b1));
        let mut blank = m.get_page(&a2).unwrap().clone();
        blank.url = "about:blank".into();
        m.push_closed(&blank);
        assert_eq!(m.closed_count(), 0);
        // A refused reopen goes back on top of the stack.
        let p = m.remove_page(&a2).unwrap();
        m.push_closed(&p);
        let top = m.pop_closed().unwrap();
        m.push_closed_entry(top.clone());
        assert_eq!(m.pop_closed(), Some(top));
    }

    #[test]
    fn closing_active_selects_neighbour_in_same_destination() {
        let mut m = WebViewTabManager::new();
        let a1 = open(&mut m, "a");
        let b1 = open(&mut m, "b");
        let a2 = open(&mut m, "a");
        let a3 = open(&mut m, "a");
        m.set_active(&b1);
        m.set_active(&a2);
        m.remove_page(&a2);
        assert_eq!(m.active_page_id.as_deref(), Some(a3.as_str()));
        m.remove_page(&a3);
        assert_eq!(m.active_page_id.as_deref(), Some(a1.as_str()));
        m.remove_page(&a1);
        assert_eq!(m.active_page_id.as_deref(), Some(b1.as_str()));
    }

    #[test]
    fn cycling_and_siblings_stay_in_destination() {
        let mut m = WebViewTabManager::new();
        let a1 = open(&mut m, "a");
        let _b = open(&mut m, "b");
        let a2 = open(&mut m, "a");
        let a3 = open(&mut m, "a");
        m.set_active(&a3);
        assert_eq!(m.cycle_target(1).as_deref(), Some(a1.as_str()));
        assert_eq!(m.cycle_target(-1).as_deref(), Some(a2.as_str()));
        assert_eq!(m.sibling_ids(&a2, true), vec![a3.clone()]);
        assert_eq!(m.sibling_ids(&a2, false), vec![a1.clone(), a3.clone()]);
        assert_eq!(m.last_viewed_for_dest("a").map(|p| p.id.clone()), Some(a3));
    }

    #[test]
    fn finished_generation_off_screen_is_unseen() {
        let mut m = mgr_with(2);
        m.on_panel_shown();
        let bg = m.pages[0].id.clone();
        m.set_generating(&bg, true);
        m.set_generating(&bg, false);
        assert!(m.get_page(&bg).unwrap().unseen);
        m.set_active(&bg);
        assert!(!m.get_page(&bg).unwrap().unseen);
        // Finishing on screen is not "unseen".
        m.set_generating(&bg, true);
        m.set_generating(&bg, false);
        assert!(!m.get_page(&bg).unwrap().unseen);
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
