//! Local query records: what was being read, what was asked, where, and the
//! saved answer. SQLite with a versioned schema and full-text search (FTS5
//! when the system SQLite provides it, LIKE otherwise). Attachments live as
//! files; only their relative path is stored.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SCHEMA_VERSION: i64 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Record {
    pub id: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub source_app: Option<String>,
    pub source_title: Option<String>,
    pub source_url: Option<String>,
    pub selection_text: Option<String>,
    pub attachment_path: Option<String>,
    pub action_id: String,
    pub destination_id: String,
    pub destination_name: String,
    pub prompt: String,
    pub conversation_url: Option<String>,
    pub response_text: Option<String>,
    pub response_markdown: Option<String>,
    pub citation_links: Vec<String>,
    /// complete | partial | unknown | manual_selection
    pub capture_status: Option<String>,
    /// queued | injected | generating | completed | failed | cancelled
    pub query_status: String,
    pub tags: Vec<String>,
    pub note: Option<String>,
    pub favorite: bool,
}

/// Fields known when a query is sent.
#[derive(Clone, Debug, Default)]
pub struct NewQuery {
    pub source_app: Option<String>,
    pub selection_text: Option<String>,
    pub attachment_path: Option<String>,
    pub action_id: String,
    pub destination_id: String,
    pub destination_name: String,
    pub prompt: String,
}

/// An answer captured from a destination page.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Capture {
    pub text: String,
    pub markdown: String,
    #[serde(default)]
    pub links: Vec<String>,
    #[serde(default)]
    pub conversation_url: Option<String>,
    pub capture_status: String,
}

pub struct RecordStore {
    conn: Mutex<Connection>,
    fts: bool,
    pub attachments_dir: PathBuf,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn json_vec(s: Option<String>) -> Vec<String> {
    s.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

const COLUMNS: &str = "id, created_at, updated_at, source_app, source_title, source_url, selection_text, \
    attachment_path, action_id, destination_id, destination_name, prompt, conversation_url, response_text, \
    response_markdown, citation_links, capture_status, query_status, tags, note, favorite";

fn from_row(r: &Row) -> rusqlite::Result<Record> {
    Ok(Record {
        id: r.get(0)?,
        created_at: r.get(1)?,
        updated_at: r.get(2)?,
        source_app: r.get(3)?,
        source_title: r.get(4)?,
        source_url: r.get(5)?,
        selection_text: r.get(6)?,
        attachment_path: r.get(7)?,
        action_id: r.get(8)?,
        destination_id: r.get(9)?,
        destination_name: r.get(10)?,
        prompt: r.get(11)?,
        conversation_url: r.get(12)?,
        response_text: r.get(13)?,
        response_markdown: r.get(14)?,
        citation_links: json_vec(r.get(15)?),
        capture_status: r.get(16)?,
        query_status: r.get(17)?,
        tags: json_vec(r.get(18)?),
        note: r.get(19)?,
        favorite: r.get::<_, i64>(20)? != 0,
    })
}

impl RecordStore {
    /// Open (and migrate) `<dir>/records.sqlite`; attachments go in `<dir>/attachments`.
    pub fn open(dir: &Path) -> rusqlite::Result<Self> {
        let _ = std::fs::create_dir_all(dir);
        let conn = Connection::open(dir.join("records.sqlite"))?;
        Self::with_connection(conn, dir.join("attachments"))
    }

    pub fn with_connection(conn: Connection, attachments_dir: PathBuf) -> rusqlite::Result<Self> {
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version < 1 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS records (
                    id TEXT PRIMARY KEY,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    source_app TEXT, source_title TEXT, source_url TEXT,
                    selection_text TEXT, attachment_path TEXT,
                    action_id TEXT NOT NULL DEFAULT 'send',
                    destination_id TEXT NOT NULL,
                    destination_name TEXT NOT NULL DEFAULT '',
                    prompt TEXT NOT NULL DEFAULT '',
                    conversation_url TEXT,
                    response_text TEXT, response_markdown TEXT,
                    citation_links TEXT,
                    capture_status TEXT,
                    query_status TEXT NOT NULL DEFAULT 'queued',
                    tags TEXT, note TEXT,
                    favorite INTEGER NOT NULL DEFAULT 0
                );
                CREATE INDEX IF NOT EXISTS records_created ON records(created_at DESC);",
            )?;
        }
        if version < SCHEMA_VERSION {
            conn.execute_batch(&format!("PRAGMA user_version = {}", SCHEMA_VERSION))?;
        }
        // Full-text index is derived data: (re)create opportunistically.
        let fts = conn
            .execute_batch(
                "CREATE VIRTUAL TABLE IF NOT EXISTS records_fts USING fts5(
                    id UNINDEXED, selection_text, prompt, response_text, note, tags,
                    tokenize = 'unicode61'
                );",
            )
            .is_ok();
        Ok(Self { conn: Mutex::new(conn), fts, attachments_dir })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn reindex(&self, conn: &Connection, id: &str) {
        if !self.fts {
            return;
        }
        let _ = conn.execute("DELETE FROM records_fts WHERE id = ?1", params![id]);
        let _ = conn.execute(
            "INSERT INTO records_fts (id, selection_text, prompt, response_text, note, tags)
             SELECT id, COALESCE(selection_text,''), prompt, COALESCE(response_text,''), COALESCE(note,''), COALESCE(tags,'')
             FROM records WHERE id = ?1",
            params![id],
        );
    }

    pub fn create_query(&self, q: NewQuery) -> rusqlite::Result<Record> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        let conn = self.lock();
        conn.execute(
            "INSERT INTO records (id, created_at, updated_at, source_app, selection_text, attachment_path,
                action_id, destination_id, destination_name, prompt, query_status)
             VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'queued')",
            params![id, now, q.source_app, q.selection_text, q.attachment_path, q.action_id,
                q.destination_id, q.destination_name, q.prompt],
        )?;
        self.reindex(&conn, &id);
        drop(conn);
        self.get(&id).map(|r| r.expect("just inserted"))
    }

    pub fn set_query_status(&self, id: &str, status: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE records SET query_status = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, status, now_ms()],
        )?;
        Ok(())
    }

    /// Idempotent: saving the same query again updates the same record.
    pub fn save_capture(&self, id: &str, c: &Capture) -> rusqlite::Result<Option<Record>> {
        let conn = self.lock();
        let links = serde_json::to_string(&c.links).unwrap_or_else(|_| "[]".into());
        let status_update = if c.capture_status == "complete" { Some("completed") } else { None };
        let n = conn.execute(
            "UPDATE records SET response_text = ?2, response_markdown = ?3, citation_links = ?4,
                conversation_url = COALESCE(?5, conversation_url), capture_status = ?6,
                query_status = COALESCE(?7, query_status), updated_at = ?8
             WHERE id = ?1",
            params![id, c.text, c.markdown, links, c.conversation_url, c.capture_status, status_update, now_ms()],
        )?;
        if n == 0 {
            return Ok(None);
        }
        self.reindex(&conn, id);
        drop(conn);
        self.get(id)
    }

    pub fn set_conversation_url(&self, id: &str, url: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE records SET conversation_url = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, url, now_ms()],
        )?;
        Ok(())
    }

    pub fn update_user_fields(&self, id: &str, tags: &[String], note: Option<&str>, favorite: bool) -> rusqlite::Result<()> {
        let conn = self.lock();
        let tags = serde_json::to_string(tags).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "UPDATE records SET tags = ?2, note = ?3, favorite = ?4, updated_at = ?5 WHERE id = ?1",
            params![id, tags, note, favorite as i64, now_ms()],
        )?;
        self.reindex(&conn, id);
        Ok(())
    }

    pub fn get(&self, id: &str) -> rusqlite::Result<Option<Record>> {
        let conn = self.lock();
        conn.query_row(&format!("SELECT {} FROM records WHERE id = ?1", COLUMNS), params![id], from_row)
            .optional()
    }

    /// Most recent first. `query` searches original text, prompt, answer, notes and tags.
    pub fn list(&self, query: Option<&str>, favorites_only: bool, limit: usize) -> rusqlite::Result<Vec<Record>> {
        let conn = self.lock();
        let fav = if favorites_only { " AND r.favorite = 1" } else { "" };
        let q = query.map(str::trim).filter(|s| !s.is_empty());
        let mut out = Vec::new();
        match q {
            Some(q) if self.fts => {
                // Quote each term so user input can't inject FTS syntax; prefix-match the last.
                let terms: Vec<String> = q
                    .split_whitespace()
                    .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
                    .collect();
                let sql = format!(
                    "SELECT {} FROM records r WHERE r.id IN (SELECT id FROM records_fts WHERE records_fts MATCH ?1){} \
                     ORDER BY r.created_at DESC LIMIT ?2",
                    COLUMNS.split(", ").map(|c| format!("r.{}", c)).collect::<Vec<_>>().join(", "),
                    fav
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params![terms.join(" "), limit as i64], from_row)?;
                for r in rows {
                    out.push(r?);
                }
                // FTS tokenization doesn't split CJK text into words; fall back to substring.
                if out.is_empty() {
                    drop(stmt);
                    return Self::list_like(&conn, q, fav, limit);
                }
            }
            Some(q) => return Self::list_like(&conn, q, fav, limit),
            None => {
                let sql = format!(
                    "SELECT {} FROM records r WHERE 1=1{} ORDER BY r.created_at DESC LIMIT ?1",
                    COLUMNS, fav
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params![limit as i64], from_row)?;
                for r in rows {
                    out.push(r?);
                }
            }
        }
        Ok(out)
    }

    fn list_like(conn: &Connection, q: &str, fav: &str, limit: usize) -> rusqlite::Result<Vec<Record>> {
        let pattern = format!("%{}%", q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        let sql = format!(
            "SELECT {} FROM records r WHERE (COALESCE(selection_text,'') LIKE ?1 ESCAPE '\\' \
             OR prompt LIKE ?1 ESCAPE '\\' OR COALESCE(response_text,'') LIKE ?1 ESCAPE '\\' \
             OR COALESCE(note,'') LIKE ?1 ESCAPE '\\' OR COALESCE(tags,'') LIKE ?1 ESCAPE '\\'){} \
             ORDER BY r.created_at DESC LIMIT ?2",
            COLUMNS, fav
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![pattern, limit as i64], from_row)?;
        rows.collect()
    }

    /// Delete a record and its attachment file.
    pub fn delete(&self, id: &str) -> rusqlite::Result<bool> {
        let attachment = self.get(id)?.and_then(|r| r.attachment_path);
        let conn = self.lock();
        let n = conn.execute("DELETE FROM records WHERE id = ?1", params![id])?;
        if self.fts {
            let _ = conn.execute("DELETE FROM records_fts WHERE id = ?1", params![id]);
        }
        drop(conn);
        if let Some(rel) = attachment {
            // Only ever delete inside our attachments directory.
            let name = Path::new(&rel).file_name().map(|n| n.to_owned());
            if let Some(name) = name {
                let _ = std::fs::remove_file(self.attachments_dir.join(name));
            }
        }
        Ok(n > 0)
    }

    /// Absolute path for a stored relative attachment path.
    pub fn attachment_abs(&self, rel: &str) -> Option<PathBuf> {
        Path::new(rel).file_name().map(|n| self.attachments_dir.join(n))
    }
}

/// Markdown export of one record (original text, prompt, answer, sources).
pub fn to_markdown(r: &Record) -> String {
    let mut s = String::new();
    let title: String = r
        .selection_text
        .as_deref()
        .unwrap_or(&r.prompt)
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(80)
        .collect();
    s.push_str(&format!("# {}\n\n", if title.is_empty() { "Peekabrowser query" } else { &title }));
    s.push_str(&format!("- Destination: {}\n", r.destination_name));
    if let Some(app) = &r.source_app {
        s.push_str(&format!("- Source app: {}\n", app));
    }
    if let Some(u) = &r.source_url {
        s.push_str(&format!("- Source: {}\n", u));
    }
    if let Some(u) = &r.conversation_url {
        s.push_str(&format!("- Conversation: {}\n", u));
    }
    if let Some(c) = &r.capture_status {
        s.push_str(&format!("- Capture: {}\n", c));
    }
    if !r.tags.is_empty() {
        s.push_str(&format!("- Tags: {}\n", r.tags.join(", ")));
    }
    if let Some(sel) = r.selection_text.as_deref().filter(|t| !t.is_empty()) {
        s.push_str("\n## Original text\n\n");
        for line in sel.lines() {
            s.push_str(&format!("> {}\n", line));
        }
    }
    if !r.prompt.is_empty() && Some(r.prompt.as_str()) != r.selection_text.as_deref() {
        s.push_str(&format!("\n## Prompt\n\n{}\n", r.prompt));
    }
    if let Some(md) = r.response_markdown.as_deref().or(r.response_text.as_deref()).filter(|t| !t.is_empty()) {
        s.push_str(&format!("\n## Answer\n\n{}\n", md.trim_end()));
    }
    if !r.citation_links.is_empty() {
        s.push_str("\n## Links\n\n");
        for l in &r.citation_links {
            s.push_str(&format!("- <{}>\n", l));
        }
    }
    if let Some(n) = r.note.as_deref().filter(|t| !t.is_empty()) {
        s.push_str(&format!("\n## Note\n\n{}\n", n));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> RecordStore {
        let dir = std::env::temp_dir().join(format!("peeka-test-{}", uuid::Uuid::new_v4()));
        RecordStore::open(&dir).unwrap()
    }

    fn q(text: &str) -> NewQuery {
        NewQuery {
            selection_text: Some(text.into()),
            action_id: "send".into(),
            destination_id: "claude".into(),
            destination_name: "Claude".into(),
            prompt: format!("Explain: {}", text),
            ..Default::default()
        }
    }

    #[test]
    fn migration_is_idempotent_and_persists() {
        let dir = std::env::temp_dir().join(format!("peeka-test-{}", uuid::Uuid::new_v4()));
        let id = {
            let s = RecordStore::open(&dir).unwrap();
            s.create_query(q("mitochondria")).unwrap().id
        };
        let s = RecordStore::open(&dir).unwrap();
        assert!(s.get(&id).unwrap().is_some());
    }

    #[test]
    fn save_twice_updates_same_record() {
        let s = store();
        let r = s.create_query(q("entropy")).unwrap();
        let mut c = Capture { text: "part".into(), markdown: "part".into(), capture_status: "partial".into(), ..Default::default() };
        s.save_capture(&r.id, &c).unwrap();
        c.text = "full answer".into();
        c.markdown = "**full** answer".into();
        c.capture_status = "complete".into();
        c.links = vec!["https://example.org".into()];
        let saved = s.save_capture(&r.id, &c).unwrap().unwrap();
        assert_eq!(s.list(None, false, 50).unwrap().len(), 1);
        assert_eq!(saved.response_markdown.as_deref(), Some("**full** answer"));
        assert_eq!(saved.query_status, "completed");
        assert_eq!(saved.citation_links, vec!["https://example.org".to_string()]);
    }

    #[test]
    fn capture_goes_to_its_own_query() {
        let s = store();
        let a = s.create_query(q("alpha")).unwrap();
        let b = s.create_query(q("beta")).unwrap();
        let c = Capture { text: "about alpha".into(), markdown: "about alpha".into(), capture_status: "complete".into(), ..Default::default() };
        s.save_capture(&a.id, &c).unwrap();
        assert!(s.get(&b.id).unwrap().unwrap().response_text.is_none());
        assert_eq!(s.get(&a.id).unwrap().unwrap().response_text.as_deref(), Some("about alpha"));
    }

    #[test]
    fn search_finds_text_and_cjk_and_handles_quotes() {
        let s = store();
        let a = s.create_query(q("photosynthesis")).unwrap();
        s.create_query(q("粒線體 是什麼")).unwrap();
        assert_eq!(s.list(Some("photosyn"), false, 50).unwrap()[0].id, a.id);
        assert_eq!(s.list(Some("粒線"), false, 50).unwrap().len(), 1);
        assert!(s.list(Some("\"bad"), false, 50).is_ok());
        assert!(s.list(Some("100%_"), false, 50).unwrap().is_empty());
    }

    #[test]
    fn favorites_tags_and_delete_removes_attachment() {
        let s = store();
        std::fs::create_dir_all(&s.attachments_dir).unwrap();
        let file = s.attachments_dir.join("x.png");
        std::fs::write(&file, b"png").unwrap();
        let mut nq = q("image");
        nq.attachment_path = Some("attachments/x.png".into());
        let r = s.create_query(nq).unwrap();
        s.update_user_fields(&r.id, &["bio".into()], Some("remember"), true).unwrap();
        assert_eq!(s.list(None, true, 10).unwrap().len(), 1);
        assert_eq!(s.list(Some("remember"), false, 10).unwrap().len(), 1);
        assert!(s.delete(&r.id).unwrap());
        assert!(!file.exists());
        assert!(s.get(&r.id).unwrap().is_none());
    }

    #[test]
    fn markdown_contains_sections() {
        let s = store();
        let r = s.create_query(q("vector")).unwrap();
        let c = Capture { text: "t".into(), markdown: "```rs\nfn x(){}\n```".into(), capture_status: "complete".into(), links: vec!["https://a".into()], ..Default::default() };
        let r = s.save_capture(&r.id, &c).unwrap().unwrap();
        let md = to_markdown(&r);
        assert!(md.contains("## Original text"));
        assert!(md.contains("```rs"));
        assert!(md.contains("<https://a>"));
    }
}
