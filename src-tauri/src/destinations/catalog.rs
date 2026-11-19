//! Built-in site catalogue for "Add a site" (ported from the iOS app's
//! `DestinationCatalog.swift`, add-site-sheet branch, 2026-11-19).
//!
//! Two kinds of entries:
//! - `url`: the query goes into a search-address template (`{q}`), so the
//!   results page opens directly; nothing is typed into the page.
//! - `inject`: the site's home page loads and providers.js fills its input box
//!   and sends (a rule with the same id must exist in providers.js).
//!
//! Built-in entries (same id as `defaults.rs`) put that default back.

use super::{defaults, Destination};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Group {
    General,
    Ai,
    Academic,
    Library,
    Reference,
    Daily,
}

impl Group {
    pub const ALL: [Group; 6] = [Group::General, Group::Ai, Group::Academic, Group::Library, Group::Reference, Group::Daily];
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub id: &'static str,
    pub name: &'static str,
    pub group: Group,
    pub home: String,
    /// Search address with `{q}`; `None` for inject entries.
    pub template: Option<&'static str>,
    /// Needs an account, a subscription or institutional access.
    pub login: bool,
    /// Address or input rule not confirmed on a real device yet.
    pub unverified: bool,
    /// Same id as a default destination.
    pub built_in: bool,
}

fn host_of(url: &str) -> Option<String> {
    url::Url::parse(&url.replace("{q}", "x")).ok()?.host_str().map(str::to_string)
}

fn url(id: &'static str, name: &'static str, group: Group, template: &'static str) -> Entry {
    let home = host_of(template).map(|h| format!("https://{}", h)).unwrap_or_else(|| template.to_string());
    Entry { id, name, group, home, template: Some(template), login: false, unverified: false, built_in: false }
}

fn inject(id: &'static str, name: &'static str, group: Group, home: &'static str) -> Entry {
    Entry { id, name, group, home: home.to_string(), template: None, login: false, unverified: false, built_in: false }
}

fn built_in(id: &'static str, group: Group) -> Entry {
    let d = defaults::default_destinations().into_iter().find(|d| d.id == id).expect("default destination");
    let name: &'static str = match id {
        "google" => "Google",
        "chatgpt" => "ChatGPT",
        "claude" => "Claude",
        "gemini" => "Gemini",
        "perplexity" => "Perplexity",
        "openevidence" => "OpenEvidence",
        _ => "?",
    };
    Entry { id, name, group, home: d.url, template: None, login: false, unverified: false, built_in: true }
}

trait EntryExt {
    fn login(self) -> Self;
    fn unverified(self) -> Self;
    fn home(self, home: &str) -> Self;
}

impl EntryExt for Entry {
    fn login(mut self) -> Self {
        self.login = true;
        self
    }
    fn unverified(mut self) -> Self {
        self.unverified = true;
        self
    }
    fn home(mut self, home: &str) -> Self {
        self.home = home.to_string();
        self
    }
}

pub fn entries() -> Vec<Entry> {
    use Group::*;
    vec![
        // General search
        built_in("google", General),
        url("bing", "Bing", General, "https://www.bing.com/search?q={q}"),
        url("duckduckgo", "DuckDuckGo", General, "https://duckduckgo.com/?q={q}"),
        url("brave", "Brave Search", General, "https://search.brave.com/search?q={q}"),
        url("startpage", "Startpage", General, "https://www.startpage.com/do/search?q={q}"),
        url("kagi", "Kagi", General, "https://kagi.com/search?q={q}").login(),
        url("ecosia", "Ecosia", General, "https://www.ecosia.org/search?q={q}"),
        url("yahoo-japan", "Yahoo! JAPAN", General, "https://search.yahoo.co.jp/search?p={q}"),
        url("naver", "Naver", General, "https://search.naver.com/search.naver?query={q}"),
        url("baidu", "Baidu", General, "https://www.baidu.com/s?wd={q}"),
        // AI chat
        built_in("chatgpt", Ai).login(),
        built_in("claude", Ai).login(),
        built_in("gemini", Ai).login(),
        built_in("perplexity", Ai),
        url("you", "You.com", Ai, "https://you.com/search?q={q}").login(),
        inject("deepseek", "DeepSeek", Ai, "https://chat.deepseek.com").login(),
        inject("poe", "Poe", Ai, "https://poe.com").login(),
        inject("kimi", "Kimi", Ai, "https://www.kimi.com").login(),
        inject("qwen", "Qwen", Ai, "https://chat.qwen.ai").login(),
        inject("mistral", "Mistral Le Chat", Ai, "https://chat.mistral.ai/chat").login().unverified(),
        inject("grok", "Grok", Ai, "https://grok.com").login().unverified(),
        inject("huggingchat", "HuggingChat", Ai, "https://huggingface.co/chat/").login(),
        inject("copilot", "Microsoft Copilot", Ai, "https://m365.cloud.microsoft/chat").login(),
        inject("uptodate-expert-ai", "UpToDate Expert AI", Ai, "https://ai.uptodate.com/").login(),
        // Academic & medical
        url("pubmed", "PubMed", Academic, "https://pubmed.ncbi.nlm.nih.gov/?term={q}"),
        url("pmc", "PMC", Academic, "https://pmc.ncbi.nlm.nih.gov/search/?term={q}"),
        url("europepmc", "Europe PMC", Academic, "https://europepmc.org/search?query={q}"),
        url("google-scholar", "Google Scholar", Academic, "https://scholar.google.com/scholar?q={q}"),
        url("semantic-scholar", "Semantic Scholar", Academic, "https://www.semanticscholar.org/search?q={q}"),
        built_in("openevidence", Academic).login(),
        url("cochrane", "Cochrane Library", Academic, "https://www.cochranelibrary.com/search?searchBy=1&searchText={q}"),
        url("clinicaltrials", "ClinicalTrials.gov", Academic, "https://clinicaltrials.gov/search?term={q}"),
        url("uptodate", "UpToDate", Academic, "https://www.uptodate.com/contents/search?search={q}").login(),
        inject("dynamed", "DynaMed", Academic, "https://www.dynamed.com").login(),
        url("medscape", "Medscape", Academic, "https://search.medscape.com/search/?q={q}").login(),
        url("drugs-com", "Drugs.com", Academic, "https://www.drugs.com/search.php?searchterm={q}"),
        inject("micromedex", "Micromedex", Academic, "https://www.micromedexsolutions.com").login().unverified(),
        url("msd-manuals", "MSD Manuals (Professional)", Academic, "https://www.msdmanuals.com/professional/searchresults?query={q}"),
        url("web-of-science", "Web of Science", Academic, "https://www.webofscience.com/wos/woscc/basic-search?query={q}").login(),
        url("scopus", "Scopus", Academic, "https://www.scopus.com/results/results.uri?s=TITLE-ABS-KEY({q})").login(),
        inject("embase", "Embase", Academic, "https://www.embase.com").login(),
        url("arxiv", "arXiv", Academic, "https://arxiv.org/search/?query={q}&searchtype=all"),
        url("medrxiv", "medRxiv", Academic, "https://www.medrxiv.org/search/{q}"),
        url("biorxiv", "bioRxiv", Academic, "https://www.biorxiv.org/search/{q}"),
        url("consensus", "Consensus", Academic, "https://consensus.app/results/?q={q}").login(),
        url("openalex", "OpenAlex", Academic, "https://openalex.org/works?search={q}"),
        url("connected-papers", "Connected Papers", Academic, "https://www.connectedpapers.com/search?q={q}"),
        url("crossref", "Crossref", Academic, "https://search.crossref.org/search/works?q={q}&from_ui=yes"),
        // Library
        url("worldcat", "WorldCat", Library, "https://search.worldcat.org/search?q={q}"),
        inject("airiti", "Airiti Library 華藝", Library, "https://www.airitilibrary.com"),
        // Reference & translation
        url("wikipedia-en", "Wikipedia (English)", Reference, "https://en.wikipedia.org/w/index.php?search={q}"),
        url("wikipedia-zh", "Wikipedia 中文", Reference, "https://zh.wikipedia.org/w/index.php?search={q}"),
        url("google-translate", "Google Translate", Reference, "https://translate.google.com/?sl=auto&tl=zh-TW&text={q}&op=translate"),
        url("deepl", "DeepL", Reference, "https://www.deepl.com/translator#en/zh-hant/{q}").home("https://www.deepl.com/translator"),
        url("cambridge", "Cambridge Dictionary", Reference, "https://dictionary.cambridge.org/search/direct/?datasetsearch=english&q={q}"),
        url("cambridge-en-zh-hant", "Cambridge 英漢（繁體）", Reference, "https://dictionary.cambridge.org/dictionary/english-chinese-traditional/{q}"),
        url("yahoo-tw-dictionary", "Yahoo 奇摩字典", Reference, "https://tw.dictionary.search.yahoo.com/search?p={q}"),
        url("merriam-webster", "Merriam-Webster", Reference, "https://www.merriam-webster.com/dictionary/{q}"),
        url("oxford-learners", "Oxford Learner's Dictionaries", Reference, "https://www.oxfordlearnersdictionaries.com/search/english/?q={q}"),
        url("moe-dict", "教育部國語辭典簡編本", Reference, "https://dict.concised.moe.edu.tw/search.jsp?md=1&word={q}"),
        url("wolfram-alpha", "Wolfram|Alpha", Reference, "https://www.wolframalpha.com/input?i={q}"),
        url("mdn", "MDN", Reference, "https://developer.mozilla.org/en-US/search?q={q}"),
        url("stack-overflow", "Stack Overflow", Reference, "https://stackoverflow.com/search?q={q}"),
        url("github", "GitHub", Reference, "https://github.com/search?q={q}&type=repositories"),
        // Everyday
        url("youtube", "YouTube", Daily, "https://www.youtube.com/results?search_query={q}"),
        url("google-maps", "Google Maps", Daily, "https://www.google.com/maps/search/?api=1&query={q}").home("https://www.google.com/maps"),
        url("amazon", "Amazon", Daily, "https://www.amazon.com/s?k={q}"),
        url("x", "X", Daily, "https://x.com/search?q={q}").login(),
        url("threads", "Threads", Daily, "https://www.threads.com/search?q={q}").login(),
    ]
}

pub fn entry(id: &str) -> Option<Entry> {
    entries().into_iter().find(|e| e.id == id)
}

impl Entry {
    /// Whether `list` already holds this entry.
    pub fn is_added(&self, list: &[Destination]) -> bool {
        list.iter().any(|d| d.catalog_id.as_deref() == Some(self.id) || (self.built_in && d.id == self.id))
    }

    /// The destination added when the user picks this entry (`order` and,
    /// for non-built-ins, `id` are filled in by the caller).
    pub fn make_destination(&self) -> Destination {
        if self.built_in {
            if let Some(d) = defaults::default_destinations().into_iter().find(|d| d.id == self.id) {
                return d;
            }
        }
        Destination {
            id: String::new(),
            name: self.name.to_string(),
            url: self.home.clone(),
            icon: String::new(),
            order: 0,
            clip_prompt: String::new(),
            url_template: self.template.map(str::to_string),
            catalog_id: Some(self.id.to_string()),
        }
    }
}

/// Fill a search-address template with `text` (query-string encoded; a
/// template with `{q}` in the path or fragment gets `%20` for spaces).
pub fn fill_template(template: &str, text: &str) -> String {
    let i = template.find("{q}").unwrap_or(template.len());
    let in_query = template[..i].contains('?') && !template[..i].contains('#');
    let encoded: String = if in_query {
        url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
    } else {
        url::form_urlencoded::byte_serialize(text.as_bytes()).collect::<String>().replace('+', "%20")
    };
    template.replace("{q}", &encoded)
}

/// A custom address with `{q}` is a search template: (home URL, template).
pub fn split_custom(url: &str) -> (String, Option<String>) {
    if url.contains("{q}") {
        let home = host_of(url).map(|h| format!("https://{}", h)).unwrap_or_else(|| url.to_string());
        (home, Some(url.to_string()))
    } else {
        (url.to_string(), None)
    }
}

/// Lowercased name / address match across every group.
pub fn matches(e: &Entry, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    e.name.to_lowercase().contains(&q)
        || e.home.to_lowercase().contains(&q)
        || e.template.map(|t| t.to_lowercase().contains(&q)).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_unique_and_templates_have_q() {
        let all = entries();
        let mut ids: Vec<_> = all.iter().map(|e| e.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), all.len());
        for e in &all {
            if let Some(t) = e.template {
                assert!(t.contains("{q}"), "{}", e.id);
            }
            assert!(e.home.starts_with("https://"), "{}", e.id);
        }
    }

    #[test]
    fn inject_entries_have_a_provider_rule() {
        let js = include_str!("../../assets/providers.js");
        for e in entries().iter().filter(|e| e.template.is_none() && !e.built_in) {
            assert!(js.contains(&format!("id: '{}'", e.id)), "missing providers.js rule for {}", e.id);
        }
    }

    #[test]
    fn every_group_has_entries() {
        let all = entries();
        for g in Group::ALL {
            assert!(all.iter().any(|e| e.group == g));
        }
    }

    #[test]
    fn fill_encodes_query_and_path() {
        assert_eq!(fill_template("https://x.com/search?q={q}", "a b&c"), "https://x.com/search?q=a+b%26c");
        assert_eq!(fill_template("https://www.medrxiv.org/search/{q}", "heart failure"), "https://www.medrxiv.org/search/heart%20failure");
        assert_eq!(fill_template("https://www.deepl.com/translator#en/zh-hant/{q}", "hi there"), "https://www.deepl.com/translator#en/zh-hant/hi%20there");
        assert_eq!(
            fill_template("https://pubmed.ncbi.nlm.nih.gov/?term={q}", "心衰竭"),
            "https://pubmed.ncbi.nlm.nih.gov/?term=%E5%BF%83%E8%A1%B0%E7%AB%AD"
        );
    }

    #[test]
    fn search_matches_name_and_host() {
        let e = entry("uptodate-expert-ai").unwrap();
        assert!(matches(&e, "expert"));
        assert!(matches(&e, "AI.UPTODATE"));
        assert!(!matches(&e, "pubmed"));
        assert!(matches(&entry("pubmed").unwrap(), "ncbi"));
    }

    #[test]
    fn built_in_restores_default_and_is_detected() {
        let e = entry("claude").unwrap();
        let d = e.make_destination();
        assert_eq!(d.id, "claude");
        assert!(e.is_added(&[d]));
        let p = entry("pubmed").unwrap();
        let mut pd = p.make_destination();
        pd.id = "x".into();
        assert_eq!(pd.url_template.as_deref(), Some("https://pubmed.ncbi.nlm.nih.gov/?term={q}"));
        assert!(p.is_added(&[pd]));
    }

    #[test]
    fn custom_template_split() {
        let (home, t) = split_custom("https://example.org/find?s={q}");
        assert_eq!(home, "https://example.org");
        assert_eq!(t.as_deref(), Some("https://example.org/find?s={q}"));
        assert_eq!(split_custom("https://example.org").1, None);
    }
}
