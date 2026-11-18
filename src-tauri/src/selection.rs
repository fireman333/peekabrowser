//! Read the text the user is pointing at in another app (three-finger lookup).
//!
//! Tiers, tried in order:
//! 1. `AXSelectedText` of the focused element in the frontmost app, but only
//!    when the tap lands on that selection (`AXBoundsForRange` of
//!    `AXSelectedTextRange`, with a small tolerance) and the element under the
//!    cursor belongs to the frontmost app. Like the system Look Up gesture.
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
    /// Synthetic ⌘C: what the app chose to copy, possibly not what was pointed at.
    SyntheticCopy,
}

impl Tier {
    pub fn label(self) -> &'static str {
        match self {
            Tier::Selection => "AXSelectedText",
            Tier::WordAtCursor => "AXRangeForPosition",
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

/// `x`, `y`: cursor in global top-left-origin points (same space as AX).
/// `copy_keycode` is only called when tier 3 runs (see [`copy_keycode`]).
#[cfg(target_os = "macos")]
pub fn capture_text_at(x: f64, y: f64, copy_keycode: impl FnOnce() -> u16) -> Option<Capture> {
    let own_pid = std::process::id() as i32;
    let hit = ax::element_at(x, y);
    let hit_pid = hit.as_ref().and_then(ax::pid_of);
    if hit_pid == Some(own_pid) {
        log::info!("lookup: cursor is over Peekabrowser itself; ignored");
        return None;
    }
    let front = frontmost_pid().filter(|p| *p != own_pid);
    // The cursor is over the app that owns the focus (and so any selection).
    let on_front = front.filter(|f| hit_pid == Some(*f));
    let sel = on_front.and_then(ax::selection);

    // 1. Existing selection, only when the tap is on it.
    if let (Some(pid), Some(s)) = (on_front, sel.as_ref()) {
        if let (Some(text), Some(b)) = (s.text.as_ref(), s.bounds) {
            if point_near_rect(b, x, y, SELECTION_TOLERANCE) {
                return Some(Capture { text: text.clone(), tier: Tier::Selection, source_app: app_name(pid) });
            }
        }
    }
    // 2. Word under the cursor.
    if let Some(el) = hit.as_ref() {
        if let Some(text) = ax::word_at_position(el, x, y) {
            let source_app = hit_pid.and_then(app_name);
            return Some(Capture { text, tier: Tier::WordAtCursor, source_app });
        }
    }
    // 1b. No word there: fall back to the selection in the same app.
    if let (Some(pid), Some(text)) = (on_front, sel.as_ref().and_then(|s| s.text.clone())) {
        return Some(Capture { text, tier: Tier::Selection, source_app: app_name(pid) });
    }
    // 3. Synthetic ⌘C, gated (see the module docs).
    let pid = on_front?;
    let allowed = match sel.as_ref().and_then(|s| s.length) {
        Some(len) => len > 0,
        None => is_ax_poor_app(pid),
    };
    if !allowed {
        log::info!("lookup: no AX-confirmed selection in the frontmost app; synthetic copy skipped");
        return None;
    }
    let text = pasteboard::copy_selection_via_cmd_c(copy_keycode())?;
    Some(Capture { text, tier: Tier::SyntheticCopy, source_app: app_name(pid) })
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
        clean(&word)
    }
}

#[cfg(target_os = "macos")]
mod ax {
    //! Minimal raw Accessibility FFI (ApplicationServices / HIServices).
    use super::{clean, word_at_utf16, Rect, AX_TIMEOUT_SECS};
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
    pub fn word_at_position(hit: &Element, x: f64, y: f64) -> Option<String> {
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

    fn word_in(el: &Element, point: &CFType) -> Option<String> {
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
        word_at_utf16(&text, (hit.location - start) as usize)
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
    fn clean_trims_and_rejects_blank() {
        assert_eq!(clean("  hi \n").as_deref(), Some("hi"));
        assert_eq!(clean(" \t"), None);
    }
}
