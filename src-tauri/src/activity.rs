//! Controlled App Nap opt-out.
//!
//! Previously the app called `beginActivityWithOptions:` once at launch with
//! `0x00FFFFFF` and never ended it. That value equals `NSActivityUserInitiated`,
//! which *includes* `NSActivityIdleSystemSleepDisabled` — so the app blocked
//! idle system sleep and App Nap for its whole lifetime.
//!
//! Now an activity is held only while at least one piece of user-initiated
//! work (an AI generation started from Peekabrowser) is in flight. Each work
//! item has an id and a hard deadline; the system activity ends as soon as the
//! last item finishes, fails, is cancelled, its page is unloaded, or it times
//! out. Idle system sleep stays allowed throughout.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// NSActivityUserInitiatedAllowingIdleSystemSleep
/// = NSActivityUserInitiated (0x00FFFFFF) & ~NSActivityIdleSystemSleepDisabled (1 << 20)
pub const ACTIVITY_OPTIONS: u64 = 0x00FF_FFFF & !(1u64 << 20);

/// Upper bound for a single generation. Long reasoning is allowed; this only
/// guarantees the token can never be held forever if completion is missed.
pub const MAX_WORK_DURATION: Duration = Duration::from_secs(15 * 60);

#[derive(Default)]
struct WorkSet {
    /// work id -> (page id, deadline)
    items: HashMap<String, (String, Instant)>,
    /// Retained NSActivity token (an `id` stored as usize) while items is non-empty.
    token: Option<usize>,
}

static WORK: Mutex<Option<WorkSet>> = Mutex::new(None);

fn with_work<R>(f: impl FnOnce(&mut WorkSet) -> R) -> R {
    let mut guard = WORK.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(WorkSet::default))
}

/// Register a unit of work. Starts the system activity if it is the first.
pub fn begin(work_id: &str, page_id: &str) {
    let deadline = Instant::now() + MAX_WORK_DURATION;
    with_work(|w| {
        w.items.insert(work_id.to_string(), (page_id.to_string(), deadline));
        if w.token.is_none() {
            w.token = native_begin();
            log::info!("activity: begin (work={})", work_id);
        }
    });
    // One-shot deadline guard (no polling): ends the work if completion is never observed.
    let id = work_id.to_string();
    std::thread::spawn(move || {
        std::thread::sleep(MAX_WORK_DURATION + Duration::from_secs(1));
        let expired = with_work(|w| matches!(w.items.get(&id), Some((_, d)) if Instant::now() >= *d));
        if expired {
            log::warn!("activity: work {} timed out", id);
            end(&id);
        }
    });
}

/// Finish a unit of work (completed, failed or cancelled). Idempotent.
pub fn end(work_id: &str) {
    with_work(|w| {
        if w.items.remove(work_id).is_some() && w.items.is_empty() {
            if let Some(t) = w.token.take() {
                native_end(t);
                log::info!("activity: end (last work={})", work_id);
            }
        }
    });
}

/// End all work tied to a page (page closed or unloaded).
pub fn end_for_page(page_id: &str) {
    let ids: Vec<String> = with_work(|w| {
        w.items
            .iter()
            .filter(|(_, (p, _))| p == page_id)
            .map(|(k, _)| k.clone())
            .collect()
    });
    for id in ids {
        end(&id);
    }
}

/// Whether any work is in flight for the page (used to protect it from unloading).
pub fn page_is_busy(page_id: &str) -> bool {
    with_work(|w| w.items.values().any(|(p, _)| p == page_id))
}

/// Number of in-flight work items (0 means the app holds no activity token).
pub fn active_count() -> usize {
    with_work(|w| w.items.len())
}

#[cfg(target_os = "macos")]
fn native_begin() -> Option<usize> {
    unsafe {
        use objc2::msg_send;
        use objc2::runtime::{AnyClass, AnyObject};
        use objc2_foundation::NSString;
        let cls = AnyClass::get(c"NSProcessInfo")?;
        let info: *mut AnyObject = msg_send![cls, processInfo];
        let reason = NSString::from_str("Peekabrowser: finishing a user-requested AI response");
        let token: *mut AnyObject = msg_send![info, beginActivityWithOptions: ACTIVITY_OPTIONS, reason: &*reason];
        if token.is_null() {
            return None;
        }
        let token: *mut AnyObject = msg_send![token, retain];
        Some(token as usize)
    }
}

#[cfg(target_os = "macos")]
fn native_end(token: usize) {
    unsafe {
        use objc2::msg_send;
        use objc2::runtime::{AnyClass, AnyObject};
        let Some(cls) = AnyClass::get(c"NSProcessInfo") else { return };
        let info: *mut AnyObject = msg_send![cls, processInfo];
        let token = token as *mut AnyObject;
        let _: () = msg_send![info, endActivity: token];
        let _: () = msg_send![token, release];
    }
}

#[cfg(not(target_os = "macos"))]
fn native_begin() -> Option<usize> {
    Some(1)
}

#[cfg(not(target_os = "macos"))]
fn native_end(_token: usize) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_allow_idle_sleep() {
        assert_eq!(ACTIVITY_OPTIONS & (1 << 20), 0);
        assert_eq!(ACTIVITY_OPTIONS, 0x00EF_FFFF);
    }

    #[test]
    fn refcount_returns_to_zero() {
        begin("w1", "p1");
        begin("w2", "p1");
        begin("w3", "p2");
        assert!(page_is_busy("p1"));
        end("w1");
        end("w1"); // idempotent
        assert!(page_is_busy("p1"));
        end_for_page("p1");
        assert!(!page_is_busy("p1"));
        assert!(with_work(|w| w.token.is_some()));
        end("w3");
        assert_eq!(active_count(), 0);
        assert!(with_work(|w| w.token.is_none()));
    }
}
