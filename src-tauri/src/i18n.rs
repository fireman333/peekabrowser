//! Interface language for strings shown by the Rust side (menu bar, notifications,
//! window titles, update status). Mirrors the frontend dictionary in `src/i18n.ts`.
//! Default is Traditional Chinese (Taiwan); "en" switches to English.

use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(false);

/// Normalize a stored language code: anything but "en" means zh-TW.
pub fn normalize(code: &str) -> &'static str {
    if code == "en" {
        "en"
    } else {
        "zh-TW"
    }
}

pub fn set_language(code: &str) {
    ENGLISH.store(normalize(code) == "en", Ordering::SeqCst);
}

pub fn is_english() -> bool {
    ENGLISH.load(Ordering::SeqCst)
}

/// Pick the string for the current language.
pub fn tr(zh: &'static str, en: &'static str) -> &'static str {
    if is_english() {
        en
    } else {
        zh
    }
}

/// Pick a format template for the current language and substitute `{}` in order.
pub fn trf(zh: &str, en: &str, args: &[&str]) -> String {
    let mut out = String::new();
    let mut rest = if is_english() { en } else { zh };
    for a in args {
        match rest.find("{}") {
            Some(i) => {
                out.push_str(&rest[..i]);
                out.push_str(a);
                rest = &rest[i + 2..];
            }
            None => break,
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_codes() {
        assert_eq!(normalize("en"), "en");
        assert_eq!(normalize("zh-TW"), "zh-TW");
        assert_eq!(normalize(""), "zh-TW");
        assert_eq!(normalize("fr"), "zh-TW");
    }

    #[test]
    fn formats_in_order() {
        // Language is global; only exercise the formatter's substitution here.
        let s = trf("版本 {} 可用 {}", "Version {} available {}", &["2.0.2", "!"]);
        assert!(s == "版本 2.0.2 可用 !" || s == "Version 2.0.2 available !");
    }
}
