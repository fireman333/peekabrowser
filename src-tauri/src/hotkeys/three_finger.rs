//! Three-finger trackpad tap → look up the text under the cursor.
//!
//! macOS does not let apps intercept the system Look Up gesture, so (like
//! MiddleClick / BetterTouchTool) raw touch frames are read from Apple's
//! private `MultitouchSupport.framework`. The user is expected to switch off
//! System Settings → Trackpad → "Look up & data detectors"; if it stays on,
//! the system Dictionary popover simply shows up as well (harmless).
//!
//! The framework is opened with `dlopen` at runtime, so there is no link-time
//! dependency: if it (or one of its symbols) is missing on some macOS version,
//! the feature logs once and stays off.
//!
//! Hardened runtime: library validation only admits code signed by Apple or
//! by our own team. MultitouchSupport is an Apple platform binary living in
//! the dyld shared cache, so no `disable-library-validation` entitlement is
//! needed (same as loading any other system framework).
//!
//! Threads: MultitouchSupport calls [`contact_frame`] on its own private
//! thread. That callback only feeds the pure [`TapDetector`]; a detected tap
//! is handed to a worker thread (channel) that captures text through
//! Accessibility and routes it like ⌘C ⌘C. UI work goes through
//! `run_on_main_thread` inside `route_text_from_other_apps`.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

// ---------------------------------------------------------------------------
// Pure tap recognition (no FFI; unit-tested)
// ---------------------------------------------------------------------------

/// All three fingers must touch down within this many seconds of the first.
pub const MAX_DOWN_SPREAD: f64 = 0.08;
/// Every finger must have lifted within this many seconds of the first touch.
pub const MAX_TAP_DURATION: f64 = 0.30;
/// Maximum centroid travel (normalized trackpad units, 0–1) while 3 are down.
pub const MAX_MOVEMENT: f32 = 0.05;
/// Minimum time between two fired taps.
pub const DEBOUNCE: f64 = 0.50;

#[derive(Debug, Clone, Copy)]
struct Gesture {
    /// Time the first finger touched down.
    start: f64,
    /// Time three fingers were first down together.
    three_at: Option<f64>,
    max_count: usize,
    origin: Option<(f32, f32)>,
    max_move: f32,
    /// A frame with exactly three fingers had unreadable positions.
    bad_positions: bool,
}

/// Recognizes a quick three-finger tap from a stream of touch frames.
/// Feed every frame (including the empty one sent when all fingers lift).
#[derive(Debug, Default)]
pub struct TapDetector {
    gesture: Option<Gesture>,
    last_fire: Option<f64>,
}

impl TapDetector {
    pub fn new() -> Self {
        Self::default()
    }

    /// `t`: frame time in seconds (monotonic). `count`: fingers on the pad.
    /// `centroid`: mean normalized position of the touches, `None` if it
    /// could not be read. Returns true exactly once per recognized tap.
    pub fn on_frame(&mut self, t: f64, count: usize, centroid: Option<(f32, f32)>) -> bool {
        if count == 0 {
            let Some(g) = self.gesture.take() else { return false };
            return self.finish(g, t);
        }
        let g = self.gesture.get_or_insert(Gesture {
            start: t,
            three_at: None,
            max_count: 0,
            origin: None,
            max_move: 0.0,
            bad_positions: false,
        });
        g.max_count = g.max_count.max(count);
        if count == 3 {
            if g.three_at.is_none() {
                g.three_at = Some(t);
            }
            match (centroid, g.origin) {
                (None, _) => g.bad_positions = true,
                (Some(c), None) => g.origin = Some(c),
                (Some((x, y)), Some((ox, oy))) => {
                    let d = ((x - ox).powi(2) + (y - oy).powi(2)).sqrt();
                    g.max_move = g.max_move.max(d);
                }
            }
        }
        false
    }

    fn finish(&mut self, g: Gesture, t: f64) -> bool {
        let Some(three_at) = g.three_at else { return false };
        let ok = g.max_count == 3
            && three_at - g.start <= MAX_DOWN_SPREAD
            && t - g.start <= MAX_TAP_DURATION
            && !g.bad_positions
            && g.max_move < MAX_MOVEMENT
            && self.last_fire.is_none_or(|last| g.start - last >= DEBOUNCE);
        if ok {
            self.last_fire = Some(t);
        } else {
            log::info!(
                "three-finger: rejected gesture (max_fingers={}, spread={:.0}ms, duration={:.0}ms, move={:.3}, bad_pos={})",
                g.max_count,
                (three_at - g.start) * 1000.0,
                (t - g.start) * 1000.0,
                g.max_move,
                g.bad_positions
            );
        }
        ok
    }
}

// ---------------------------------------------------------------------------
// MultitouchSupport bridge
// ---------------------------------------------------------------------------

/// `MTTouch` layout read here (64-bit, unchanged from 10.x through macOS 15,
/// still matching on 26/27 as far as MiddleClick-style tools report):
///
/// ```text
/// off  type    field
///   0  i32     frame
///   8  f64     timestamp
///  16  i32     identifier
///  20  i32     state        (1 notTracking … 4 makeTouch, 5 touching, 6 breakTouch …)
///  24  i32     fingerId
///  28  i32     handId
///  32  f32×2   normalized position (x, y), 0–1, origin bottom-left
///  40  f32×2   normalized velocity
///  48  …       size, angle, axes, mm position/velocity, padding
///  96          sizeof(MTTouch)
/// ```
///
/// Only the finger count (callback argument) and the normalized position are
/// used. Positions are sanity-checked (finite, roughly 0–1); if they are not,
/// the layout has changed and no tap fires (conservative: better silent than
/// firing on three-finger swipes). Touch `state` is not used.
const MT_TOUCH_STRIDE: usize = 96;
const MT_TOUCH_FRAME: usize = 0;
const MT_TOUCH_TIMESTAMP: usize = 8;
const MT_TOUCH_STATE: usize = 20;
const MT_TOUCH_POS_X: usize = 32;
const MT_TOUCH_POS_Y: usize = 36;

/// Runtime check of the assumed layout: the first frames with two or more
/// touches are cross-checked (every record's `frame` equals the callback's,
/// its `timestamp` is close to the callback's, `state` is 1–7). Enough
/// consistent frames mark the layout verified (no more checks); repeated
/// mismatches turn the feature off for this run, with one log line, rather
/// than reading the wrong fields (or past the buffer, if the stride shrank).
const LAYOUT_UNVERIFIED: u8 = 0;
const LAYOUT_OK: u8 = 1;
const LAYOUT_BAD: u8 = 2;
const LAYOUT_FRAMES_TO_TRUST: u32 = 5;
const LAYOUT_FRAMES_TO_REJECT: u32 = 3;
static LAYOUT_STATE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(LAYOUT_UNVERIFIED);
static LAYOUT_GOOD_FRAMES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static LAYOUT_BAD_FRAMES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Whether one MTTouch record's header agrees with the callback arguments.
pub fn touch_record_consistent(rec_frame: i32, rec_timestamp: f64, rec_state: i32, frame: i32, timestamp: f64) -> bool {
    rec_frame == frame
        && rec_timestamp.is_finite()
        && (rec_timestamp - timestamp).abs() < 1.0
        && (1..=7).contains(&rec_state)
}

/// Feed one frame to the layout check; returns false once the layout is
/// known to be wrong (callers then ignore the frame).
fn layout_usable(touches: *const u8, n: usize, frame: i32, timestamp: f64) -> bool {
    use std::sync::atomic::Ordering::SeqCst;
    match LAYOUT_STATE.load(SeqCst) {
        LAYOUT_OK => return true,
        LAYOUT_BAD => return false,
        _ => {}
    }
    if n < 2 || touches.is_null() {
        return true; // a single record cannot reveal a stride change
    }
    let consistent = (0..n).all(|i| unsafe {
        // SAFETY: as in contact_frame; only header fields of each record.
        let base = touches.add(i * MT_TOUCH_STRIDE);
        touch_record_consistent(
            std::ptr::read_unaligned(base.add(MT_TOUCH_FRAME) as *const i32),
            std::ptr::read_unaligned(base.add(MT_TOUCH_TIMESTAMP) as *const f64),
            std::ptr::read_unaligned(base.add(MT_TOUCH_STATE) as *const i32),
            frame,
            timestamp,
        )
    });
    if consistent {
        if LAYOUT_GOOD_FRAMES.fetch_add(1, SeqCst) + 1 >= LAYOUT_FRAMES_TO_TRUST {
            LAYOUT_STATE.store(LAYOUT_OK, SeqCst);
            log::info!("three-finger: MTTouch layout verified");
        }
        true
    } else if LAYOUT_BAD_FRAMES.fetch_add(1, SeqCst) + 1 >= LAYOUT_FRAMES_TO_REJECT {
        if LAYOUT_STATE.swap(LAYOUT_BAD, SeqCst) != LAYOUT_BAD {
            log::error!(
                "three-finger: MTTouch layout does not match this macOS version (stride {}); lookup taps disabled",
                MT_TOUCH_STRIDE
            );
        }
        false
    } else {
        false
    }
}

type MTDeviceRef = *mut c_void;
type ContactCallback = extern "C" fn(MTDeviceRef, *const u8, i32, f64, i32) -> i32;

struct Mt {
    create_list: unsafe extern "C" fn() -> *const c_void,
    register: unsafe extern "C" fn(MTDeviceRef, ContactCallback),
    unregister: unsafe extern "C" fn(MTDeviceRef, ContactCallback),
    start: unsafe extern "C" fn(MTDeviceRef, i32) -> i32,
    stop: unsafe extern "C" fn(MTDeviceRef) -> i32,
}

const RTLD_NOW: i32 = 0x2;
extern "C" {
    fn dlopen(path: *const std::os::raw::c_char, mode: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const std::os::raw::c_char) -> *mut c_void;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFArrayGetCount(arr: *const c_void) -> isize;
    fn CFArrayGetValueAtIndex(arr: *const c_void, idx: isize) -> *const c_void;
    fn CFRelease(cf: *const c_void);
}

/// Loaded once; `None` if the private framework is unavailable.
fn mt() -> Option<&'static Mt> {
    static MT: OnceLock<Option<Mt>> = OnceLock::new();
    MT.get_or_init(|| unsafe {
        let h = dlopen(
            c"/System/Library/PrivateFrameworks/MultitouchSupport.framework/MultitouchSupport".as_ptr(),
            RTLD_NOW,
        );
        if h.is_null() {
            log::warn!("three-finger: MultitouchSupport.framework not available");
            return None;
        }
        let sym = |name: &std::ffi::CStr| {
            let p = dlsym(h, name.as_ptr());
            if p.is_null() {
                log::warn!("three-finger: missing symbol {:?}", name);
            }
            p
        };
        let (a, b, c, d, e) = (
            sym(c"MTDeviceCreateList"),
            sym(c"MTRegisterContactFrameCallback"),
            sym(c"MTUnregisterContactFrameCallback"),
            sym(c"MTDeviceStart"),
            sym(c"MTDeviceStop"),
        );
        if [a, b, c, d, e].iter().any(|p| p.is_null()) {
            return None;
        }
        Some(Mt {
            create_list: std::mem::transmute::<*mut c_void, unsafe extern "C" fn() -> *const c_void>(a),
            register: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(MTDeviceRef, ContactCallback)>(b),
            unregister: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(MTDeviceRef, ContactCallback)>(c),
            start: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(MTDeviceRef, i32) -> i32>(d),
            stop: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(MTDeviceRef) -> i32>(e),
        })
    })
    .as_ref()
}

/// The device list from `MTDeviceCreateList` while running (owned; released on stop).
struct Running {
    list: usize,
    devices: Vec<usize>,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);
/// One detector per device (pointer) so two trackpads never mix frames.
static DETECTORS: Mutex<Option<HashMap<usize, TapDetector>>> = Mutex::new(None);
static TAPS: OnceLock<Mutex<Sender<()>>> = OnceLock::new();
static WAKE_OBSERVER: AtomicBool = AtomicBool::new(false);
/// The "needs Accessibility" notice was shown (once per launch).
static AX_WARNED: AtomicBool = AtomicBool::new(false);

/// Accessibility gates text capture (like ⌘C ⌘C). Taps are still detected
/// without it, so granting it later works without a relaunch. Logs every
/// time, notifies once per launch.
fn accessibility_ok(app: &AppHandle) -> bool {
    if crate::native::accessibility_trusted() {
        return true;
    }
    log::warn!("three-finger: Accessibility not granted; lookups are ignored until it is");
    if !AX_WARNED.swap(true, Ordering::SeqCst) {
        crate::lifecycle::notify(
            app,
            crate::i18n::tr(
                "三指點按查詢需要「輔助使用」權限",
                "Three-finger lookup needs Accessibility permission",
            ),
        );
    }
    false
}

/// Called by MultitouchSupport on its private thread for every touch frame.
/// Must stay cheap and must never touch UI.
extern "C" fn contact_frame(device: MTDeviceRef, touches: *const u8, count: i32, timestamp: f64, frame: i32) -> i32 {
    let n = count.max(0) as usize;
    if !layout_usable(touches, n, frame, timestamp) {
        return 0;
    }
    let centroid = if n == 0 || touches.is_null() {
        None
    } else {
        let (mut sx, mut sy) = (0.0f32, 0.0f32);
        let mut ok = true;
        for i in 0..n {
            // SAFETY: MultitouchSupport passes `count` contiguous MTTouch records.
            let (x, y) = unsafe {
                let base = touches.add(i * MT_TOUCH_STRIDE);
                (
                    std::ptr::read_unaligned(base.add(MT_TOUCH_POS_X) as *const f32),
                    std::ptr::read_unaligned(base.add(MT_TOUCH_POS_Y) as *const f32),
                )
            };
            if !(x.is_finite() && y.is_finite() && (-0.1..=1.1).contains(&x) && (-0.1..=1.1).contains(&y)) {
                ok = false;
                break;
            }
            sx += x;
            sy += y;
        }
        ok.then(|| (sx / n as f32, sy / n as f32))
    };
    let fired = DETECTORS
        .lock()
        .ok()
        .and_then(|mut d| {
            d.as_mut()
                .map(|m| m.entry(device as usize).or_default().on_frame(timestamp, n, centroid))
        })
        .unwrap_or(false);
    if fired {
        if let Some(tx) = TAPS.get().and_then(|t| t.lock().ok()) {
            let _ = tx.send(());
        }
    }
    0
}

/// Start (or stop) listening to match the `three_finger_lookup` setting.
pub fn sync(app: &AppHandle) {
    let enabled = app
        .try_state::<crate::app_settings::AppSettingsStore>()
        .map(|s| s.get().three_finger_lookup)
        .unwrap_or(false);
    if enabled {
        start(app);
    } else {
        stop();
    }
}

/// Begin listening on every multitouch device present now (built-in trackpad,
/// Magic Trackpad). No-op if already running.
pub fn start(app: &AppHandle) {
    ensure_worker(app);
    ensure_wake_observer(app);
    let Some(mt) = mt() else { return };
    let Ok(mut running) = RUNNING.lock() else { return };
    if running.is_some() {
        return;
    }
    accessibility_ok(app);
    if let Ok(mut d) = DETECTORS.lock() {
        *d = Some(HashMap::new());
    }
    unsafe {
        let list = (mt.create_list)();
        if list.is_null() {
            log::warn!("three-finger: no multitouch device list");
            return;
        }
        let mut devices = Vec::new();
        for i in 0..CFArrayGetCount(list) {
            let dev = CFArrayGetValueAtIndex(list, i) as MTDeviceRef;
            if dev.is_null() {
                continue;
            }
            (mt.register)(dev, contact_frame);
            let rc = (mt.start)(dev, 0);
            log::info!("three-finger: started multitouch device #{} (rc {})", i, rc);
            devices.push(dev as usize);
        }
        if devices.is_empty() {
            log::info!("three-finger: no multitouch devices found");
        }
        *running = Some(Running { list: list as usize, devices });
    }
}

/// Stop listening and release the device list.
pub fn stop() {
    // Check RUNNING first so a disabled setting never even dlopens the framework.
    let Ok(mut running) = RUNNING.lock() else { return };
    let Some(r) = running.take() else { return };
    let Some(mt) = mt() else { return };
    unsafe {
        for &dev in &r.devices {
            (mt.unregister)(dev as MTDeviceRef, contact_frame);
            (mt.stop)(dev as MTDeviceRef);
        }
        CFRelease(r.list as *const c_void);
    }
    if let Ok(mut d) = DETECTORS.lock() {
        *d = None;
    }
    log::info!("three-finger: stopped ({} devices)", r.devices.len());
}

/// Re-scan devices (picks up a Magic Trackpad paired since start, and
/// revives devices that stop delivering frames after sleep).
fn restart(app: &AppHandle) {
    let was_running = RUNNING.lock().map(|r| r.is_some()).unwrap_or(false);
    if was_running {
        stop();
        sync(app);
    }
}

/// Worker thread: turns a detected tap into captured text + routing.
fn ensure_worker(app: &AppHandle) {
    if TAPS.get().is_some() {
        return;
    }
    let (tx, rx) = channel::<()>();
    if TAPS.set(Mutex::new(tx)).is_err() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let mut last = Instant::now() - Duration::from_secs(10);
        while rx.recv().is_ok() {
            // Global debounce (two trackpads, or a burst queued while busy).
            if last.elapsed() < Duration::from_secs_f64(DEBOUNCE) {
                continue;
            }
            last = Instant::now();
            on_tap(&app);
        }
    });
}

fn on_tap(app: &AppHandle) {
    if !accessibility_ok(app) {
        return;
    }
    let (x, y) = crate::panel::get_cursor_topleft_pos();
    let Some(cap) = crate::selection::capture_text_at(x, y, || copy_keycode_on_main(app)) else {
        log::info!("three-finger: tap at ({:.0}, {:.0}) found no text", x, y);
        return;
    };
    log::info!("three-finger: captured {} chars via {}", cap.text.chars().count(), cap.tier.label());
    crate::panel::lookup_highlight::show(app, cap.bounds, (x, y));
    let must_choose = cap.tier.must_choose();
    let source_app = cap.source_app.or_else(crate::native::frontmost_app_name);
    if let Some(state) = app.try_state::<crate::delivery::PickerState>() {
        if let Ok(mut s) = state.0.lock() {
            s.payload = Some(crate::delivery::Payload::Text { text: cap.text });
            s.source_app = source_app;
        }
    }
    // Synthetic ⌘C may have copied something other than what was pointed
    // at (whole line, file name): always let the user choose.
    crate::commands::route_text_from_other_apps_with(app, must_choose);
}

/// Keycode of "c" in the current keyboard layout, read on the main thread
/// (Text Input Sources are main-thread only). Falls back to ANSI "C".
fn copy_keycode_on_main(app: &AppHandle) -> u16 {
    let (tx, rx) = channel();
    let queued = app.run_on_main_thread(move || {
        let _ = tx.send(crate::selection::copy_keycode());
    });
    if queued.is_err() {
        return crate::selection::DEFAULT_KEYCODE_C;
    }
    rx.recv_timeout(Duration::from_millis(300)).unwrap_or(crate::selection::DEFAULT_KEYCODE_C)
}

/// Multitouch devices go quiet after sleep; restart them on wake.
fn ensure_wake_observer(app: &AppHandle) {
    if WAKE_OBSERVER.swap(true, Ordering::SeqCst) {
        return;
    }
    let app2 = app.clone();
    let queued = app.run_on_main_thread(move || {
        #[cfg(target_os = "macos")]
        unsafe {
            use block2::RcBlock;
            use objc2::msg_send;
            use objc2::runtime::{AnyClass, AnyObject};
            use objc2_foundation::NSString;
            let Some(ws_cls) = AnyClass::get(c"NSWorkspace") else { return };
            let ws: *mut AnyObject = msg_send![ws_cls, sharedWorkspace];
            if ws.is_null() {
                return;
            }
            let center: *mut AnyObject = msg_send![ws, notificationCenter];
            let name = NSString::from_str("NSWorkspaceDidWakeNotification");
            let app3 = app2.clone();
            let block = RcBlock::new(move |_n: *mut AnyObject| {
                let app4 = app3.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(2));
                    restart(&app4);
                });
            });
            let nil: *mut AnyObject = std::ptr::null_mut();
            let obs: *mut AnyObject = msg_send![center, addObserverForName: &*name, object: nil, queue: nil, usingBlock: &*block];
            if !obs.is_null() {
                let _: *mut AnyObject = msg_send![obs, retain]; // lives for the app's lifetime
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = app2;
    });
    if queued.is_err() {
        WAKE_OBSERVER.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_record_header_check() {
        assert!(touch_record_consistent(42, 1000.25, 4, 42, 1000.3));
        assert!(!touch_record_consistent(41, 1000.25, 4, 42, 1000.3), "frame mismatch");
        assert!(!touch_record_consistent(42, 1003.0, 4, 42, 1000.3), "timestamp far off");
        assert!(!touch_record_consistent(42, f64::NAN, 4, 42, 1000.3));
        assert!(!touch_record_consistent(42, 1000.25, 0, 42, 1000.3), "state out of range");
        assert!(!touch_record_consistent(42, 1000.25, 9, 42, 1000.3));
    }

    const C: Option<(f32, f32)> = Some((0.5, 0.5));

    /// Three fingers down at `t0` (spread `spread`), up at `t0 + dur`.
    fn tap(d: &mut TapDetector, t0: f64, spread: f64, dur: f64) -> bool {
        let mut fired = d.on_frame(t0, 1, C);
        fired |= d.on_frame(t0 + spread / 2.0, 2, C);
        fired |= d.on_frame(t0 + spread, 3, C);
        fired |= d.on_frame(t0 + dur - 0.01, 3, C);
        fired |= d.on_frame(t0 + dur, 0, None);
        fired
    }

    #[test]
    fn valid_tap_fires_once() {
        let mut d = TapDetector::new();
        assert!(tap(&mut d, 10.0, 0.03, 0.15));
    }

    #[test]
    fn simultaneous_touchdown_fires() {
        let mut d = TapDetector::new();
        assert!(!d.on_frame(1.0, 3, C));
        assert!(!d.on_frame(1.05, 3, C));
        assert!(d.on_frame(1.1, 0, None));
    }

    #[test]
    fn too_slow_does_not_fire() {
        let mut d = TapDetector::new();
        assert!(!tap(&mut d, 10.0, 0.03, 0.45));
    }

    #[test]
    fn staggered_touchdown_does_not_fire() {
        let mut d = TapDetector::new();
        assert!(!tap(&mut d, 10.0, 0.15, 0.25));
    }

    #[test]
    fn moved_swipe_does_not_fire() {
        let mut d = TapDetector::new();
        d.on_frame(1.0, 3, Some((0.40, 0.5)));
        d.on_frame(1.05, 3, Some((0.48, 0.5)));
        d.on_frame(1.10, 3, Some((0.60, 0.5)));
        assert!(!d.on_frame(1.15, 0, None));
    }

    #[test]
    fn small_jitter_still_fires() {
        let mut d = TapDetector::new();
        d.on_frame(1.0, 3, Some((0.50, 0.50)));
        d.on_frame(1.05, 3, Some((0.51, 0.49)));
        assert!(d.on_frame(1.1, 0, None));
    }

    #[test]
    fn four_fingers_do_not_fire() {
        let mut d = TapDetector::new();
        d.on_frame(1.0, 3, C);
        d.on_frame(1.02, 4, C);
        d.on_frame(1.05, 3, C);
        assert!(!d.on_frame(1.1, 0, None));
    }

    #[test]
    fn two_fingers_do_not_fire() {
        let mut d = TapDetector::new();
        d.on_frame(1.0, 2, C);
        d.on_frame(1.05, 2, C);
        assert!(!d.on_frame(1.1, 0, None));
    }

    #[test]
    fn unreadable_positions_do_not_fire() {
        let mut d = TapDetector::new();
        d.on_frame(1.0, 3, None);
        assert!(!d.on_frame(1.1, 0, None));
    }

    #[test]
    fn debounce_blocks_quick_second_tap() {
        let mut d = TapDetector::new();
        assert!(tap(&mut d, 10.0, 0.02, 0.12));
        assert!(!tap(&mut d, 10.3, 0.02, 0.12));
        assert!(tap(&mut d, 11.0, 0.02, 0.12));
    }

    #[test]
    fn lift_without_gesture_is_ignored() {
        let mut d = TapDetector::new();
        assert!(!d.on_frame(1.0, 0, None));
    }
}
