//! Single entry point for page lifecycle: create, reuse, switch, hide, unload,
//! restore and close all go through here, so the sidebar list, the native
//! viewer slots and the idle bookkeeping can't drift apart.
//!
//! Resource rules:
//! - At most [`MAX_LIVE_SLOTS`] pages are loaded at once (busy pages excepted).
//!   Opening another reuses a pooled slot or reclaims the oldest hidden page.
//! - Native windows are never destroyed (NSPanel teardown aborts in tao, see
//!   `panel::recycle_slot`); freed slots go to a pool on about:blank and are
//!   reused before any new window is created, so the window count is bounded
//!   by the highest number of simultaneously loaded pages.
//! - Hidden pages are unloaded after the configured idle time by a one-shot
//!   scheduled check (no resident polling). Generating pages and pages with an
//!   unsent draft are kept.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

use crate::destinations::Destination;
use crate::webviews::{PageInfo, WebViewTabManager};

pub const MAX_LIVE_SLOTS: usize = 6;

type Mgr = Mutex<WebViewTabManager>;

/// Pages whose last observed state had an unsent draft.
static DRAFTS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static MAINTENANCE_SCHEDULED: AtomicBool = AtomicBool::new(false);
static MAINTENANCE_WAKE: (Mutex<bool>, std::sync::Condvar) = (Mutex::new(false), std::sync::Condvar::new());

fn has_draft(page_id: &str) -> bool {
    DRAFTS.lock().map(|d| d.iter().any(|p| p == page_id)).unwrap_or(false)
}

fn set_draft(page_id: &str, draft: bool) {
    if let Ok(mut d) = DRAFTS.lock() {
        d.retain(|p| p != page_id);
        if draft {
            d.push(page_id.to_string());
        }
    }
}

/// Protected from unloading/reclaiming.
pub fn is_busy(page_id: &str) -> bool {
    crate::activity::page_is_busy(page_id) || has_draft(page_id)
}

fn unload_after(app: &AppHandle) -> Duration {
    let secs = app
        .try_state::<crate::app_settings::AppSettingsStore>()
        .map(|s| s.get().background_unload_secs)
        .unwrap_or(300);
    Duration::from_secs(secs)
}

pub fn emit_pages(app: &AppHandle, mgr: &WebViewTabManager) {
    if let Some(sidebar) = app.get_webview_window(crate::panel::SIDEBAR_LABEL) {
        let _ = sidebar.emit("pages-updated", mgr.get_all_pages());
        let _ = sidebar.emit("active-page-changed", mgr.active_page_id.clone().unwrap_or_default());
    }
}

fn emit_now(app: &AppHandle) {
    if let Ok(mgr) = app.state::<Mgr>().lock() {
        emit_pages(app, &mgr);
    }
}

/// Short, recoverable status message: in the sidebar, or as a system
/// notification when the panel is hidden (e.g. ⌘⇧E from another app).
pub fn notify(app: &AppHandle, msg: &str) {
    if let Some(sidebar) = app.get_webview_window(crate::panel::SIDEBAR_LABEL) {
        let _ = sidebar.emit("notice", msg);
    }
    let visible = app.state::<Mgr>().lock().map(|m| m.panel_visible()).unwrap_or(false);
    if !visible {
        let script = format!(
            "display notification \"{}\" with title \"Peekabrowser\"",
            msg.replace('\\', "\\\\").replace('"', "\\\"")
        );
        std::thread::spawn(move || {
            let _ = std::process::Command::new("osascript").arg("-e").arg(script).output();
        });
    }
}

/// Get a viewer slot showing `url`: pooled slot → reclaimed hidden page → new window.
fn acquire_slot(app: &AppHandle, url: &str) -> Result<String, String> {
    if let Some(label) = crate::panel::pop_recycled_label() {
        crate::panel::navigate_slot(app, &label, url).map_err(|e| e.to_string())?;
        return Ok(label);
    }
    let reclaim = {
        let mgr = app.state::<Mgr>();
        let mgr = mgr.lock().map_err(|_| "lock")?;
        if mgr.live_slot_count() >= MAX_LIVE_SLOTS {
            mgr.oldest_reclaimable(is_busy)
        } else {
            None
        }
    };
    if let Some(pid) = reclaim {
        log::info!("lifecycle: slot cap reached, unloading {}", pid);
        unload_page(app, &pid);
        if let Some(label) = crate::panel::pop_recycled_label() {
            crate::panel::navigate_slot(app, &label, url).map_err(|e| e.to_string())?;
            return Ok(label);
        }
    }
    let label = app.state::<Mgr>().lock().map_err(|_| "lock")?.new_slot_label();
    crate::panel::create_viewer_window(app, &label, url).map_err(|e| e.to_string())?;
    Ok(label)
}

/// Open a new page for a destination and show it.
pub fn open_new_page(app: &AppHandle, dest: &Destination) -> Result<PageInfo, String> {
    let label = acquire_slot(app, &dest.url)?;
    let (page, evicted) = {
        let mgr = app.state::<Mgr>();
        let mut mgr = mgr.lock().map_err(|_| "lock")?;
        let (page, evicted) = mgr.create_page(&dest.id, &dest.name, &dest.icon, &label, &dest.url, is_busy);
        mgr.set_active(&page.id);
        (page, evicted)
    };
    if let Some(ev) = evicted {
        log::info!("lifecycle: page limit reached, removing {}", ev.id);
        set_draft(&ev.id, false);
        if let Some(l) = ev.label {
            release_slot(app, &ev.id, &l);
        }
    }
    crate::panel::show_page_viewer(app, &label);
    remember_hidden_state(app);
    emit_now(app);
    ensure_maintenance(app);
    Ok(page)
}

/// Show an existing page, restoring it first if it was unloaded.
pub fn activate_page(app: &AppHandle, page_id: &str) -> Result<(), String> {
    let (label, url) = {
        let mgr = app.state::<Mgr>();
        let mgr = mgr.lock().map_err(|_| "lock")?;
        let p = mgr.get_page(page_id).ok_or("page not found")?;
        (p.label.clone(), p.url.clone())
    };
    let label = match label {
        Some(l) => l,
        None => {
            let l = acquire_slot(app, &url)?;
            app.state::<Mgr>().lock().map_err(|_| "lock")?.attach(page_id, &l);
            l
        }
    };
    app.state::<Mgr>().lock().map_err(|_| "lock")?.set_active(page_id);
    crate::panel::show_page_viewer(app, &label);
    remember_hidden_state(app);
    emit_now(app);
    ensure_maintenance(app);
    Ok(())
}

/// Sidebar destination click: last page for it, or a new one.
pub fn switch_destination(app: &AppHandle, dest: &Destination) -> Result<(), String> {
    let existing = app
        .state::<Mgr>()
        .lock()
        .map_err(|_| "lock")?
        .get_last_page_for_dest(&dest.id)
        .map(|p| p.id.clone());
    match existing {
        Some(id) => activate_page(app, &id),
        None => open_new_page(app, dest).map(|_| ()),
    }
}

/// Close a page by id. A viewer slot label (`page-N`, sent by the in-page
/// ⌘W handler) is accepted too and resolved to the page it currently shows.
pub fn close_page(app: &AppHandle, page_id: &str) {
    let (removed, next) = {
        let mgr = app.state::<Mgr>();
        let Ok(mut mgr) = mgr.lock() else { return };
        let page_id = if mgr.get_page(page_id).is_some() {
            page_id.to_string()
        } else {
            match mgr.page_id_for_label(page_id) {
                Some(id) => id,
                None => return,
            }
        };
        let page_id = page_id.as_str();
        let removed = mgr.remove_page(page_id);
        (removed, mgr.active_page_id.clone())
    };
    if let Some(p) = removed {
        set_draft(&p.id, false);
        if let Some(l) = p.label {
            release_slot(app, &p.id, &l);
        }
    }
    match next {
        Some(id) => {
            let _ = activate_page(app, &id);
        }
        None => emit_now(app),
    }
}

pub fn close_page_by_label(app: &AppHandle, label: &str) {
    let id = app.state::<Mgr>().lock().ok().and_then(|m| m.page_id_for_label(label));
    if let Some(id) = id {
        close_page(app, &id);
    }
}

pub fn remove_pages_for_dest(app: &AppHandle, dest_id: &str) {
    let (removed, next) = {
        let mgr = app.state::<Mgr>();
        let Ok(mut mgr) = mgr.lock() else { return };
        let had_active = mgr.get_active_page().map(|p| p.dest_id == dest_id).unwrap_or(false);
        let removed = mgr.remove_pages_for_dest(dest_id);
        (removed, if had_active { mgr.active_page_id.clone() } else { None })
    };
    for p in removed {
        set_draft(&p.id, false);
        if let Some(l) = p.label {
            release_slot(app, &p.id, &l);
        }
    }
    match next {
        Some(id) => {
            let _ = activate_page(app, &id);
        }
        None => emit_now(app),
    }
}

/// Return a slot to the pool and end any work tied to the page.
fn release_slot(app: &AppHandle, page_id: &str, label: &str) {
    crate::activity::end_for_page(page_id);
    crate::panel::recycle_slot(app, label);
}

/// Detach a page from its slot, keeping URL/title/query so it can be restored.
pub fn unload_page(app: &AppHandle, page_id: &str) {
    // Capture the latest URL synchronously (cheap, no JS).
    let label = app.state::<Mgr>().lock().ok().and_then(|m| m.get_page(page_id).and_then(|p| p.label.clone()));
    let Some(label) = label else { return };
    let url = app.get_webview_window(&label).and_then(|w| w.url().ok()).map(|u| u.to_string());
    let freed = {
        let mgr = app.state::<Mgr>();
        let Ok(mut mgr) = mgr.lock() else { return };
        mgr.update_meta(page_id, None, url.as_deref());
        mgr.unload(page_id)
    };
    if let Some(l) = freed {
        log::info!("lifecycle: unloaded {} (slot {})", page_id, l);
        release_slot(app, page_id, &l);
    }
    emit_now(app);
}

/// Unload only if the page is still hidden, not active, and in the same slot
/// as when the maintenance check decided to unload it.
fn unload_if_still_idle(app: &AppHandle, page_id: &str, expected_label: &str) {
    let ok = app
        .state::<Mgr>()
        .lock()
        .map(|m| {
            m.active_page_id.as_deref() != Some(page_id)
                && m.get_page(page_id)
                    .map(|p| p.label.as_deref() == Some(expected_label) && p.hidden_since.is_some())
                    .unwrap_or(false)
        })
        .unwrap_or(false);
    if ok && !is_busy(page_id) {
        unload_page(app, page_id);
    }
}

pub fn on_panel_hidden(app: &AppHandle) {
    if let Ok(mut m) = app.state::<Mgr>().lock() {
        m.on_panel_hidden();
    }
    remember_hidden_state(app);
    ensure_maintenance(app);
}

pub fn on_panel_shown(app: &AppHandle) {
    if let Ok(mut m) = app.state::<Mgr>().lock() {
        m.on_panel_shown();
    }
}

/// Asynchronously record title/URL/draft of hidden, loaded pages (used for the
/// sidebar list and to protect drafts from being unloaded).
fn remember_hidden_state(app: &AppHandle) {
    let targets: Vec<(String, String)> = app
        .state::<Mgr>()
        .lock()
        .map(|m| {
            m.pages
                .iter()
                .filter(|p| p.hidden_since.is_some())
                .filter_map(|p| p.label.clone().map(|l| (p.id.clone(), l)))
                .collect()
        })
        .unwrap_or_default();
    if targets.is_empty() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        for (pid, label) in targets {
            if let Some(st) = crate::delivery::page_status(&app, &label) {
                set_draft(&pid, st.draft && !st.generating);
                if let Ok(mut m) = app.state::<Mgr>().lock() {
                    if m.get_page(&pid).and_then(|p| p.label.as_deref()) == Some(label.as_str()) {
                        m.update_meta(&pid, Some(&st.title), Some(&st.url));
                    }
                }
            }
        }
        emit_now(&app);
    });
}

pub fn set_page_generating(app: &AppHandle, page_id: &str, generating: bool, title: Option<&str>, url: Option<&str>) {
    if let Ok(mut m) = app.state::<Mgr>().lock() {
        let changed = m.get_page(page_id).map(|p| p.generating != generating).unwrap_or(false);
        m.set_generating(page_id, generating);
        m.update_meta(page_id, title, url);
        if changed {
            emit_pages(app, &m);
        }
    }
    if !generating {
        ensure_maintenance(app);
    }
}

/// Schedule one background check at the next unload deadline. Only one such
/// thread exists at a time; it exits when no hidden page remains loaded.
pub fn ensure_maintenance(app: &AppHandle) {
    if MAINTENANCE_SCHEDULED.swap(true, Ordering::SeqCst) {
        // Already scheduled: wake it so it recomputes its deadline (e.g. after
        // the unload delay setting changed).
        let (lock, cv) = &MAINTENANCE_WAKE;
        if let Ok(mut w) = lock.lock() {
            *w = true;
            cv.notify_one();
        }
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        loop {
            let max_age = unload_after(&app);
            let wait = app.state::<Mgr>().lock().ok().and_then(|m| m.next_due_in(Instant::now(), max_age, crate::activity::page_is_busy));
            let Some(wait) = wait else { break };
            // Small floor so a burst of protected pages can't spin.
            if maintenance_sleep(wait.max(Duration::from_secs(5))) {
                continue; // woken early: recompute the deadline
            }

            let due = app
                .state::<Mgr>()
                .lock()
                .map(|m| m.due_for_unload(Instant::now(), unload_after(&app), crate::activity::page_is_busy))
                .unwrap_or_default();
            for pid in due {
                // Fresh check right before unloading: keep drafts and generations.
                let label = app.state::<Mgr>().lock().ok().and_then(|m| m.get_page(&pid).and_then(|p| p.label.clone()));
                let Some(label) = label else { continue };
                let st = crate::delivery::page_status(&app, &label);
                let protect = st.as_ref().map(|s| s.draft || s.generating).unwrap_or(false);
                if protect {
                    set_draft(&pid, true);
                    // Defer: restart this page's idle clock.
                    if let Ok(mut m) = app.state::<Mgr>().lock() {
                        if let Some(p) = m.pages.iter_mut().find(|p| p.id == pid) {
                            p.hidden_since = Some(Instant::now());
                        }
                    }
                    continue;
                }
                if let Some(st) = st {
                    if let Ok(mut m) = app.state::<Mgr>().lock() {
                        m.update_meta(&pid, Some(&st.title), Some(&st.url));
                    }
                }
                set_draft(&pid, false);
                let app2 = app.clone();
                let pid2 = pid.clone();
                let _ = app.run_on_main_thread(move || unload_if_still_idle(&app2, &pid2, &label));
            }
        }
        MAINTENANCE_SCHEDULED.store(false, Ordering::SeqCst);
        // A page may have become hidden between the last check and the store
        // above (its ensure_maintenance saw the flag set and returned).
        let pending = app
            .state::<Mgr>()
            .lock()
            .ok()
            .and_then(|m| m.next_due_in(Instant::now(), unload_after(&app), crate::activity::page_is_busy))
            .is_some();
        if pending {
            ensure_maintenance(&app);
        }
    });
}

/// Sleep up to `d`; returns true if woken early by `ensure_maintenance`.
fn maintenance_sleep(d: Duration) -> bool {
    let (lock, cv) = &MAINTENANCE_WAKE;
    let Ok(guard) = lock.lock() else {
        std::thread::sleep(d);
        return false;
    };
    let (mut guard, _) = cv.wait_timeout_while(guard, d, |w| !*w).unwrap_or_else(|e| e.into_inner());
    let woke = *guard;
    *guard = false;
    woke
}

/// Delivery target for the active page.
pub fn active_target(app: &AppHandle) -> Option<crate::delivery::Target> {
    let m = app.state::<Mgr>();
    let m = m.lock().ok()?;
    let p = m.get_active_page()?;
    let label = p.label.clone()?;
    Some(crate::delivery::Target {
        page_id: p.id.clone(),
        slot_gen: crate::panel::slot_generation(&label)?,
        label,
    })
}

pub fn link_query(app: &AppHandle, page_id: &str, record_id: &str) {
    if let Ok(mut m) = app.state::<Mgr>().lock() {
        m.set_query(page_id, record_id);
    }
}
