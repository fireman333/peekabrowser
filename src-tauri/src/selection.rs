//! Read the text the user is pointing at in another app (three-finger lookup).
//!
//! Tiers, tried in order:
//! 1. `AXSelectedText` of the focused element in the frontmost app (an
//!    existing selection wins).
//! 2. The word under the cursor: `AXUIElementCopyElementAtPosition` →
//!    `AXRangeForPosition` → `AXStringForRange` on a small window around that
//!    index → word boundaries from `CFStringTokenizer` (handles CJK too).
//! 3. Last resort: a synthetic ⌘C (CGEvent) with the pasteboard saved before
//!    and restored afterwards. ⌘C ⌘C detection is told to ignore it.
//!
//! Never synthesizes clicks. Needs Accessibility. All calls here are safe off
//! the main thread (AX and NSPasteboard are thread-safe; nothing touches UI).

use std::ffi::c_void;
use std::time::{Duration, Instant};

/// Text found at the cursor.
pub struct Capture {
    pub text: String,
    /// Which tier produced it (for logs).
    pub tier: &'static str,
    /// Name of the app that owns the text, if known.
    pub source_app: Option<String>,
}

/// Longest text taken from a selection (the picker/destinations cap anyway).
const MAX_CHARS: usize = 20_000;
/// Per-call AX timeout so a hung app cannot stall the worker thread.
const AX_TIMEOUT_SECS: f32 = 0.3;

/// `x`, `y`: cursor in global top-left-origin points (same space as AX).
#[cfg(target_os = "macos")]
pub fn capture_text_at(x: f64, y: f64) -> Option<Capture> {
    let own_pid = std::process::id() as i32;
    let hit = ax::element_at(x, y);
    let hit_pid = hit.as_ref().and_then(ax::pid_of);
    if hit_pid == Some(own_pid) {
        log::info!("lookup: cursor is over Peekabrowser itself; ignored");
        return None;
    }
    let front = frontmost_pid().filter(|p| *p != own_pid);

    // 1. Existing selection in the frontmost app.
    if let Some(pid) = front {
        if let Some(text) = ax::selected_text(pid) {
            return Some(Capture { text, tier: "AXSelectedText", source_app: app_name(pid) });
        }
    }
    // 2. Word under the cursor.
    if let Some(el) = hit.as_ref() {
        if let Some(text) = ax::word_at_position(el, x, y) {
            let source_app = hit_pid.and_then(app_name);
            return Some(Capture { text, tier: "AXRangeForPosition", source_app });
        }
    }
    // 3. Synthetic ⌘C (only copies an existing selection the AX tiers could not read).
    if let Some(pid) = front {
        if let Some(text) = pasteboard::copy_selection_via_cmd_c() {
            return Some(Capture { text, tier: "synthetic ⌘C", source_app: app_name(pid) });
        }
    }
    None
}

#[cfg(not(target_os = "macos"))]
pub fn capture_text_at(_x: f64, _y: f64) -> Option<Capture> {
    None
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
    use super::{clean, word_at_utf16, AX_TIMEOUT_SECS};
    use core_foundation::base::{CFRange, CFType, CFTypeRef, TCFType};
    use core_foundation::number::CFNumber;
    use core_foundation::string::{CFString, CFStringRef};
    use std::ffi::c_void;

    type AXUIElementRef = *const c_void;
    const AX_OK: i32 = 0;
    const AX_VALUE_CGPOINT: u32 = 1;
    const AX_VALUE_CFRANGE: u32 = 4;
    /// Characters fetched on each side of the hit index.
    const WINDOW: isize = 64;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
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
        /// Takes ownership of a +1 reference.
        unsafe fn own(r: AXUIElementRef) -> Option<Self> {
            (!r.is_null()).then(|| {
                AXUIElementSetMessagingTimeout(r, AX_TIMEOUT_SECS);
                Element(CFType::wrap_under_create_rule(r))
            })
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
        // The value is an AXUIElement; retain it under our own wrapper.
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
            let sys = Element::own(AXUIElementCreateSystemWide())?;
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

    /// Tier 1: the focused element's selected text in app `pid`.
    pub fn selected_text(pid: i32) -> Option<String> {
        let app = unsafe { Element::own(AXUIElementCreateApplication(pid))? };
        let focused = element_attr(&app, "AXFocusedUIElement")?;
        clean(&string_of(&attr(&focused, "AXSelectedText")?)?)
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

    const KEYCODE_C: u16 = 8;
    /// How long to wait for the target app to write the pasteboard.
    const COPY_WAIT: Duration = Duration::from_millis(350);
    /// ⌘C ⌘C ignores key presses and pasteboard changes for this long.
    const IGNORE: Duration = Duration::from_millis(1200);

    unsafe fn general() -> Option<*mut AnyObject> {
        let pb: *mut AnyObject = msg_send![AnyClass::get(c"NSPasteboard")?, generalPasteboard];
        (!pb.is_null()).then_some(pb)
    }

    unsafe fn change_count(pb: *mut AnyObject) -> isize {
        msg_send![pb, changeCount]
    }

    /// Copies of every pasteboard item (all types), as owned (+1) NSPasteboardItems.
    unsafe fn snapshot(pb: *mut AnyObject) -> Vec<*mut AnyObject> {
        let mut out = Vec::new();
        let Some(item_cls) = AnyClass::get(c"NSPasteboardItem") else { return out };
        let items: *mut AnyObject = msg_send![pb, pasteboardItems];
        if items.is_null() {
            return out;
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
        out
    }

    unsafe fn restore(pb: *mut AnyObject, saved: Vec<*mut AnyObject>) {
        let _: isize = msg_send![pb, clearContents];
        if !saved.is_empty() {
            if let Some(arr_cls) = AnyClass::get(c"NSArray") {
                let arr: *mut AnyObject = msg_send![arr_cls, arrayWithObjects: saved.as_ptr(), count: saved.len()];
                let _: bool = msg_send![pb, writeObjects: arr];
            }
        }
        for item in saved {
            let _: () = msg_send![item, release];
        }
    }

    unsafe fn string(pb: *mut AnyObject) -> Option<String> {
        let ty = NSString::from_str("public.utf8-plain-text");
        let s: *mut AnyObject = msg_send![pb, stringForType: &*ty];
        (!s.is_null()).then(|| (*(s as *const NSString)).to_string())
    }

    fn post_cmd_c() -> bool {
        use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
        let Ok(src) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else { return false };
        let (Ok(down), Ok(up)) = (
            CGEvent::new_keyboard_event(src.clone(), KEYCODE_C, true),
            CGEvent::new_keyboard_event(src, KEYCODE_C, false),
        ) else {
            return false;
        };
        down.set_flags(CGEventFlags::CGEventFlagCommand);
        up.set_flags(CGEventFlags::CGEventFlagCommand);
        down.post(CGEventTapLocation::HID);
        up.post(CGEventTapLocation::HID);
        true
    }

    pub fn copy_selection_via_cmd_c() -> Option<String> {
        objc2::rc::autoreleasepool(|_| unsafe {
            let pb = general()?;
            crate::hotkeys::double_cmd_c::ignore_synthetic_copy(IGNORE);
            let saved = snapshot(pb);
            let before = change_count(pb);
            if !post_cmd_c() {
                for item in saved {
                    let _: () = msg_send![item, release];
                }
                return None;
            }
            let deadline = Instant::now() + COPY_WAIT;
            let mut changed = false;
            while Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
                if change_count(pb) != before {
                    changed = true;
                    // Let multi-type writers finish before reading.
                    std::thread::sleep(Duration::from_millis(30));
                    break;
                }
            }
            if !changed {
                for item in saved {
                    let _: () = msg_send![item, release];
                }
                return None; // nothing selected: the app did not copy
            }
            let text = string(pb);
            crate::hotkeys::double_cmd_c::ignore_synthetic_copy(IGNORE);
            restore(pb, saved);
            text.as_deref().and_then(clean)
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
    fn clean_trims_and_rejects_blank() {
        assert_eq!(clean("  hi \n").as_deref(), Some("hi"));
        assert_eq!(clean(" \t"), None);
    }
}
