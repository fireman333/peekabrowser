//! Prompts per destination and profession templates (a port of the iOS app's
//! "prompt-templates" / "translate-entry" model).
//!
//! - A prompt (`QueryLens`) is `{id, title, prompt}`. The final text sent is
//!   `prompt + destination.clip_prompt + text` (plain prefix, see [`compose`]).
//! - Seven fixed profession templates, four built-in prompts each, with IDs
//!   `<profession>.<key>`; prompts the user adds to a template get
//!   `<profession>.user.<uuid>`. Templates cannot be added, renamed or deleted;
//!   an edited template may be empty.
//! - Each destination has its own ordered list. A destination without a stored
//!   list follows the selected template. Editing the selected template carries
//!   into every destination, except prompts that destination edited itself
//!   (tracked in `edited`).
//! - Built-in text follows the interface language until edited.
//!
//! Persisted as one JSON file (`prompts.json`), so a template edit and its
//! propagation are written together (iOS needs a pending-edit journal for this
//! because it stores them as separate defaults keys).

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryLens {
    #[serde(default)]
    pub id: String,
    pub title: String,
    pub prompt: String,
}

impl QueryLens {
    pub fn new(id: &str, title: &str, prompt: &str) -> Self {
        Self { id: id.into(), title: title.into(), prompt: prompt.into() }
    }
}

/// Fixed template order (matches iOS `LensProfession.allCases`).
pub const PROFESSIONS: [&str; 7] = ["general", "student", "physician", "lawyer", "researcher", "engineer", "teacher"];

pub const DEFAULT_PROFESSION: &str = "general";

/// Unknown or missing profession → "general".
pub fn normalize_profession(p: &str) -> &'static str {
    PROFESSIONS.iter().copied().find(|x| *x == p).unwrap_or(DEFAULT_PROFESSION)
}

/// (profession, key, English title, zh-Hant title, English prompt, zh-Hant prompt).
/// Verbatim from the iOS app (QueryLens.swift + Localizable.xcstrings).
type Builtin = (&'static str, &'static str, &'static str, &'static str, &'static str, &'static str);

const BUILTINS: &[Builtin] = &[
    ("general", "explain", "Explain simply", "簡單解釋", "Explain this simply: ", "請簡單解釋："),
    ("general", "summarize", "Summarize", "摘要", "Summarize the key points of: ", "請摘要重點："),
    ("general", "proscons", "Pros and cons", "優缺點", "List the pros and cons of: ", "請列出優缺點："),
    ("general", "translate", "Translate to English", "翻成英文", "Translate into English: ", "請翻譯成英文："),
    ("student", "tutor", "Explain step by step", "逐步解釋", "Explain this step by step like a tutor: ", "請像家教一樣逐步解釋："),
    ("student", "quiz", "Quiz me", "考考我", "Write 5 quiz questions with answers about: ", "請出 5 題測驗題並附答案，主題是："),
    ("student", "keypoints", "Key points", "重點整理", "Summarize the key points to remember about: ", "請整理需要記住的重點："),
    ("student", "examples", "Give examples", "舉例", "Give 3 concrete examples of: ", "請舉 3 個具體例子："),
    ("physician", "explain", "Explain simply", "簡單解釋", "Explain this simply for a busy clinician: ", "請用忙碌臨床人員能快速看懂的方式簡單解釋："),
    ("physician", "ddx", "Differential diagnosis", "鑑別診斷", "List the differential diagnosis, most likely first, for: ", "請依可能性由高到低列出鑑別診斷："),
    ("physician", "dose", "Dose & renal adjustment", "劑量與腎功能調整", "Give the usual adult dose and the renal dose adjustment for: ", "請列出成人常用劑量與腎功能不全時的劑量調整："),
    ("physician", "guideline", "Latest guideline", "最新指引", "Summarize the latest guideline recommendations for: ", "請摘要最新的指引建議："),
    ("lawyer", "plain", "Plain-language summary", "白話摘要", "Summarize in plain language: ", "請用白話摘要："),
    ("lawyer", "issues", "Key legal issues", "法律爭點", "Identify the key legal issues in: ", "請找出主要的法律爭點："),
    ("lawyer", "law", "Relevant law", "相關法規", "List the laws and cases relevant to: ", "請列出相關的法規與判決："),
    ("lawyer", "risks", "Legal risks", "法律風險", "List the legal risks in: ", "請列出法律風險："),
    ("researcher", "evidence", "Summarize the evidence", "整理證據", "Summarize the current evidence on: ", "請整理目前的研究證據："),
    ("researcher", "papers", "Find key papers", "找關鍵論文", "Find key papers, with citations, on: ", "請找出關鍵論文並附引用："),
    ("researcher", "methods", "Critique the methods", "評論研究方法", "Critique the study design and methods of: ", "請評論研究設計與方法："),
    ("researcher", "gaps", "Research gaps", "研究缺口", "List the open research questions on: ", "請列出尚未解決的研究問題："),
    ("engineer", "explaincode", "Explain the code", "解釋程式碼", "Explain what this code does: ", "請解釋這段程式碼在做什麼："),
    ("engineer", "bugs", "Find bugs", "找錯誤", "Find bugs in this code: ", "請找出這段程式碼的錯誤："),
    ("engineer", "tests", "Write tests", "寫測試", "Write unit tests for: ", "請寫單元測試："),
    ("engineer", "compare", "Compare options", "比較方案", "Compare the technical options for: ", "請比較技術方案："),
    ("teacher", "lesson", "Lesson plan", "教案", "Write a short lesson plan on: ", "請寫一份簡短教案："),
    ("teacher", "forstudents", "Explain for students", "給學生的解釋", "Explain this for high-school students: ", "請用高中生聽得懂的方式解釋："),
    ("teacher", "quiz", "Quiz questions", "測驗題", "Write 5 quiz questions with answers on: ", "請出 5 題測驗題並附答案："),
    ("teacher", "discussion", "Discussion questions", "討論題", "Suggest discussion questions about: ", "請提出討論題："),
];

fn builtin_lens(b: &Builtin, english: bool) -> QueryLens {
    let id = format!("{}.{}", b.0, b.1);
    if english {
        QueryLens { id, title: b.2.into(), prompt: b.4.into() }
    } else {
        QueryLens { id, title: b.3.into(), prompt: b.5.into() }
    }
}

/// The four built-in prompts of a profession in the given language.
pub fn builtins(profession: &str, english: bool) -> Vec<QueryLens> {
    let p = normalize_profession(profession);
    BUILTINS.iter().filter(|b| b.0 == p).map(|b| builtin_lens(b, english)).collect()
}

pub fn is_builtin_id(id: &str) -> bool {
    BUILTINS.iter().any(|b| id.len() == b.0.len() + 1 + b.1.len() && id == format!("{}.{}", b.0, b.1))
}

/// A built-in exactly as the app supplied it, in either language. An edited
/// built-in keeps its ID but is the user's prompt now.
pub fn is_unmodified_builtin(l: &QueryLens) -> bool {
    BUILTINS.iter().any(|b| builtin_lens(b, true) == *l || builtin_lens(b, false) == *l)
}

/// A prompt ID a template supplies: a built-in ID or `<profession>.user.<uuid>`.
pub fn is_template_prompt_id(id: &str) -> bool {
    is_builtin_id(id) || PROFESSIONS.iter().any(|p| id.starts_with(&format!("{}.user.", p)))
}

pub fn new_template_prompt_id(profession: &str) -> String {
    format!("{}.user.{}", normalize_profession(profession), uuid::Uuid::new_v4())
}

/// The prompt form's save rule: a prompt that does not end in whitespace gets
/// a trailing space, so it reads as a prefix ("Explain this: " + query).
pub fn with_trailing_space(prompt: &str) -> String {
    let mut s = prompt.to_string();
    if !s.is_empty() && !s.ends_with(char::is_whitespace) {
        s.push(' ');
    }
    s
}

/// Final text: chosen prompt + destination prompt prefix + query (plain concatenation).
pub fn compose(lens_prompt: &str, clip_prompt: &str, text: &str) -> String {
    format!("{}{}{}", lens_prompt, clip_prompt, text)
}

/// Readies a list coming from the settings window: items without an ID get one
/// from `new_id`; items that are new or changed (compared with `before` by ID)
/// must have a non-empty name and prompt and get the trailing-space rule.
/// Unchanged items are kept as is (a zh-Hant built-in has no trailing space and
/// must stay equal to the built-in).
pub fn prepare_list(
    before: &[QueryLens],
    list: Vec<QueryLens>,
    mut new_id: impl FnMut() -> String,
) -> Result<Vec<QueryLens>, String> {
    let before: HashMap<&str, &QueryLens> = before.iter().map(|l| (l.id.as_str(), l)).collect();
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(list.len());
    for mut l in list {
        if l.id.trim().is_empty() {
            l.id = new_id();
        }
        if !seen.insert(l.id.clone()) {
            continue; // one entry per ID
        }
        if before.get(l.id.as_str()) != Some(&&l) {
            if l.title.trim().is_empty() || l.prompt.trim().is_empty() {
                return Err("A prompt needs a name and a prompt".into());
            }
            l.title = l.title.trim().to_string();
            l.prompt = with_trailing_space(&l.prompt);
        }
        out.push(l);
    }
    Ok(out)
}

fn by_id(list: &[QueryLens]) -> HashMap<String, QueryLens> {
    let mut m = HashMap::new();
    for l in list {
        m.entry(l.id.clone()).or_insert_with(|| l.clone());
    }
    m
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PromptData {
    /// Selected template (profession raw value).
    pub profession: String,
    /// Edited templates; a profession without an entry follows its built-ins.
    pub templates: BTreeMap<String, Vec<QueryLens>>,
    /// Destination ID → its stored prompt list; no entry = follows the template.
    pub destinations: BTreeMap<String, Vec<QueryLens>>,
    /// Destination ID → IDs of template prompts edited inside that destination.
    pub edited: BTreeMap<String, BTreeSet<String>>,
}

impl Default for PromptData {
    fn default() -> Self {
        Self {
            profession: DEFAULT_PROFESSION.into(),
            templates: BTreeMap::new(),
            destinations: BTreeMap::new(),
            edited: BTreeMap::new(),
        }
    }
}

impl PromptData {
    pub fn profession(&self) -> &'static str {
        normalize_profession(&self.profession)
    }

    /// A template's prompts: the edited list or the built-in four.
    pub fn template_prompts(&self, profession: &str, english: bool) -> Vec<QueryLens> {
        let p = normalize_profession(profession);
        self.templates.get(p).cloned().unwrap_or_else(|| builtins(p, english))
    }

    pub fn is_template_edited(&self, profession: &str) -> bool {
        self.templates.contains_key(normalize_profession(profession))
    }

    /// The prompts shown for a destination, in order.
    pub fn prompts_for(&self, destination_id: &str, english: bool) -> Vec<QueryLens> {
        self.destinations
            .get(destination_id)
            .cloned()
            .unwrap_or_else(|| self.template_prompts(self.profession(), english))
    }

    fn is_edited_here(&self, prompt_id: &str, destination_id: &str) -> bool {
        self.edited.get(destination_id).is_some_and(|s| s.contains(prompt_id))
    }

    /// Stores a destination's list. A template prompt changed here becomes the
    /// destination's own version (marked by ID); changed back to the template's
    /// text it follows the template again; unchanged prompts keep their mark.
    pub fn set_prompts(&mut self, destination_id: &str, prompts: Vec<QueryLens>, english: bool) {
        let before = by_id(&self.prompts_for(destination_id, english));
        let template = by_id(&self.template_prompts(self.profession(), english));
        let marked = self.edited.get(destination_id).cloned().unwrap_or_default();
        let mut marks = BTreeSet::new();
        for p in prompts.iter().filter(|p| is_template_prompt_id(&p.id)) {
            if before.get(&p.id) == Some(p) {
                if marked.contains(&p.id) {
                    marks.insert(p.id.clone());
                }
            } else if template.get(&p.id) != Some(p) && !is_unmodified_builtin(p) {
                marks.insert(p.id.clone());
            }
        }
        if marks.is_empty() {
            self.edited.remove(destination_id);
        } else {
            self.edited.insert(destination_id.into(), marks);
        }
        self.destinations.insert(destination_id.into(), prompts);
    }

    /// Drops marks whose prompt is no longer in its destination's stored list.
    fn prune_edited(&mut self) {
        let mut map = BTreeMap::new();
        for (id, marks) in &self.edited {
            let present: HashSet<&str> =
                self.destinations.get(id).map(|l| l.iter().map(|x| x.id.as_str()).collect()).unwrap_or_default();
            let kept: BTreeSet<String> = marks.iter().filter(|m| present.contains(m.as_str())).cloned().collect();
            if !kept.is_empty() {
                map.insert(id.clone(), kept);
            }
        }
        self.edited = map;
    }

    /// A removed destination's prompts go with it.
    pub fn remove_destination(&mut self, destination_id: &str) {
        self.destinations.remove(destination_id);
        self.edited.remove(destination_id);
    }

    /// Chooses the template. In every stored list, prompts that follow the old
    /// template are replaced by the new template; the user's own (added or
    /// edited) stay after it. A list left with only template prompts follows
    /// the new template again (its entry is dropped).
    pub fn set_profession(&mut self, profession: &str, english: bool) {
        let profession = normalize_profession(profession);
        if profession == self.profession() {
            return;
        }
        let old_template = self.template_prompts(self.profession(), english);
        let new_template = self.template_prompts(profession, english);
        let mut map = BTreeMap::new();
        for (id, list) in &self.destinations {
            let mine: Vec<QueryLens> = list
                .iter()
                .filter(|l| {
                    let follows = !self.is_edited_here(&l.id, id)
                        && (is_unmodified_builtin(l) || old_template.contains(l));
                    !follows
                })
                .cloned()
                .collect();
            if !mine.is_empty() {
                map.insert(id.clone(), Self::presets(&new_template, &mine));
            }
        }
        self.profession = profession.into();
        self.destinations = map;
        self.prune_edited();
    }

    /// Writes a template (None: follow the built-ins again) and, for the
    /// selected template, carries the change into the destinations.
    pub fn set_template(&mut self, profession: &str, prompts: Option<Vec<QueryLens>>, english: bool) {
        let p = normalize_profession(profession);
        let old = self.template_prompts(p, english);
        match prompts {
            Some(list) => {
                self.templates.insert(p.into(), list);
            }
            None => {
                self.templates.remove(p);
            }
        }
        if p == self.profession() {
            let new = self.template_prompts(p, english);
            self.propagate(&old, &new);
        }
    }

    /// "Restore built-in prompts": the template follows its built-in four again.
    pub fn restore_template(&mut self, profession: &str, english: bool) {
        if self.is_template_edited(profession) {
            self.set_template(profession, None, english);
        }
    }

    /// Carries a template edit into the stored lists. A stored prompt follows
    /// the template when its ID is a template prompt's and it equals the old or
    /// new version (or the unmodified built-in), and was not edited here; it
    /// becomes the new version, or goes when deleted from the template. Edited
    /// template prompts keep their template place; added ones stay after the
    /// template. New template prompts reach every list that still shows some of
    /// the template (all of them when the template was empty).
    fn propagate(&mut self, old: &[QueryLens], new: &[QueryLens]) {
        let old_by_id = by_id(old);
        let new_by_id = by_id(new);
        let template_ids: HashSet<&String> = old_by_id.keys().chain(new_by_id.keys()).collect();
        let mut map = BTreeMap::new();
        for (id, stored) in &self.destinations {
            let follows = |l: &QueryLens| {
                template_ids.contains(&l.id)
                    && !self.is_edited_here(&l.id, id)
                    && (old_by_id.get(&l.id) == Some(l) || new_by_id.get(&l.id) == Some(l) || is_unmodified_builtin(l))
            };
            let mut seen = HashSet::new();
            let list: Vec<QueryLens> = stored.iter().filter(|l| seen.insert(l.id.clone())).cloned().collect();
            let mine = by_id(&list);
            let shows_template = old.is_empty() || list.iter().any(|l| old_by_id.contains_key(&l.id));
            let mut head = Vec::new();
            let mut seen_new = HashSet::new();
            for t in new.iter().filter(|t| seen_new.insert(t.id.clone())) {
                if let Some(m) = mine.get(&t.id) {
                    head.push(if follows(m) { t.clone() } else { m.clone() });
                } else if !old_by_id.contains_key(&t.id) && shows_template {
                    head.push(t.clone());
                }
            }
            let tail = list.iter().filter(|l| !new_by_id.contains_key(&l.id) && !follows(l)).cloned();
            head.extend(tail);
            map.insert(id.clone(), head);
        }
        self.destinations = map;
        self.prune_edited();
    }

    /// The template's prompts plus the user's, one entry per ID: a user prompt
    /// whose ID is in `presets` takes that preset's place; the rest follow.
    pub fn presets(presets: &[QueryLens], mine: &[QueryLens]) -> Vec<QueryLens> {
        let mut seen = HashSet::new();
        let mine: Vec<&QueryLens> = mine.iter().filter(|l| seen.insert(l.id.clone())).collect();
        let mine_by_id: HashMap<&str, &QueryLens> = mine.iter().map(|l| (l.id.as_str(), *l)).collect();
        let preset_ids: HashSet<&str> = presets.iter().map(|l| l.id.as_str()).collect();
        presets
            .iter()
            .map(|p| mine_by_id.get(p.id.as_str()).map(|m| (*m).clone()).unwrap_or_else(|| p.clone()))
            .chain(mine.iter().filter(|l| !preset_ids.contains(l.id.as_str())).map(|l| (*l).clone()))
            .collect()
    }
}

/// Rust-owned store at `<app data>/prompts.json`. A missing or unreadable file
/// means the defaults: template "general", built-ins everywhere.
pub struct PromptStore {
    data: Mutex<PromptData>,
    path: PathBuf,
}

impl PromptStore {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let path = app_data_dir.join("prompts.json");
        let mut data: PromptData = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        data.profession = normalize_profession(&data.profession).into();
        Self { data: Mutex::new(data), path }
    }

    pub fn get(&self) -> PromptData {
        self.data.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Mutates the data and writes the file (atomically) when it changed.
    pub fn update<R>(&self, f: impl FnOnce(&mut PromptData) -> R) -> R {
        let mut guard = self.data.lock().unwrap_or_else(|e| e.into_inner());
        let before = guard.clone();
        let r = f(&mut guard);
        if *guard != before {
            if let Ok(json) = serde_json::to_string_pretty(&*guard) {
                if let Some(parent) = self.path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let tmp = self.path.with_extension("json.tmp");
                if std::fs::write(&tmp, json).is_ok() {
                    let _ = std::fs::rename(&tmp, &self.path);
                }
            }
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EN: bool = true;
    const ZH: bool = false;

    fn ids(l: &[QueryLens]) -> Vec<&str> {
        l.iter().map(|x| x.id.as_str()).collect()
    }

    #[test]
    fn seven_templates_of_four_builtins() {
        for p in PROFESSIONS {
            let b = builtins(p, EN);
            assert_eq!(b.len(), 4, "{p}");
            assert!(b.iter().all(|l| l.id.starts_with(&format!("{p}.")) && is_builtin_id(&l.id)));
            assert!(b.iter().all(|l| l.prompt.ends_with(' ')));
            assert_eq!(ids(&builtins(p, ZH)), ids(&b));
        }
        assert_eq!(builtins("bogus", EN), builtins("general", EN));
        assert!(is_template_prompt_id("teacher.user.abc"));
        assert!(!is_template_prompt_id("5b8c-uuid"));
    }

    #[test]
    fn missing_data_seeds_destination_from_general_template() {
        let d = PromptData::default();
        assert_eq!(d.profession(), "general");
        assert_eq!(d.prompts_for("dest", EN), builtins("general", EN));
        // Built-in text follows the interface language until edited.
        assert_eq!(d.prompts_for("dest", ZH), builtins("general", ZH));
        let parsed: PromptData = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed, PromptData::default());
    }

    #[test]
    fn seeding_from_selected_template_keeps_own_prompts_after_it() {
        let mut d = PromptData::default();
        let mut list = d.prompts_for("a", EN);
        list.push(QueryLens::new("mine", "Mine", "Mine: "));
        d.set_prompts("a", list, EN);
        d.set_profession("physician", EN);
        let a = d.prompts_for("a", EN);
        assert_eq!(ids(&a), vec!["physician.explain", "physician.ddx", "physician.dose", "physician.guideline", "mine"]);
        // A destination without its own prompts just follows the template.
        assert!(!d.destinations.contains_key("b"));
        assert_eq!(d.prompts_for("b", EN), builtins("physician", EN));
        // Only template prompts left → follows the template (entry dropped).
        d.set_prompts("c", d.prompts_for("c", EN), EN);
        d.set_profession("lawyer", EN);
        assert!(!d.destinations.contains_key("c"));
    }

    #[test]
    fn template_edit_propagates_except_where_destination_edited() {
        let mut d = PromptData::default();
        // Destination A edits "general.explain"; B keeps it, adds its own prompt.
        let mut a = d.prompts_for("a", EN);
        a[0].prompt = "My own explain: ".into();
        d.set_prompts("a", a, EN);
        assert!(d.edited["a"].contains("general.explain"));
        let mut b = d.prompts_for("b", EN);
        b.push(QueryLens::new("bown", "B own", "B: "));
        d.set_prompts("b", b, EN);

        // Edit the template's first two prompts and add one.
        let mut t = d.template_prompts("general", EN);
        t[0].prompt = "Template explain: ".into();
        t[1].title = "TL;DR".into();
        t.push(QueryLens::new("general.user.x", "New", "New: "));
        d.set_template("general", Some(t), EN);

        let a = d.prompts_for("a", EN);
        assert_eq!(a[0].prompt, "My own explain: ", "edited here: stays");
        assert_eq!(a[1].title, "TL;DR");
        assert_eq!(a.last().unwrap().id, "general.user.x");
        let b = d.prompts_for("b", EN);
        assert_eq!(b[0].prompt, "Template explain: ");
        assert_eq!(ids(&b), vec!["general.explain", "general.summarize", "general.proscons", "general.translate", "general.user.x", "bown"]);
        // Unstored destination follows directly.
        assert_eq!(d.prompts_for("z", EN)[0].prompt, "Template explain: ");

        // Changing it back to the template's text follows the template again.
        let mut a = d.prompts_for("a", EN);
        a[0].prompt = "Template explain: ".into();
        d.set_prompts("a", a, EN);
        assert!(!d.edited.get("a").is_some_and(|s| s.contains("general.explain")));
    }

    #[test]
    fn editing_a_non_selected_template_does_not_touch_destinations() {
        let mut d = PromptData::default();
        d.set_prompts("a", d.prompts_for("a", EN), EN);
        let before = d.destinations.clone();
        d.set_template("teacher", Some(vec![]), EN);
        assert_eq!(d.destinations, before);
        assert!(d.template_prompts("teacher", EN).is_empty(), "a template may be empty");
    }

    #[test]
    fn deleting_from_template_removes_only_from_unedited_destinations() {
        let mut d = PromptData::default();
        let mut a = d.prompts_for("a", EN);
        a[2].title = "Pros/cons (mine)".into();
        d.set_prompts("a", a, EN);
        d.set_prompts("b", d.prompts_for("b", EN), EN);

        let mut t = d.template_prompts("general", EN);
        t.remove(2); // general.proscons
        d.set_template("general", Some(t), EN);

        assert!(ids(&d.prompts_for("b", EN)).iter().all(|i| *i != "general.proscons"));
        let a = d.prompts_for("a", EN);
        let kept = a.iter().find(|l| l.id == "general.proscons").expect("edited prompt stays");
        assert_eq!(kept.title, "Pros/cons (mine)");
        assert_eq!(a.last().unwrap().id, "general.proscons", "kept after the template");
    }

    #[test]
    fn restore_brings_back_the_four_builtins() {
        let mut d = PromptData::default();
        d.set_prompts("a", d.prompts_for("a", EN), EN);
        d.set_template("general", Some(vec![QueryLens::new("general.user.1", "Only", "Only: ")]), EN);
        assert_eq!(ids(&d.prompts_for("a", EN)), vec!["general.user.1"]);
        d.restore_template("general", EN);
        assert!(!d.is_template_edited("general"));
        assert_eq!(d.template_prompts("general", EN), builtins("general", EN));
        assert_eq!(d.prompts_for("a", EN), builtins("general", EN));
        // Restore on an unedited template is a no-op.
        let snapshot = d.clone();
        d.restore_template("general", EN);
        assert_eq!(d, snapshot);
    }

    #[test]
    fn restore_after_emptying_reseeds_destinations() {
        let mut d = PromptData::default();
        d.set_prompts("a", vec![QueryLens::new("own", "Own", "Own: ")], EN);
        d.set_template("general", Some(vec![]), EN);
        d.restore_template("general", EN);
        // The template was empty, so its built-ins reach every list.
        assert_eq!(ids(&d.prompts_for("a", EN)), vec!["general.explain", "general.summarize", "general.proscons", "general.translate", "own"]);
    }

    #[test]
    fn unmodified_builtin_in_other_language_still_follows() {
        let mut d = PromptData::default();
        d.set_prompts("a", d.prompts_for("a", ZH), ZH); // stored in zh-Hant
        let mut t = d.template_prompts("general", EN);
        t[0].prompt = "New: ".into();
        d.set_template("general", Some(t), EN);
        assert_eq!(d.prompts_for("a", EN)[0].prompt, "New: ");
    }

    #[test]
    fn removing_destination_drops_its_list_and_marks() {
        let mut d = PromptData::default();
        let mut a = d.prompts_for("a", EN);
        a[0].title = "x".into();
        d.set_prompts("a", a, EN);
        d.remove_destination("a");
        assert!(d.destinations.is_empty() && d.edited.is_empty());
    }

    #[test]
    fn final_text_is_prompt_then_prefix_then_query() {
        assert_eq!(compose("Explain this simply: ", "請用繁體中文回答：", "aspirin"), "Explain this simply: 請用繁體中文回答：aspirin");
        assert_eq!(compose("", "", "aspirin"), "aspirin");
    }

    #[test]
    fn trailing_space_rule() {
        assert_eq!(with_trailing_space("Explain:"), "Explain: ");
        assert_eq!(with_trailing_space("Explain: "), "Explain: ");
        assert_eq!(with_trailing_space("Line\n"), "Line\n");
        assert_eq!(with_trailing_space(""), "");
        assert_eq!(with_trailing_space("請解釋："), "請解釋： ");
    }

    #[test]
    fn prepare_list_assigns_ids_validates_and_spaces_only_changed_items() {
        let before = builtins("general", ZH);
        let mut list = before.clone();
        list.push(QueryLens::new("", "New", "Do this:"));
        let mut n = 0;
        let out = prepare_list(&before, list, || { n += 1; format!("id{n}") }).unwrap();
        assert_eq!(out[0], before[0], "unchanged zh built-in keeps its exact text");
        assert_eq!(out[4], QueryLens::new("id1", "New", "Do this: "));
        let bad = vec![QueryLens::new("", "  ", "x")];
        assert!(prepare_list(&[], bad, || "q".into()).is_err());
        let bad = vec![QueryLens::new("", "Name", " \n ")];
        assert!(prepare_list(&[], bad, || "q".into()).is_err());
    }

    #[test]
    fn store_persists_and_tolerates_missing_file() {
        let dir = std::env::temp_dir().join(format!("pb-prompts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = PromptStore::new(dir.clone());
        assert_eq!(store.get(), PromptData::default());
        store.update(|d| d.set_profession("engineer", EN));
        let again = PromptStore::new(dir.clone());
        assert_eq!(again.get().profession, "engineer");
        let _ = std::fs::remove_dir_all(dir);
    }
}
