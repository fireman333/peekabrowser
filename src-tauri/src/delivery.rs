//! Typed query delivery into destination pages, plus the bounded generation
//! watcher that holds an activity token only while an answer is being written.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

pub const PROVIDERS_JS: &str = include_str!("../assets/providers.js");

/// What the picker will send. Image and prompt are separate fields, so a
/// prompt prefix can never turn a screenshot into "text".
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Payload {
    Text { text: String },
    Image { path: PathBuf },
}

/// Captured at trigger time (double copy / screenshot), read by the picker.
#[derive(Clone, Debug, Default)]
pub struct PendingQuery {
    pub payload: Option<Payload>,
    pub source_app: Option<String>,
}

/// Holds the pending query for the picker popup.
pub struct PickerState(pub std::sync::Mutex<PendingQuery>);

/// Build `JSON.stringify(<providers>.<method>(<args>))`; args are JSON-serialized
/// so no prompt text is ever spliced into JS source.
pub fn provider_call(method: &str, args: &[serde_json::Value]) -> String {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    format!("JSON.stringify(({}).{}({}))", PROVIDERS_JS, method, args.join(","))
}

#[derive(Debug, Default, Deserialize)]
pub struct PageStatus {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub generating: bool,
    #[serde(default)]
    pub draft: bool,
    #[serde(default, rename = "knowsCompletion")]
    pub knows_completion: bool,
}

#[derive(Debug, Default, Deserialize)]
struct InjectResult {
    #[serde(default)]
    status: String,
}

/// Call a provider method on a viewer from a background thread.
pub fn call<T: for<'de> Deserialize<'de>>(app: &AppHandle, label: &str, method: &str, args: &[serde_json::Value]) -> Option<T> {
    let w = app.get_webview_window(label)?;
    let raw = crate::native::eval_blocking(&w, &provider_call(method, args), Duration::from_secs(4))?;
    serde_json::from_str(&raw).ok()
}

pub fn page_status(app: &AppHandle, label: &str) -> Option<PageStatus> {
    call(app, label, "status", &[])
}

/// A delivery target: the viewer slot and the generation it had when the
/// query was created. If the slot is recycled for another page, the
/// generation changes and every pending step for the old query stops.
#[derive(Clone)]
pub struct Target {
    pub page_id: String,
    pub label: String,
    pub slot_gen: u64,
}

impl Target {
    fn alive(&self) -> bool {
        crate::panel::slot_generation(&self.label) == Some(self.slot_gen)
    }
}

pub struct Delivery {
    pub target: Target,
    pub payload: Payload,
    /// Destination prompt prefix (or template); sent with the image, or
    /// prepended to text.
    pub prompt: String,
    pub record_id: Option<String>,
}

fn set_status(app: &AppHandle, record_id: &Option<String>, status: &str) {
    if let (Some(id), Some(store)) = (record_id, app.try_state::<crate::records::RecordStore>()) {
        let _ = store.set_query_status(id, status);
    }
}

/// Deliver in the background: wait for the page, inject once, stop on ack.
pub fn spawn(app: AppHandle, d: Delivery) {
    std::thread::spawn(move || {
        let request_id = uuid::Uuid::new_v4().to_string();
        let ok = match &d.payload {
            Payload::Text { text } => {
                let full = format!("{}{}", d.prompt, text);
                inject_text(&app, &d.target, &request_id, &full)
            }
            Payload::Image { path } => deliver_image(&app, &d.target, &request_id, path, &d.prompt),
        };
        if !ok {
            log::warn!("delivery: failed for page {}", d.target.page_id);
            set_status(&app, &d.record_id, "failed");
            crate::lifecycle::notify(&app, "Couldn't place the query on this page — paste it manually (⌘V).");
            if let Payload::Text { text } = &d.payload {
                copy_to_clipboard(&app, &format!("{}{}", d.prompt, text));
            }
            return;
        }
        set_status(&app, &d.record_id, "injected");
        watch_generation(&app, &d.target, &d.record_id);
    });
}

/// Poll page readiness at a modest rate for a bounded time; the first
/// successful injection ends the loop. Request id makes retries idempotent.
fn inject_text(app: &AppHandle, t: &Target, request_id: &str, text: &str) -> bool {
    let payload = serde_json::json!({ "requestId": request_id, "kind": "text", "text": text, "submit": true });
    retry_until(t, Duration::from_secs(25), || {
        let r: Option<InjectResult> = call(app, &t.label, "inject", &[payload.clone()]);
        match r.map(|r| r.status).as_deref() {
            Some("ok") | Some("duplicate") => Step::Done(true),
            _ => Step::Retry,
        }
    })
}

fn deliver_image(app: &AppHandle, t: &Target, request_id: &str, path: &PathBuf, prompt: &str) -> bool {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            log::warn!("delivery: cannot read screenshot: {}", e);
            return false;
        }
    };
    let data_url = format!(
        "data:image/png;base64,{}",
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data)
    );
    let payload = serde_json::json!({ "requestId": request_id, "kind": "image", "imageDataUrl": data_url });

    let mut unsupported = false;
    let pasted = retry_until(t, Duration::from_secs(30), || {
        let r: Option<InjectResult> = call(app, &t.label, "inject", &[payload.clone()]);
        match r.map(|r| r.status).as_deref() {
            Some("pasted") | Some("duplicate") => Step::Done(true),
            Some("unsupported") => {
                unsupported = true;
                Step::Done(false)
            }
            _ => Step::Retry,
        }
    });

    if unsupported {
        // Destination can't take images: send recognized text instead.
        log::info!("delivery: destination has no image input, using OCR text");
        return match crate::commands::ocr_image(path) {
            Ok(text) if !text.trim().is_empty() => inject_text(app, t, request_id, &format!("{}{}", prompt, text)),
            _ => false,
        };
    }
    if !pasted {
        return false;
    }

    // Only fall back to drag-and-drop when the paste demonstrably didn't attach.
    std::thread::sleep(Duration::from_millis(1200));
    let accepted = |app: &AppHandle| {
        call::<serde_json::Value>(app, &t.label, "imageAccepted", &[serde_json::json!(request_id)])
            .and_then(|v| v.get("accepted").and_then(|a| a.as_bool()))
            .unwrap_or(false)
    };
    if t.alive() && !accepted(app) {
        log::info!("delivery: paste not accepted, trying drop");
        let _: Option<serde_json::Value> = call(app, &t.label, "dropImage", &[payload.clone()]);
    }
    if t.alive() && !prompt.is_empty() {
        let _: Option<serde_json::Value> = call(app, &t.label, "insertPrompt", &[serde_json::json!(prompt)]);
    }
    t.alive()
}

enum Step {
    Done(bool),
    Retry,
}

fn retry_until(t: &Target, limit: Duration, mut f: impl FnMut() -> Step) -> bool {
    let start = Instant::now();
    let mut delay = Duration::from_millis(300);
    while start.elapsed() < limit {
        std::thread::sleep(delay);
        if !t.alive() {
            return false;
        }
        if let Step::Done(v) = f() {
            return v;
        }
        delay = (delay + Duration::from_millis(200)).min(Duration::from_millis(1000));
    }
    false
}

/// Bounded watcher: holds an activity token while the page is generating, so
/// a hidden panel can finish the answer. Never stops or reloads the page.
fn watch_generation(app: &AppHandle, t: &Target, record_id: &Option<String>) {
    let work_id = format!("gen-{}", uuid::Uuid::new_v4());
    crate::activity::begin(&work_id, &t.page_id);
    let start = Instant::now();
    let mut seen = false;
    let mut idle_checks = 0;
    while start.elapsed() < crate::activity::MAX_WORK_DURATION {
        std::thread::sleep(Duration::from_secs(if seen { 3 } else { 2 }));
        if !t.alive() {
            set_status(app, record_id, "cancelled");
            break;
        }
        let Some(st) = page_status(app, &t.label) else {
            idle_checks += 1;
            if idle_checks > 10 {
                break;
            }
            continue;
        };
        let knows = st.knows_completion;
        crate::lifecycle::set_page_generating(app, &t.page_id, st.generating, Some(&st.title), Some(&st.url));
        if st.generating {
            if !seen {
                set_status(app, record_id, "generating");
            }
            seen = true;
            idle_checks = 0;
        } else if seen {
            idle_checks += 1;
            if idle_checks >= 2 {
                set_status(app, record_id, "completed");
                break;
            }
        } else if !knows || start.elapsed() > Duration::from_secs(45) {
            // Unknown site, or nothing observed: don't keep the app awake guessing.
            break;
        }
    }
    if let (Some(id), Some(store)) = (record_id, app.try_state::<crate::records::RecordStore>()) {
        if let Some(st) = page_status(app, &t.label).filter(|_| t.alive()) {
            let _ = store.set_conversation_url(id, &st.url);
        }
    }
    crate::lifecycle::set_page_generating(app, &t.page_id, false, None, None);
    crate::activity::end(&work_id);
}

pub fn copy_to_clipboard(app: &AppHandle, text: &str) {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    let _ = app.clipboard().write_text(text.to_string());
}
