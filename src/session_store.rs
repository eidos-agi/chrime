//! Persist and shim (restore) API sessions.
//!
//! A saved session is one JSON file under the sessions dir (default `logs/sessions/`).
//! It holds the single HTML buffer + history + flags — no second page copy.
//! Loading "shims" that state into the *current* live session (new breadcrumb SESS;
//! the saved id is recorded as `shim_from` for full traceability).
//!
//! See `docs/BREADCRUMBS.md` segment `SAVE` / `SHIM`.

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::trace;

const FORMAT: &str = "chrime.session.v1";

/// Portable page + session blob (one HTML buffer).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSession {
    pub format: String,
    /// Unique save id (also the filename stem). Hierarchical: may include parent SESS.
    pub id: String,
    /// Plain English name for agents/humans.
    pub name: String,
    pub saved_at: String,
    pub saved_at_ms: u64,
    /// Breadcrumb of the session that was saved.
    pub source_sess: String,
    pub source_run: String,
    pub history: Vec<String>,
    pub ai_vis: bool,
    pub page: SavedPage,
    /// Optional free-form notes (never secrets).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPage {
    pub url: Option<String>,
    pub title: Option<String>,
    /// Single HTML buffer — the only page body we store.
    pub html: String,
}

fn sessions_dir() -> PathBuf {
    let dir = std::env::var("CHRIME_SESSIONS_DIR").unwrap_or_else(|_| "logs/sessions".into());
    let p = PathBuf::from(dir);
    let _ = fs::create_dir_all(&p);
    p
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn iso_now() -> String {
    let ms = now_ms();
    format!("{}.{:03}Z", ms / 1000, ms % 1000)
}

fn sanitize_id(raw: &str) -> String {
    let s: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        format!("sess_{}", now_ms())
    } else {
        s.chars().take(120).collect()
    }
}

fn path_for(id: &str) -> PathBuf {
    sessions_dir().join(format!("{}.json", sanitize_id(id)))
}

/// Build a unique save id under the current session breadcrumb.
pub fn new_save_id(source_sess: &str, name: &str) -> String {
    let ms = now_ms();
    let slug = sanitize_id(name);
    // Hierarchical: CHRIME….SESS….SAVE.<slug>_<ms>
    format!("{}.SAVE.{}_{}", source_sess, slug, ms)
}

pub fn save(
    source_sess: &str,
    source_run: &str,
    name: &str,
    history: &[String],
    ai_vis: bool,
    page: SavedPage,
    note: Option<String>,
) -> Result<SavedSession, String> {
    let id = new_save_id(source_sess, name);
    let file_stem = sanitize_id(&format!("{}_{}", name.trim().replace(' ', "_"), now_ms()));
    let saved = SavedSession {
        format: FORMAT.into(),
        id: id.clone(),
        name: if name.trim().is_empty() {
            file_stem.clone()
        } else {
            name.trim().into()
        },
        saved_at: iso_now(),
        saved_at_ms: now_ms(),
        source_sess: source_sess.into(),
        source_run: source_run.into(),
        history: history.to_vec(),
        ai_vis,
        page,
        note,
    };
    let path = sessions_dir().join(format!("{file_stem}.json"));
    // Also write alias path by hierarchical id hash-safe name
    let path_by_id = path_for(&file_stem);
    let body = serde_json::to_string_pretty(&saved).map_err(|e| e.to_string())?;
    fs::write(&path, &body).map_err(|e| format!("write {}: {e}", path.display()))?;
    if path_by_id != path {
        let _ = fs::write(&path_by_id, &body);
    }
    // Index entry for list without loading full HTML
    append_index(&saved, &path)?;

    let crumb = format!("{}.SAVE.{}", source_sess, file_stem);
    trace::emit(
        &crumb,
        Some(source_sess),
        "session_save",
        &format!(
            "Saved session `{name}` as {file_stem} ({html} HTML bytes, {hist} history entries).",
            name = saved.name,
            html = saved.page.html.len(),
            hist = saved.history.len()
        ),
        json!({
            "save_id": saved.id,
            "file": path.display().to_string(),
            "file_stem": file_stem,
            "html_bytes": saved.page.html.len(),
            "history_len": saved.history.len(),
            "url": saved.page.url,
        }),
    );

    Ok(saved)
}

fn append_index(saved: &SavedSession, path: &Path) -> Result<(), String> {
    let idx = sessions_dir().join("index.jsonl");
    let line = json!({
        "id": saved.id,
        "name": saved.name,
        "file": path.file_name().and_then(|s| s.to_str()),
        "saved_at": saved.saved_at,
        "url": saved.page.url,
        "html_bytes": saved.page.html.len(),
        "history_len": saved.history.len(),
        "source_sess": saved.source_sess,
    });
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(idx)
        .map_err(|e| e.to_string())?;
    writeln!(f, "{line}").map_err(|e| e.to_string())?;
    Ok(())
}

/// Resolve a user-provided id/name/path to a file.
fn resolve_file(id_or_name: &str) -> Result<PathBuf, String> {
    let raw = id_or_name.trim();
    if raw.is_empty() {
        return Err("id or name required".into());
    }
    // Direct path
    let as_path = Path::new(raw);
    if as_path.is_file() {
        return Ok(as_path.to_path_buf());
    }
    // stem.json in sessions dir
    let stem = sanitize_id(raw);
    let p = sessions_dir().join(format!("{stem}.json"));
    if p.is_file() {
        return Ok(p);
    }
    // Search index / directory for name match or id contains
    let dir = sessions_dir();
    if let Ok(rd) = fs::read_dir(&dir) {
        for ent in rd.flatten() {
            let path = ent.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if path.file_stem().and_then(|s| s.to_str()) == Some(stem.as_str()) {
                return Ok(path);
            }
            if let Ok(txt) = fs::read_to_string(&path) {
                if let Ok(s) = serde_json::from_str::<SavedSession>(&txt) {
                    if s.id == raw || s.name == raw || s.id.contains(raw) {
                        return Ok(path);
                    }
                }
            }
        }
    }
    Err(format!(
        "session not found: {raw} (looked in {})",
        dir.display()
    ))
}

pub fn load(id_or_name: &str) -> Result<SavedSession, String> {
    let path = resolve_file(id_or_name)?;
    let txt = fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let saved: SavedSession =
        serde_json::from_str(&txt).map_err(|e| format!("parse {}: {e}", path.display()))?;
    if saved.format != FORMAT {
        return Err(format!(
            "unsupported session format `{}` (want {FORMAT})",
            saved.format
        ));
    }
    Ok(saved)
}

pub fn list() -> Result<Vec<serde_json::Value>, String> {
    let dir = sessions_dir();
    let mut out = Vec::new();
    let rd = fs::read_dir(&dir).map_err(|e| format!("read dir {}: {e}", dir.display()))?;
    for ent in rd.flatten() {
        let path = ent.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if path.file_name().and_then(|n| n.to_str()) == Some("index.jsonl") {
            continue;
        }
        if let Ok(txt) = fs::read_to_string(&path) {
            if let Ok(s) = serde_json::from_str::<SavedSession>(&txt) {
                out.push(json!({
                    "id": s.id,
                    "name": s.name,
                    "file": path.file_name().and_then(|n| n.to_str()),
                    "saved_at": s.saved_at,
                    "url": s.page.url,
                    "title": s.page.title,
                    "html_bytes": s.page.html.len(),
                    "history_len": s.history.len(),
                    "source_sess": s.source_sess,
                    "ai_vis": s.ai_vis,
                }));
            }
        }
    }
    out.sort_by(|a, b| {
        let am = a.get("saved_at").and_then(|x| x.as_str()).unwrap_or("");
        let bm = b.get("saved_at").and_then(|x| x.as_str()).unwrap_or("");
        bm.cmp(am)
    });
    Ok(out)
}

pub fn delete(id_or_name: &str) -> Result<String, String> {
    let path = resolve_file(id_or_name)?;
    let name = path.display().to_string();
    fs::remove_file(&path).map_err(|e| format!("delete {name}: {e}"))?;
    Ok(name)
}

pub fn sessions_dir_display() -> String {
    sessions_dir().display().to_string()
}
