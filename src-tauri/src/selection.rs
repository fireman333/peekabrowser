//! Read the text the user is pointing at in another app (three-finger lookup).
//!
//! Tiers, tried in order:
//! 1. `AXSelectedText` of the focused element in the frontmost app, but only
//!    when the tap lands on that selection (`AXBoundsForRange` of
//!    `AXSelectedTextRange`, with a small tolerance) and the element under the
//!    cursor belongs to the frontmost app. Like the system Look Up gesture.
//! W. WebKit web content (Safari, Mail, WKWebView apps): when the element
//!    under the cursor sits inside an `AXWebArea`, WebKit's opaque text
//!    markers are used instead of character ranges (web content implements
//!    neither `AXRangeForPosition` nor `AXSelectedTextRange`):
//!    a. `AXSelectedTextMarkerRange` of the web area, only when the tap lands
//!       on it (`AXBoundsForTextMarkerRange`, same tolerance as tier 1);
//!    b. `AXTextMarkerForPosition` → the character under the point →
//!       `AXRight/LeftWordTextMarkerRangeForTextMarker` → the candidate whose
//!       bounds contain the point; long CJK runs are narrowed with
//!       `CFStringTokenizer`. Chromium also answers these when its AX tree is
//!       on (we never set `AXEnhancedUserInterface`); otherwise tier 3 covers it.
//! 2. The word under the cursor: `AXUIElementCopyElementAtPosition` →
//!    `AXRangeForPosition` → `AXStringForRange` on a small window around that
//!    index → word boundaries from `CFStringTokenizer` (handles CJK too).
//!    If that finds nothing, an existing selection in the same (frontmost)
//!    app is used after all (tap on whitespace next to a selection).
//! 3. Last resort: a synthetic ⌘C (CGEvent) with the pasteboard saved before
//!    and restored afterwards. ⌘C ⌘C detection is told to ignore it. Gated:
//!    the cursor must be over the frontmost app, and AX must report a
//!    non-empty `AXSelectedTextRange`. When the selection range cannot be read
//!    at all, the copy is still tried for apps known to expose little or no
//!    AX text (Electron / Chromium-based bundles), because there it is the
//!    only way to get text. Many apps copy something even with no selection
//!    (VS Code copies the whole line, Finder the file name), so text from this
//!    tier is never auto-sent: [`Tier::must_choose`] makes the caller open
//!    the picker.
//!
//! Never synthesizes clicks. Needs Accessibility. All calls here are safe off
//! the main thread (AX and NSPasteboard are thread-safe; nothing touches UI),
//! except [`copy_keycode`], which reads the keyboard layout (main thread).

use std::ffi::c_void;
use std::time::{Duration, Instant};

/// Which tier produced the captured text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// An existing selection (tap on it, or fallback when no word was found).
    Selection,
    /// The word under the cursor.
    WordAtCursor,
    /// Selection in WebKit web content (text-marker range under the tap).
    WebSelection,
    /// The word under the cursor in WebKit web content (text markers).
    WebWord,
    /// Synthetic ⌘C: what the app chose to copy, possibly not what was pointed at.
    SyntheticCopy,
}

impl Tier {
    pub fn label(self) -> &'static str {
        match self {
            Tier::Selection => "AXSelectedText",
            Tier::WordAtCursor => "AXRangeForPosition",
            Tier::WebSelection => "AXSelectedTextMarkerRange",
            Tier::WebWord => "AXTextMarkerForPosition",
            Tier::SyntheticCopy => "synthetic ⌘C",
        }
    }

    /// Text from this tier must go through the picker, never auto-send.
    pub fn must_choose(self) -> bool {
        matches!(self, Tier::SyntheticCopy)
    }
}

/// Text found at the cursor.
pub struct Capture {
    pub text: String,
    /// Which tier produced it (logs and routing: see [`Tier::must_choose`]).
    pub tier: Tier,
    /// Name of the app that owns the text, if known.
    pub source_app: Option<String>,
    /// On-screen bounds of the captured text (x, y, w, h in global top-left
    /// points, the AX coordinate space), when the tier can tell cheaply.
    /// `None` for synthetic ⌘C. For a CJK run narrowed by the tokenizer this
    /// is the bounds of the whole run WebKit reported.
    pub bounds: Option<(f64, f64, f64, f64)>,
}

/// Longest text taken from a selection (the picker/destinations cap anyway).
const MAX_CHARS: usize = 20_000;
/// Per-element AX timeout so a hung app cannot stall the worker thread.
const AX_TIMEOUT_SECS: f32 = 0.3;
/// How far (points) outside the selection bounds a tap still counts as on it.
const SELECTION_TOLERANCE: f64 = 4.0;
/// Fallback virtual keycode of "c" (ANSI QWERTY).
pub const DEFAULT_KEYCODE_C: u16 = 8;

/// Axis-aligned rectangle in global top-left-origin points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn tuple(self) -> (f64, f64, f64, f64) {
        (self.x, self.y, self.w, self.h)
    }
}

/// Whether (`px`, `py`) lies inside `r` grown by `tol` on every side.
/// Empty rects (a caret, or bounds the app could not compute) never match.
pub fn point_near_rect(r: Rect, px: f64, py: f64, tol: f64) -> bool {
    if !(r.w > 0.0 && r.h > 0.0) || !r.x.is_finite() || !r.y.is_finite() {
        return false;
    }
    px >= r.x - tol && px <= r.x + r.w + tol && py >= r.y - tol && py <= r.y + r.h + tol
}

/// Whether a bundle's `Contents/Frameworks` entries mark it as an Electron or
/// Chromium-based app (little or no AX text unless a screen reader is on).
pub fn is_ax_poor_bundle<S: AsRef<str>>(framework_names: &[S]) -> bool {
    framework_names.iter().any(|n| {
        let n = n.as_ref();
        n == "Electron Framework.framework"
            || n == "Chromium Embedded Framework.framework"
            || (n.ends_with(" Framework.framework") && (n.contains("Chrome") || n.contains("Chromium")))
    })
}

/// Whether `s` contains Han, kana or Hangul (scripts written without spaces).
pub fn has_cjk(s: &str) -> bool {
    s.chars().any(|c| {
        matches!(c as u32,
            0x3040..=0x30FF      // Hiragana, Katakana
            | 0x3400..=0x4DBF    // CJK Extension A
            | 0x4E00..=0x9FFF    // CJK Unified Ideographs
            | 0xAC00..=0xD7AF    // Hangul syllables
            | 0xF900..=0xFAFF    // CJK Compatibility Ideographs
            | 0x20000..=0x2FA1F) // CJK Extensions B+
    })
}

/// Of the word candidates WebKit offers around a point (right word first),
/// the one the point is on: a candidate whose bounds contain the point wins;
/// if no candidate has readable bounds at all, the first real word.
/// Blank / punctuation-only candidates never count.
pub fn pick_web_word(cands: &[(String, Option<Rect>)], x: f64, y: f64) -> Option<String> {
    const TOL: f64 = 1.0;
    let real = |s: &String| s.chars().any(char::is_alphanumeric);
    if let Some((w, _)) = cands.iter().find(|(w, b)| real(w) && b.is_some_and(|b| point_near_rect(b, x, y, TOL))) {
        return clean(w);
    }
    if cands.iter().all(|(_, b)| b.is_none()) {
        return cands.iter().find(|(w, _)| real(w)).and_then(|(w, _)| clean(w));
    }
    None
}

/// WebKit can hand back a whole CJK run as one "word". Narrow it to the
/// dictionary word around UTF-16 offset `offset` (the character under the
/// cursor). Non-CJK words, or an unknown offset, are returned unchanged.
#[cfg(target_os = "macos")]
pub fn narrow_web_word(word: &str, offset: Option<usize>) -> String {
    const SHORT: usize = 4;
    let Some(off) = offset else { return word.to_string() };
    if !has_cjk(word) || word.chars().count() <= SHORT {
        return word.to_string();
    }
    let len = word.encode_utf16().count();
    word_at_utf16(word, off.min(len.saturating_sub(1))).unwrap_or_else(|| word.to_string())
}

/// `x`, `y`: cursor in global top-left-origin points (same space as AX).
/// `copy_keycode` is only called when tier 3 runs (see [`copy_keycode`]).
/// Logs one info line per call with every tier tried and why it failed.
#[cfg(target_os = "macos")]
pub fn capture_text_at(x: f64, y: f64, copy_keycode: impl FnOnce() -> u16) -> Option<Capture> {
    let mut trace: Vec<String> = Vec::new();
    let out = capture_traced(x, y, copy_keycode, &mut trace);
    log::info!(
        "lookup at ({:.0}, {:.0}): {} [{}]",
        x,
        y,
        out.as_ref().map(|c| format!("got {} chars via {}", c.text.chars().count(), c.tier.label())).unwrap_or_else(|| "nothing".into()),
        trace.join("; ")
    );
    out
}

#[cfg(target_os = "macos")]
fn capture_traced(x: f64, y: f64, copy_keycode: impl FnOnce() -> u16, trace: &mut Vec<String>) -> Option<Capture> {
    let own_pid = std::process::id() as i32;
    let hit = ax::element_at(x, y);
    let hit_pid = hit.as_ref().and_then(ax::pid_of);
    let role = hit.as_ref().and_then(ax::role_of);
    trace.push(format!(
        "hit role={} pid={} bundle={}",
        role.as_deref().unwrap_or(if hit.is_some() { "?" } else { "none (no element at point)" }),
        hit_pid.map(|p| p.to_string()).unwrap_or_else(|| "?".into()),
        hit_pid.and_then(bundle_id).unwrap_or_else(|| "?".into())
    ));
    if hit_pid == Some(own_pid) {
        trace.push("cursor is over Peekabrowser itself; ignored".into());
        return None;
    }
    let front = frontmost_pid().filter(|p| *p != own_pid);
    // The cursor is over the app that owns the focus (and so any selection).
    let on_front = front.filter(|f| hit_pid == Some(*f));
    if on_front.is_none() {
        trace.push(format!("not over frontmost app (front pid={:?}): selection tiers skipped", front));
    }
    let sel = on_front.and_then(ax::selection);

    // 1. Existing selection, only when the tap is on it.
    match (on_front, sel.as_ref()) {
        (Some(pid), Some(s)) => match (s.text.as_ref(), s.bounds) {
            (Some(text), Some(b)) if point_near_rect(b, x, y, SELECTION_TOLERANCE) => {
                let bounds = Some(b.tuple());
                return Some(Capture { text: text.clone(), tier: Tier::Selection, source_app: app_name(pid), bounds });
            }
            (Some(_), Some(_)) => trace.push("T1 AXSelectedText: tap is off the selection".into()),
            (Some(_), None) => trace.push("T1 AXSelectedText: selection has no AXBoundsForRange".into()),
            (None, _) => trace.push(format!("T1 AXSelectedText: none (AXSelectedTextRange len={:?})", s.length)),
        },
        (Some(_), None) => trace.push("T1 AXSelectedText: no focused element".into()),
        _ => {}
    }
    // W. WebKit web content (text markers).
    match hit.as_ref().and_then(ax::web_area_of) {
        None => trace.push("TW web: no AXWebArea ancestor".into()),
        Some(web) => {
            trace.push("TW web: AXWebArea found".into());
            if on_front.is_some() {
                match ax::web_selection(&web) {
                    Some((text, Some(b))) if point_near_rect(b, x, y, SELECTION_TOLERANCE) => {
                        let (tier, source_app, bounds) = (Tier::WebSelection, hit_pid.and_then(app_name), Some(b.tuple()));
                        return Some(Capture { text, tier, source_app, bounds });
                    }
                    Some((_, Some(_))) => trace.push("TWs selection: tap is off it".into()),
                    Some((_, None)) => trace.push("TWs selection: no AXBoundsForTextMarkerRange".into()),
                    None => trace.push("TWs selection: none".into()),
                }
            }
            match ax::web_word_at(&web, x, y) {
                Ok((text, b)) => {
                    let (tier, source_app, bounds) = (Tier::WebWord, hit_pid.and_then(app_name), b.map(Rect::tuple));
                    return Some(Capture { text, tier, source_app, bounds });
                }
                Err(why) => trace.push(format!("TWw word: {why}")),
            }
        }
    }
    // 2. Word under the cursor.
    if let Some(el) = hit.as_ref() {
        if let Some((text, b)) = ax::word_at_position(el, x, y) {
            let source_app = hit_pid.and_then(app_name);
            return Some(Capture { text, tier: Tier::WordAtCursor, source_app, bounds: b.map(Rect::tuple) });
        }
        trace.push("T2 AXRangeForPosition: no word (unsupported or on whitespace)".into());
    }
    // 1b. No word there: fall back to the selection in the same app.
    if let (Some(pid), Some(s)) = (on_front, sel.as_ref()) {
        if let Some(text) = s.text.clone() {
            return Some(Capture { text, tier: Tier::Selection, source_app: app_name(pid), bounds: s.bounds.map(Rect::tuple) });
        }
    }
    // 3. Synthetic ⌘C, gated (see the module docs).
    let pid = on_front?;
    let allowed = match sel.as_ref().and_then(|s| s.length) {
        Some(len) => len > 0,
        None => is_ax_poor_app(pid),
    };
    if !allowed {
        trace.push("T3 ⌘C: skipped (no AX-confirmed selection, not an Electron/Chromium bundle)".into());
        return None;
    }
    let Some(text) = pasteboard::copy_selection_via_cmd_c(copy_keycode()) else {
        trace.push("T3 ⌘C: pasteboard unchanged or empty".into());
        return None;
    };
    Some(Capture { text, tier: Tier::SyntheticCopy, source_app: app_name(pid), bounds: None })
}

#[cfg(target_os = "macos")]
fn bundle_id(pid: i32) -> Option<String> {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::NSString;
    objc2::rc::autoreleasepool(|_| unsafe {
        let cls = AnyClass::get(c"NSRunningApplication")?;
        let app: *mut AnyObject = msg_send![cls, runningApplicationWithProcessIdentifier: pid];
        if app.is_null() {
            return None;
        }
        let id: *mut AnyObject = msg_send![app, bundleIdentifier];
        (!id.is_null()).then(|| (*(id as *const NSString)).to_string())
    })
}

#[cfg(not(target_os = "macos"))]
pub fn capture_text_at(_x: f64, _y: f64, _copy_keycode: impl FnOnce() -> u16) -> Option<Capture> {
    None
}

/// Electron / Chromium bundle check for app `pid` (reads its Frameworks folder).
#[cfg(target_os = "macos")]
fn is_ax_poor_app(pid: i32) -> bool {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::NSString;
    let bundle: Option<String> = objc2::rc::autoreleasepool(|_| unsafe {
        let cls = AnyClass::get(c"NSRunningApplication")?;
        let app: *mut AnyObject = msg_send![cls, runningApplicationWithProcessIdentifier: pid];
        if app.is_null() {
            return None;
        }
        let url: *mut AnyObject = msg_send![app, bundleURL];
        if url.is_null() {
            return None;
        }
        let path: *mut AnyObject = msg_send![url, path];
        (!path.is_null()).then(|| (*(path as *const NSString)).to_string())
    });
    let Some(bundle) = bundle else { return false };
    let names: Vec<String> = std::fs::read_dir(std::path::Path::new(&bundle).join("Contents/Frameworks"))
        .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    is_ax_poor_bundle(&names)
}

/// First virtual keycode in 0..128 that `translate` maps to "c"
/// ([`DEFAULT_KEYCODE_C`] if none).
pub fn find_keycode_for_c(translate: impl Fn(u16) -> Option<char>) -> u16 {
    (0u16..128).find(|k| translate(*k) == Some('c')).unwrap_or(DEFAULT_KEYCODE_C)
}

/// Virtual keycode that types "c" with ⌘ held in the current keyboard layout
/// (Dvorak, AZERTY, "Dvorak – QWERTY ⌘" …). Falls back to 8 (ANSI "C").
/// Text Input Sources must be read on the main thread.
#[cfg(target_os = "macos")]
pub fn copy_keycode() -> u16 {
    use core_foundation::string::CFStringRef;
    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        static kTISPropertyUnicodeKeyLayoutData: CFStringRef;
        fn TISCopyCurrentKeyboardLayoutInputSource() -> *const c_void;
        fn TISGetInputSourceProperty(src: *const c_void, key: CFStringRef) -> *const c_void;
        fn LMGetKbdType() -> u8;
        fn UCKeyTranslate(
            layout: *const c_void,
            keycode: u16,
            action: u16,
            modifiers: u32,
            kbd_type: u32,
            options: u32,
            dead_key_state: *mut u32,
            max_len: usize,
            actual_len: *mut usize,
            chars: *mut u16,
        ) -> i32;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFDataGetBytePtr(data: *const c_void) -> *const u8;
        fn CFRelease(cf: *const c_void);
    }
    const ACTION_DOWN: u16 = 0; // kUCKeyActionDown
    const CMD_MODIFIER: u32 = (0x100 >> 8) & 0xFF; // (cmdKey >> 8) & 0xFF
    const NO_DEAD_KEYS: u32 = 1; // kUCKeyTranslateNoDeadKeysMask
    unsafe {
        let src = TISCopyCurrentKeyboardLayoutInputSource();
        if src.is_null() {
            return DEFAULT_KEYCODE_C;
        }
        let data = TISGetInputSourceProperty(src, kTISPropertyUnicodeKeyLayoutData);
        let layout = if data.is_null() { std::ptr::null() } else { CFDataGetBytePtr(data) as *const c_void };
        let code = if layout.is_null() {
            DEFAULT_KEYCODE_C
        } else {
            let kbd = LMGetKbdType() as u32;
            let translate = |mods: u32| {
                move |k: u16| {
                    let mut dead = 0u32;
                    let mut len = 0usize;
                    let mut buf = [0u16; 4];
                    let err = UCKeyTranslate(
                        layout, k, ACTION_DOWN, mods, kbd, NO_DEAD_KEYS, &mut dead, buf.len(), &mut len, buf.as_mut_ptr(),
                    );
                    if err != 0 || len != 1 {
                        return None;
                    }
                    char::from_u32(buf[0] as u32).map(|c| c.to_ascii_lowercase())
                }
            };
            // With ⌘ first ("Dvorak – QWERTY ⌘" switches to QWERTY under ⌘).
            let with_cmd = find_keycode_for_c(translate(CMD_MODIFIER));
            if with_cmd != DEFAULT_KEYCODE_C || translate(CMD_MODIFIER)(DEFAULT_KEYCODE_C) == Some('c') {
                with_cmd
            } else {
                find_keycode_for_c(translate(0))
            }
        };
        CFRelease(src);
        code
    }
}

#[cfg(not(target_os = "macos"))]
pub fn copy_keycode() -> u16 {
    DEFAULT_KEYCODE_C
}

/// Trim and cap captured text; `None` if nothing is left.
fn clean(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    Some(t.chars().take(MAX_CHARS).collect())
}

#[cfg(target_os = "macos")]
fn frontmost_pid() -> Option<i32> {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    objc2::rc::autoreleasepool(|_| unsafe {
        let ws: *mut AnyObject = msg_send![AnyClass::get(c"NSWorkspace")?, sharedWorkspace];
        if ws.is_null() {
            return None;
        }
        let app: *mut AnyObject = msg_send![ws, frontmostApplication];
        if app.is_null() {
            return None;
        }
        let pid: i32 = msg_send![app, processIdentifier];
        Some(pid)
    })
}

#[cfg(target_os = "macos")]
fn app_name(pid: i32) -> Option<String> {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::NSString;
    objc2::rc::autoreleasepool(|_| unsafe {
        let cls = AnyClass::get(c"NSRunningApplication")?;
        let app: *mut AnyObject = msg_send![cls, runningApplicationWithProcessIdentifier: pid];
        if app.is_null() {
            return None;
        }
        let name: *mut AnyObject = msg_send![app, localizedName];
        if name.is_null() {
            return None;
        }
        Some((*(name as *const NSString)).to_string())
    })
}

/// The word containing UTF-16 index `idx` of `s`, using the system word
/// tokenizer (so Chinese / Japanese get dictionary words, not whole runs).
/// `None` if `idx` is on whitespace or punctuation.
#[cfg(target_os = "macos")]
pub fn word_at_utf16(s: &str, idx: usize) -> Option<String> {
    word_range_at_utf16(s, idx).map(|(w, _, _)| w)
}

/// Like [`word_at_utf16`], plus the word's UTF-16 start and length in `s`.
#[cfg(target_os = "macos")]
pub fn word_range_at_utf16(s: &str, idx: usize) -> Option<(String, usize, usize)> {
    use core_foundation::base::{CFRange, TCFType};
    use core_foundation::string::{CFString, CFStringRef};
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringTokenizerCreate(
            alloc: *const c_void,
            string: CFStringRef,
            range: CFRange,
            options: usize,
            locale: *const c_void,
        ) -> *const c_void;
        fn CFStringTokenizerGoToTokenAtIndex(tokenizer: *const c_void, index: isize) -> usize;
        fn CFStringTokenizerGetCurrentTokenRange(tokenizer: *const c_void) -> CFRange;
        fn CFRelease(cf: *const c_void);
    }
    const UNIT_WORD: usize = 0; // kCFStringTokenizerUnitWord
    const TOKEN_NONE: usize = 0; // kCFStringTokenizerTokenNone
    let units: Vec<u16> = s.encode_utf16().collect();
    if idx >= units.len() {
        return None;
    }
    let cf = CFString::new(s);
    unsafe {
        let tok = CFStringTokenizerCreate(
            std::ptr::null(),
            cf.as_concrete_TypeRef(),
            CFRange::init(0, units.len() as isize),
            UNIT_WORD,
            std::ptr::null(),
        );
        if tok.is_null() {
            return None;
        }
        let kind = CFStringTokenizerGoToTokenAtIndex(tok, idx as isize);
        let r = CFStringTokenizerGetCurrentTokenRange(tok);
        CFRelease(tok);
        if kind == TOKEN_NONE || r.location < 0 || r.length <= 0 {
            return None;
        }
        let (a, b) = (r.location as usize, (r.location + r.length) as usize);
        let word = String::from_utf16_lossy(units.get(a..b)?);
        // The tokenizer also returns runs of punctuation in some locales.
        if !word.chars().any(char::is_alphanumeric) {
            return None;
        }
        clean(&word).map(|w| (w, a, b - a))
    }
}

#[cfg(target_os = "macos")]
mod ax {
    //! Minimal raw Accessibility FFI (ApplicationServices / HIServices).
    use super::{clean, narrow_web_word, pick_web_word, point_near_rect, word_range_at_utf16, Rect, AX_TIMEOUT_SECS};
    use core_foundation::array::CFArray;
    use core_foundation::base::{CFRange, CFType, CFTypeRef, TCFType};
    use core_foundation::number::CFNumber;
    use core_foundation::string::{CFString, CFStringRef};
    use std::ffi::c_void;

    type AXUIElementRef = *const c_void;
    const AX_OK: i32 = 0;
    const AX_VALUE_CGPOINT: u32 = 1;
    const AX_VALUE_CGRECT: u32 = 3;
    const AX_VALUE_CFRANGE: u32 = 4;
    /// Characters fetched on each side of the hit index.
    const WINDOW: isize = 64;
    /// How many `AXParent` steps to climb looking for an `AXWebArea`.
    const WEB_AREA_DEPTH: usize = 12;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct CGRect {
        origin: CGPoint,
        size: CGPoint,
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(el: AXUIElementRef, attr: CFStringRef, value: *mut CFTypeRef) -> i32;
        fn AXUIElementCopyParameterizedAttributeValue(
            el: AXUIElementRef,
            attr: CFStringRef,
            param: CFTypeRef,
            result: *mut CFTypeRef,
        ) -> i32;
        fn AXUIElementCopyElementAtPosition(app: AXUIElementRef, x: f32, y: f32, el: *mut AXUIElementRef) -> i32;
        fn AXUIElementSetMessagingTimeout(el: AXUIElementRef, secs: f32) -> i32;
        fn AXUIElementGetPid(el: AXUIElementRef, pid: *mut i32) -> i32;
        fn AXValueCreate(kind: u32, value: *const c_void) -> CFTypeRef;
        fn AXValueGetValue(value: CFTypeRef, kind: u32, out: *mut c_void) -> bool;
        fn AXValueGetTypeID() -> usize;
    }

    /// Owned AXUIElement (released on drop).
    pub struct Element(CFType);

    impl Element {
        fn raw(&self) -> AXUIElementRef {
            self.0.as_CFTypeRef()
        }
        /// Takes ownership of a +1 reference and gives the element its own
        /// messaging timeout (never set on the system-wide element: that one
        /// is the default for every AX call in the process).
        unsafe fn own(r: AXUIElementRef) -> Option<Self> {
            (!r.is_null()).then(|| {
                AXUIElementSetMessagingTimeout(r, AX_TIMEOUT_SECS);
                Element(CFType::wrap_under_create_rule(r))
            })
        }
        /// Takes ownership of a +1 reference without touching its timeout.
        unsafe fn own_plain(r: AXUIElementRef) -> Option<Self> {
            (!r.is_null()).then(|| Element(CFType::wrap_under_create_rule(r)))
        }
    }

    fn attr(el: &Element, name: &str) -> Option<CFType> {
        let key = CFString::new(name);
        let mut out: CFTypeRef = std::ptr::null();
        let err = unsafe { AXUIElementCopyAttributeValue(el.raw(), key.as_concrete_TypeRef(), &mut out) };
        (err == AX_OK && !out.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(out) })
    }

    fn param_attr(el: &Element, name: &str, param: &CFType) -> Option<CFType> {
        let key = CFString::new(name);
        let mut out: CFTypeRef = std::ptr::null();
        let err = unsafe {
            AXUIElementCopyParameterizedAttributeValue(el.raw(), key.as_concrete_TypeRef(), param.as_CFTypeRef(), &mut out)
        };
        (err == AX_OK && !out.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(out) })
    }

    fn element_attr(el: &Element, name: &str) -> Option<Element> {
        let v = attr(el, name)?;
        // The value is an AXUIElement; keep it under our own wrapper with its
        // own timeout (elements do not inherit the parent's).
        unsafe { AXUIElementSetMessagingTimeout(v.as_CFTypeRef(), AX_TIMEOUT_SECS) };
        Some(Element(v))
    }

    fn string_of(v: &CFType) -> Option<String> {
        v.downcast::<CFString>().map(|s| s.to_string())
    }

    fn range_value(r: CFRange) -> CFType {
        unsafe { CFType::wrap_under_create_rule(AXValueCreate(AX_VALUE_CFRANGE, &r as *const CFRange as *const c_void)) }
    }

    fn range_of(v: &CFType) -> Option<CFRange> {
        if v.type_of() as usize != unsafe { AXValueGetTypeID() } {
            return None;
        }
        let mut r = CFRange::init(0, 0);
        unsafe { AXValueGetValue(v.as_CFTypeRef(), AX_VALUE_CFRANGE, &mut r as *mut CFRange as *mut c_void) }
            .then_some(r)
    }

    pub fn element_at(x: f64, y: f64) -> Option<Element> {
        unsafe {
            let sys = Element::own_plain(AXUIElementCreateSystemWide())?;
            let mut hit: AXUIElementRef = std::ptr::null();
            let err = AXUIElementCopyElementAtPosition(sys.raw(), x as f32, y as f32, &mut hit);
            if err != AX_OK {
                return None;
            }
            Element::own(hit)
        }
    }

    pub fn pid_of(el: &Element) -> Option<i32> {
        let mut pid = 0;
        (unsafe { AXUIElementGetPid(el.raw(), &mut pid) } == AX_OK).then_some(pid)
    }

    /// The focused element's selection in app `pid`, as far as AX exposes it.
    pub struct Selection {
        /// Selected text (cleaned), if readable and non-blank.
        pub text: Option<String>,
        /// Length of `AXSelectedTextRange`, if readable.
        pub length: Option<isize>,
        /// On-screen bounds of the selected range, if the app reports them.
        pub bounds: Option<Rect>,
    }

    /// `None` if the app has no readable focused element.
    pub fn selection(pid: i32) -> Option<Selection> {
        let app = unsafe { Element::own(AXUIElementCreateApplication(pid))? };
        let focused = element_attr(&app, "AXFocusedUIElement")?;
        let text = attr(&focused, "AXSelectedText").and_then(|v| string_of(&v)).and_then(|s| clean(&s));
        let range = attr(&focused, "AXSelectedTextRange").and_then(|v| range_of(&v));
        let bounds = range
            .filter(|r| r.length > 0)
            .and_then(|r| param_attr(&focused, "AXBoundsForRange", &range_value(r)))
            .and_then(|v| rect_of(&v));
        Some(Selection { text, length: range.map(|r| r.length), bounds })
    }

    fn rect_of(v: &CFType) -> Option<Rect> {
        if v.type_of() as usize != unsafe { AXValueGetTypeID() } {
            return None;
        }
        let mut r = CGRect::default();
        unsafe { AXValueGetValue(v.as_CFTypeRef(), AX_VALUE_CGRECT, &mut r as *mut CGRect as *mut c_void) }.then_some(
            Rect { x: r.origin.x, y: r.origin.y, w: r.size.x, h: r.size.y },
        )
    }

    /// Tier 2: the word under (x, y) in `hit` or one of its first ancestors
    /// (text views often answer for their static-text children).
    pub fn word_at_position(hit: &Element, x: f64, y: f64) -> Option<(String, Option<Rect>)> {
        let point = CGPoint { x, y };
        let point = unsafe {
            CFType::wrap_under_create_rule(AXValueCreate(AX_VALUE_CGPOINT, &point as *const CGPoint as *const c_void))
        };
        let mut ancestor: Option<Element> = None;
        for _ in 0..3 {
            let el = ancestor.as_ref().unwrap_or(hit);
            if let Some(word) = word_in(el, &point) {
                return Some(word);
            }
            let parent = element_attr(el, "AXParent")?;
            ancestor = Some(parent);
        }
        None
    }

    pub fn role_of(el: &Element) -> Option<String> {
        attr(el, "AXRole").and_then(|v| string_of(&v))
    }

    /// `hit` or its nearest ancestor with role `AXWebArea` (WebKit web content).
    pub fn web_area_of(hit: &Element) -> Option<Element> {
        let mut cur: Option<Element> = None;
        for _ in 0..=WEB_AREA_DEPTH {
            let el = cur.as_ref().unwrap_or(hit);
            if role_of(el).as_deref() == Some("AXWebArea") {
                return Some(match cur {
                    Some(e) => e,
                    None => Element(hit.0.clone()),
                });
            }
            cur = Some(element_attr(el, "AXParent")?);
        }
        None
    }

    fn point_value(x: f64, y: f64) -> CFType {
        let p = CGPoint { x, y };
        unsafe { CFType::wrap_under_create_rule(AXValueCreate(AX_VALUE_CGPOINT, &p as *const CGPoint as *const c_void)) }
    }

    // Text markers (AXTextMarkerRef / AXTextMarkerRangeRef) are opaque CF
    // objects: kept as owned `CFType`s and passed back unchanged.

    fn marker_string(web: &Element, range: &CFType) -> Option<String> {
        param_attr(web, "AXStringForTextMarkerRange", range).and_then(|v| string_of(&v))
    }

    fn marker_bounds(web: &Element, range: &CFType) -> Option<Rect> {
        param_attr(web, "AXBoundsForTextMarkerRange", range).and_then(|v| rect_of(&v))
    }

    /// Range between two markers, in either order.
    fn marker_range(web: &Element, a: &CFType, b: &CFType) -> Option<CFType> {
        let pair = CFArray::from_CFTypes(&[a.clone(), b.clone()]);
        param_attr(web, "AXTextMarkerRangeForUnorderedTextMarkers", &pair.as_CFType())
    }

    /// The web area's selection (cleaned text, bounds); `None` if empty.
    pub fn web_selection(web: &Element) -> Option<(String, Option<Rect>)> {
        let range = attr(web, "AXSelectedTextMarkerRange")?;
        let text = clean(&marker_string(web, &range)?)?;
        Some((text, marker_bounds(web, &range)))
    }

    /// Tier W: the word under (x, y) in web area `web`, or why there is none.
    /// A point in the 1–2 pt gap between two blocks makes WebKit resolve the
    /// marker somewhere unrelated (rejected by the bounds check), so small
    /// vertical nudges are tried too.
    pub fn web_word_at(web: &Element, x: f64, y: f64) -> Result<(String, Option<Rect>), String> {
        let mut first_err = None;
        for dy in [0.0, 3.0, -3.0] {
            match web_word_at_exact(web, x, y + dy) {
                Ok(w) => return Ok(w),
                Err(e) => {
                    first_err.get_or_insert(e);
                }
            }
        }
        Err(first_err.unwrap_or_default())
    }

    fn web_word_at_exact(web: &Element, x: f64, y: f64) -> Result<(String, Option<Rect>), String> {
        let at = param_attr(web, "AXTextMarkerForPosition", &point_value(x, y))
            .ok_or("AXTextMarkerForPosition unsupported or no text here")?;
        // The marker is the caret position nearest the point, which may sit
        // after the character under it: pick the marker that starts it.
        let on_point = |a: &CFType, b: &CFType| {
            marker_range(web, a, b).and_then(|r| marker_bounds(web, &r)).is_some_and(|r| point_near_rect(r, x, y, 0.5))
        };
        let next = param_attr(web, "AXNextTextMarkerForTextMarker", &at);
        let prev = param_attr(web, "AXPreviousTextMarkerForTextMarker", &at);
        let ch = if next.as_ref().is_some_and(|n| on_point(&at, n)) {
            at
        } else if prev.as_ref().is_some_and(|p| on_point(p, &at)) {
            prev.unwrap()
        } else {
            at
        };
        let mut cands: Vec<(String, Option<Rect>, CFType)> = Vec::new();
        for name in ["AXRightWordTextMarkerRangeForTextMarker", "AXLeftWordTextMarkerRangeForTextMarker"] {
            if let Some(r) = param_attr(web, name, &ch) {
                if let Some(s) = marker_string(web, &r) {
                    let b = marker_bounds(web, &r);
                    cands.push((s, b, r));
                }
            }
        }
        if cands.is_empty() {
            return Err("no word ranges for the marker".into());
        }
        let plain: Vec<(String, Option<Rect>)> = cands.iter().map(|(s, b, _)| (s.clone(), *b)).collect();
        let Some(word) = pick_web_word(&plain, x, y) else {
            let seen: Vec<String> = plain.iter().map(|(s, b)| format!("{:?}@{:?}", s, b.map(|r| (r.x as i64, r.y as i64, r.w as i64, r.h as i64)))).collect();
            return Err(format!("no candidate word under the point (candidates {})", seen.join(", ")));
        };
        let chosen = cands.iter().find(|(s, _, _)| s.trim() == word);
        let bounds = chosen.and_then(|(_, b, _)| *b);
        // UTF-16 offset of the character under the cursor inside the word
        // (only needed to narrow long CJK runs).
        let offset = if super::has_cjk(&word) {
            chosen
                .and_then(|(s, _, r)| {
                    let start = param_attr(web, "AXStartTextMarkerForTextMarkerRange", r)?;
                    let lead = marker_string(web, &marker_range(web, &start, &ch)?)?;
                    let skipped = s.encode_utf16().count() - s.trim_start().encode_utf16().count();
                    lead.encode_utf16().count().checked_sub(skipped)
                })
        } else {
            None
        };
        Ok((narrow_web_word(&word, offset), bounds))
    }

    fn word_in(el: &Element, point: &CFType) -> Option<(String, Option<Rect>)> {
        let hit = range_of(&param_attr(el, "AXRangeForPosition", point)?)?;
        if hit.location < 0 {
            return None;
        }
        let total = attr(el, "AXNumberOfCharacters")
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64())
            .map(|n| n as isize);
        let start = (hit.location - WINDOW).max(0);
        let mut end = hit.location + WINDOW;
        if let Some(t) = total {
            end = end.min(t);
        }
        if end <= start {
            return None;
        }
        let text = string_of(&param_attr(el, "AXStringForRange", &range_value(CFRange::init(start, end - start)))?)?;
        let (word, a, len) = word_range_at_utf16(&text, (hit.location - start) as usize)?;
        let range = range_value(CFRange::init(start + a as isize, len as isize));
        let bounds = param_attr(el, "AXBoundsForRange", &range).and_then(|v| rect_of(&v));
        Some((word, bounds))
    }
}

#[cfg(target_os = "macos")]
mod pasteboard {
    //! Tier 3: synthetic ⌘C with the user's pasteboard saved and restored.
    use super::{clean, Duration, Instant};
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::NSString;

    /// How long to wait for the target app to write the pasteboard.
    const COPY_WAIT: Duration = Duration::from_millis(350);
    /// After giving up, keep watching this long (from the key press) for a
    /// late write, and put the user's pasteboard back if one comes.
    const LATE_WAIT: Duration = Duration::from_millis(1000);
    /// ⌘C ⌘C ignores key presses and pasteboard changes for this long.
    const IGNORE: Duration = Duration::from_millis(1200);
    /// nspasteboard.org marker: clipboard managers skip transient writes.
    const TRANSIENT_TYPE: &str = "org.nspasteboard.TransientType";

    /// Owned (+1) NSPasteboardItem copies. NSPasteboardItem is a plain data
    /// container, safe to hand to another thread once nothing else uses it.
    struct Saved(Vec<*mut AnyObject>);
    unsafe impl Send for Saved {}

    impl Saved {
        unsafe fn release(self) {
            for item in self.0 {
                let _: () = msg_send![item, release];
            }
        }
    }

    unsafe fn general() -> Option<*mut AnyObject> {
        let pb: *mut AnyObject = msg_send![AnyClass::get(c"NSPasteboard")?, generalPasteboard];
        (!pb.is_null()).then_some(pb)
    }

    unsafe fn change_count(pb: *mut AnyObject) -> isize {
        msg_send![pb, changeCount]
    }

    /// Copies of every pasteboard item (all types), as owned (+1) NSPasteboardItems.
    unsafe fn snapshot(pb: *mut AnyObject) -> Saved {
        let mut out = Vec::new();
        let Some(item_cls) = AnyClass::get(c"NSPasteboardItem") else { return Saved(out) };
        let items: *mut AnyObject = msg_send![pb, pasteboardItems];
        if items.is_null() {
            return Saved(out);
        }
        let n: usize = msg_send![items, count];
        for i in 0..n {
            let item: *mut AnyObject = msg_send![items, objectAtIndex: i];
            let types: *mut AnyObject = msg_send![item, types];
            if types.is_null() {
                continue;
            }
            let copy: *mut AnyObject = msg_send![item_cls, alloc];
            let copy: *mut AnyObject = msg_send![copy, init];
            let tn: usize = msg_send![types, count];
            for j in 0..tn {
                let ty: *mut AnyObject = msg_send![types, objectAtIndex: j];
                let data: *mut AnyObject = msg_send![item, dataForType: ty];
                if !data.is_null() {
                    let _: bool = msg_send![copy, setData: data, forType: ty];
                }
            }
            out.push(copy);
        }
        Saved(out)
    }

    /// Put the saved items back, marked transient so clipboard managers do
    /// not record the restore as a new copy.
    unsafe fn restore(pb: *mut AnyObject, saved: Saved) {
        let _: isize = msg_send![pb, clearContents];
        if !saved.0.is_empty() {
            if let Some(data_cls) = AnyClass::get(c"NSData") {
                let empty: *mut AnyObject = msg_send![data_cls, data];
                let ty = NSString::from_str(TRANSIENT_TYPE);
                if let Some(first) = saved.0.first() {
                    let _: bool = msg_send![*first, setData: empty, forType: &*ty];
                }
            }
            let ok = match AnyClass::get(c"NSArray") {
                Some(arr_cls) => {
                    let arr: *mut AnyObject =
                        msg_send![arr_cls, arrayWithObjects: saved.0.as_ptr(), count: saved.0.len()];
                    let ok: bool = msg_send![pb, writeObjects: arr];
                    ok
                }
                None => false,
            };
            if !ok {
                log::warn!("lookup: could not restore the pasteboard ({} items); it is now empty", saved.0.len());
            }
        }
        saved.release();
    }

    unsafe fn string(pb: *mut AnyObject) -> Option<String> {
        let ty = NSString::from_str("public.utf8-plain-text");
        let s: *mut AnyObject = msg_send![pb, stringForType: &*ty];
        (!s.is_null()).then(|| (*(s as *const NSString)).to_string())
    }

    fn post_cmd_c(keycode: u16) -> bool {
        use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
        let Ok(src) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else { return false };
        let (Ok(down), Ok(up)) = (
            CGEvent::new_keyboard_event(src.clone(), keycode, true),
            CGEvent::new_keyboard_event(src, keycode, false),
        ) else {
            return false;
        };
        down.set_flags(CGEventFlags::CGEventFlagCommand);
        up.set_flags(CGEventFlags::CGEventFlagCommand);
        down.post(CGEventTapLocation::HID);
        up.post(CGEventTapLocation::HID);
        true
    }

    /// `keycode`: virtual key that types "c" in the current layout.
    pub fn copy_selection_via_cmd_c(keycode: u16) -> Option<String> {
        objc2::rc::autoreleasepool(|_| unsafe {
            let pb = general()?;
            crate::hotkeys::double_cmd_c::ignore_synthetic_copy(IGNORE);
            let saved = snapshot(pb);
            let before = change_count(pb);
            if !post_cmd_c(keycode) {
                saved.release();
                return None;
            }
            let pressed = Instant::now();
            let mut changed = false;
            while pressed.elapsed() < COPY_WAIT {
                std::thread::sleep(Duration::from_millis(20));
                if change_count(pb) != before {
                    changed = true;
                    // Let multi-type writers finish before reading.
                    std::thread::sleep(Duration::from_millis(30));
                    break;
                }
            }
            if !changed {
                // Nothing yet: probably no selection. A slow app may still
                // write; watch a little longer in the background and put the
                // user's pasteboard back if it does.
                std::thread::spawn(move || late_restore(saved, before, pressed));
                return None;
            }
            let text = string(pb);
            crate::hotkeys::double_cmd_c::ignore_synthetic_copy(IGNORE);
            restore(pb, saved);
            text.as_deref().and_then(clean)
        })
    }

    fn late_restore(saved: Saved, before: isize, pressed: Instant) {
        objc2::rc::autoreleasepool(|_| unsafe {
            let Some(pb) = general() else {
                saved.release();
                return;
            };
            while pressed.elapsed() < LATE_WAIT {
                std::thread::sleep(Duration::from_millis(25));
                if change_count(pb) != before {
                    std::thread::sleep(Duration::from_millis(30));
                    log::info!("lookup: late synthetic copy after {:?}; pasteboard restored", pressed.elapsed());
                    crate::hotkeys::double_cmd_c::ignore_synthetic_copy(IGNORE);
                    restore(pb, saved);
                    return;
                }
            }
            saved.release();
        })
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn word_in_latin_text() {
        let s = "The quick brown fox";
        assert_eq!(word_at_utf16(s, 5).as_deref(), Some("quick"));
        assert_eq!(word_at_utf16(s, 0).as_deref(), Some("The"));
        assert_eq!(word_at_utf16(s, 18).as_deref(), Some("fox"));
    }

    #[test]
    fn whitespace_and_out_of_range_give_none() {
        assert_eq!(word_at_utf16("ab cd", 2), None);
        assert_eq!(word_at_utf16("ab", 9), None);
        assert_eq!(word_at_utf16("", 0), None);
    }

    #[test]
    fn utf16_index_after_emoji() {
        // "😀" is two UTF-16 units; index 3 is inside "hello".
        assert_eq!(word_at_utf16("😀 hello", 3).as_deref(), Some("hello"));
    }

    #[test]
    fn word_range_reports_utf16_span() {
        assert_eq!(word_range_at_utf16("The quick", 6), Some(("quick".to_string(), 4, 5)));
        // After an emoji (2 UTF-16 units) the start is in UTF-16 units.
        assert_eq!(word_range_at_utf16("😀 hello", 4), Some(("hello".to_string(), 3, 5)));
    }

    #[test]
    fn cjk_gives_a_word_not_the_whole_run() {
        let s = "我今天去醫院看病";
        let w = word_at_utf16(s, 4).expect("a word");
        assert!(!w.is_empty() && w.chars().count() < s.chars().count(), "got {w}");
        assert!(s.contains(&w));
    }

    #[test]
    fn point_near_rect_uses_tolerance() {
        let r = Rect { x: 100.0, y: 50.0, w: 200.0, h: 20.0 };
        assert!(point_near_rect(r, 150.0, 60.0, 4.0));
        assert!(point_near_rect(r, 97.0, 48.0, 4.0)); // just outside, within tolerance
        assert!(point_near_rect(r, 304.0, 74.0, 4.0));
        assert!(!point_near_rect(r, 305.0, 60.0, 4.0));
        assert!(!point_near_rect(r, 150.0, 80.0, 4.0));
        assert!(!point_near_rect(r, 150.0, 60.0 - 15.0, 4.0));
    }

    #[test]
    fn point_near_rect_rejects_empty_rects() {
        let caret = Rect { x: 100.0, y: 50.0, w: 0.0, h: 18.0 };
        assert!(!point_near_rect(caret, 100.0, 55.0, 4.0));
        let nan = Rect { x: f64::NAN, y: 0.0, w: 10.0, h: 10.0 };
        assert!(!point_near_rect(nan, 1.0, 1.0, 4.0));
    }

    #[test]
    fn ax_poor_bundles() {
        assert!(is_ax_poor_bundle(&["Electron Framework.framework", "Squirrel.framework"]));
        assert!(is_ax_poor_bundle(&["Google Chrome Framework.framework"]));
        assert!(is_ax_poor_bundle(&["Chromium Embedded Framework.framework"]));
        assert!(!is_ax_poor_bundle(&["Sparkle.framework", "Mantle.framework"]));
        assert!(!is_ax_poor_bundle::<&str>(&[]));
    }

    #[test]
    fn keycode_for_c_follows_layout() {
        // QWERTY: keycode 8 is "c".
        let qwerty = |k: u16| (k == 8).then_some('c');
        assert_eq!(find_keycode_for_c(qwerty), 8);
        // Dvorak: "c" sits on the QWERTY "i" key (keycode 34).
        let dvorak = |k: u16| match k {
            34 => Some('c'),
            8 => Some('j'),
            _ => None,
        };
        assert_eq!(find_keycode_for_c(dvorak), 34);
        // No "c" anywhere (e.g. a non-Latin layout): fall back to 8.
        assert_eq!(find_keycode_for_c(|_| None), DEFAULT_KEYCODE_C);
    }

    #[test]
    fn only_synthetic_copy_forces_the_picker() {
        assert!(Tier::SyntheticCopy.must_choose());
        assert!(!Tier::Selection.must_choose());
        assert!(!Tier::WordAtCursor.must_choose());
    }

    #[test]
    fn web_tiers_never_force_the_picker() {
        assert!(!Tier::WebWord.must_choose());
        assert!(!Tier::WebSelection.must_choose());
    }

    #[test]
    fn has_cjk_detects_scripts() {
        assert!(has_cjk("醫院"));
        assert!(has_cjk("カタカナ"));
        assert!(has_cjk("한국어"));
        assert!(has_cjk("heart 心臟"));
        assert!(!has_cjk("heart"));
        assert!(!has_cjk("café – naïve"));
    }

    fn r(x: f64, y: f64, w: f64, h: f64) -> Option<Rect> {
        Some(Rect { x, y, w, h })
    }

    #[test]
    fn pick_web_word_prefers_the_candidate_under_the_point() {
        // Point at x=130: on "heart" (left word), not on "pumps" (right word).
        let c = vec![("pumps".to_string(), r(150.0, 10.0, 40.0, 16.0)), ("heart".to_string(), r(110.0, 10.0, 36.0, 16.0))];
        assert_eq!(pick_web_word(&c, 130.0, 18.0).as_deref(), Some("heart"));
        assert_eq!(pick_web_word(&c, 160.0, 18.0).as_deref(), Some("pumps"));
    }

    #[test]
    fn pick_web_word_rejects_blank_and_off_text_points() {
        let c = vec![(" ".to_string(), r(146.0, 10.0, 4.0, 16.0)), ("heart".to_string(), r(110.0, 10.0, 36.0, 16.0))];
        // On the space between words: no word.
        assert_eq!(pick_web_word(&c, 148.0, 18.0), None);
        // Far below the line (tap in a margin): no word.
        assert_eq!(pick_web_word(&c, 120.0, 80.0), None);
        // Punctuation only never counts.
        let p = vec![("—".to_string(), r(100.0, 10.0, 10.0, 16.0))];
        assert_eq!(pick_web_word(&p, 105.0, 18.0), None);
    }

    #[test]
    fn pick_web_word_without_bounds_takes_first_real_word() {
        let c = vec![(" ".to_string(), None), (" heart ".to_string(), None)];
        assert_eq!(pick_web_word(&c, 0.0, 0.0).as_deref(), Some("heart"));
        assert_eq!(pick_web_word(&[], 0.0, 0.0), None);
    }

    #[test]
    fn narrow_web_word_splits_long_cjk_runs_only() {
        let run = "我今天去醫院看病";
        let w = narrow_web_word(run, Some(4));
        assert!(w.chars().count() < run.chars().count() && run.contains(&w), "got {w}");
        // Offset past the end is clamped, not dropped.
        let tail = narrow_web_word(run, Some(99));
        assert!(run.ends_with(&tail), "got {tail}");
        // Short CJK words, Latin words and unknown offsets are kept whole.
        assert_eq!(narrow_web_word("醫院", Some(1)), "醫院");
        assert_eq!(narrow_web_word("cardiomyopathy", Some(3)), "cardiomyopathy");
        assert_eq!(narrow_web_word(run, None), run);
    }

    /// Real check against whatever app is under the mouse (e.g. Safari):
    /// `cargo test --lib live_capture -- --ignored --nocapture`.
    /// `PEEKA_AT="x,y"` warps the cursor there first (global top-left points).
    /// Needs Accessibility for the process running the test.
    #[test]
    #[ignore]
    fn live_capture_at_mouse() {
        use core_graphics::display::CGDisplay;
        use core_graphics::event::CGEvent;
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
        use core_graphics::geometry::CGPoint;
        #[link(name = "ApplicationServices", kind = "framework")]
        extern "C" {
            fn AXIsProcessTrusted() -> bool;
        }
        let _ = env_logger::builder().is_test(true).filter_level(log::LevelFilter::Info).try_init();
        let trusted = unsafe { AXIsProcessTrusted() };
        println!("AXIsProcessTrusted = {trusted}");
        if !trusted {
            println!("SKIP: this process has no Accessibility permission");
            return;
        }
        if let Ok(at) = std::env::var("PEEKA_AT") {
            let v: Vec<f64> = at.split(',').filter_map(|s| s.trim().parse().ok()).collect();
            if let [x, y] = v[..] {
                let _ = CGDisplay::warp_mouse_cursor_position(CGPoint::new(x, y));
                std::thread::sleep(Duration::from_millis(200));
            }
        }
        let src = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).expect("event source");
        let p = CGEvent::new(src).expect("event").location();
        println!("mouse at ({:.0}, {:.0})", p.x, p.y);
        match capture_text_at(p.x, p.y, || DEFAULT_KEYCODE_C) {
            Some(c) => println!("CAPTURED via {}: {:?} (app {:?}, bounds {:?})", c.tier.label(), c.text, c.source_app, c.bounds),
            None => println!("CAPTURED nothing"),
        }
    }

    #[test]
    fn clean_trims_and_rejects_blank() {
        assert_eq!(clean("  hi \n").as_deref(), Some("hi"));
        assert_eq!(clean(" \t"), None);
    }
}
