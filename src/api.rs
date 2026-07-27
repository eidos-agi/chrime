//! Unified JSON API — Chrime is fully operable with zero human clicks.
//!
//! Same ops on:
//! - `chrime --api` (JSONL on stdin/stdout)
//! - `chrime --listen 127.0.0.1:7420` (JSONL per TCP connection)
//! - GUI process (listens by default) so a window can be driven without touching it
//!
//! Secrets never appear in responses (`secret_output: "suppressed"`).

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

#[cfg(feature = "gui")]
use std::sync::mpsc::{self, Receiver, Sender};

use crate::hancock;
use crate::knox::{self, KnoxFillResult};
use crate::session_store;
use crate::trace::{self, TraceSession};
use crate::views::ViewKind;
use crate::Engine;

/// Optional live surface (GUI WebView). Headless API works without it;
/// ops that need injection return a clear error.
pub trait LiveSurface {
    fn eval_js(&mut self, js: &str) -> Result<(), String>;
    fn set_ai_vis(&mut self, on: bool);
    fn ai_vis(&self) -> bool;
    fn mark_count(&self) -> usize;
    /// Dual-pane geometry (mode, effective orientation, page pixel size). None if not GUI.
    fn layout_info(&self) -> Option<serde_json::Value> {
        None
    }
    /// Set pane layout. `mode`: auto|side|stack. `page_ratio`: 0.45–0.85 share for the live page.
    fn set_pane_layout(
        &mut self,
        mode: Option<&str>,
        page_ratio: Option<f64>,
    ) -> Result<serde_json::Value, String> {
        let _ = (mode, page_ratio);
        Err("layout needs the dual-pane GUI".into())
    }
    /// Cycle auto → side → stack → auto (chrome button / agent convenience).
    fn cycle_pane_layout(&mut self) -> Result<serde_json::Value, String> {
        Err("layout needs the dual-pane GUI".into())
    }
}

/// Session state for headless / TCP API (history for `back`/`forward` + breadcrumb SEQ).
pub struct Session {
    pub history: Vec<String>,
    /// URLs popped by `back` — restored by `forward`. Cleared on a fresh navigate/click.
    pub forward: Vec<String>,
    pub ai_vis: bool,
    /// Hierarchical breadcrumb session (`CHRIME.RUN.*.SESS.sNNNN`).
    pub trace: TraceSession,
    /// Set by `quit` with force — the loop breaks so `main` can return and engines can run
    /// their shutdown (the Servo cookie jar is only written on a clean engine drop).
    pub quit: bool,
}

impl Session {
    pub fn new() -> Self {
        Session {
            history: Vec::new(),
            forward: Vec::new(),
            ai_vis: false,
            trace: TraceSession::new(),
            quit: false,
        }
    }

    /// Record a user-driven navigation. Invalidates anything that was in the forward stack.
    fn push_url(&mut self, url: String) {
        if self.history.last().map(|u| u.as_str()) != Some(url.as_str()) {
            self.history.push(url);
            self.forward.clear();
        }
    }

    /// Pop current URL onto the forward stack; return the previous URL to load.
    fn go_back(&mut self) -> Option<String> {
        if self.history.len() <= 1 {
            return None;
        }
        if let Some(cur) = self.history.pop() {
            self.forward.push(cur);
        }
        self.history.last().cloned()
    }

    /// Pop the next forward URL onto history; return it to load.
    fn go_forward(&mut self) -> Option<String> {
        let next = self.forward.pop()?;
        self.history.push(next.clone());
        Some(next)
    }
}

fn err(code: &str, msg: &str) -> String {
    serde_json::json!({ "ok": false, "code": code, "error": msg }).to_string()
}

fn ok_json(v: serde_json::Value) -> String {
    v.to_string()
}

/// Dispatch one JSON line. `live` is Some when a WebView can inject/fill.
/// Every call is breadcrumbed under `session.trace` (see docs/BREADCRUMBS.md).
pub fn dispatch(
    eng: &mut dyn Engine,
    session: &mut Session,
    live: Option<&mut dyn LiveSurface>,
    line: &str,
) -> String {
    let body = dispatch_inner(eng, session, live, line);
    // Wrap with unique hierarchical id + append-only log (secrets redacted).
    trace::wrap_dispatch(&session.trace, line, body)
}

fn dispatch_inner(
    eng: &mut dyn Engine,
    session: &mut Session,
    mut live: Option<&mut dyn LiveSurface>,
    line: &str,
) -> String {
    let v: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return err("bad_json", &format!("bad json: {e}")),
    };
    let op = v.get("op").and_then(|o| o.as_str()).unwrap_or("");
    let has_live = live.is_some();

    match op {
        "ping" | "hello" => ok_json(serde_json::json!({
            "ok": true,
            "chrime": env!("CARGO_PKG_VERSION"),
            "api": "jsonl",
            "engine": eng.engine_name(),
            "live": has_live,
            "url": eng.current_url(),
            "ai_vis": live.as_ref().map(|l| l.ai_vis()).unwrap_or(session.ai_vis),
            "trace_root": session.trace.root,
            "hierarchy_doc": "docs/BREADCRUMBS.md",
        })),

        "help" | "ops" => ok_json(serde_json::json!({
            "ok": true,
            "ops": [
                "ping", "help", "status",
                "navigate", "back", "forward", "current", "snapshot", "view", "views", "read", "links",
                "find_text", "query", "click", "settle",
                "fill", "type", "press",
                "knox_find", "knox_fill", "knox_use",
                "session_save", "session_load", "session_list", "session_delete",
                "hancock_request", "hancock_wait", "hancock_pending",
                "set_ai_vis", "toggle_ai_vis", "ai_marks",
                "layout",
                "eval", "wait", "quit"
            ],
            "views": ViewKind::all().iter().map(|v| v.as_str()).collect::<Vec<_>>(),
            "note": "Fully agent-driven. Hancock: request human permission before risky surf actions. STILL_PENDING is NOT approval.",
            "sessions_dir": session_store::sessions_dir_display(),
            "secret_output": "suppressed",
            "hierarchy_doc": "docs/BREADCRUMBS.md",
            "hancock": "native — hancock_request / hancock_wait / hancock_pending"
        })),

        "status" => {
            let snap = eng.snapshot();
            let mut body = serde_json::json!({
                "ok": true,
                "engine": eng.engine_name(),
                "url": eng.current_url(),
                "title": snap.title,
                "node_count": snap.node_count,
                "history_len": session.history.len(),
                "forward_len": session.forward.len(),
                "live": has_live,
                "ai_vis": live.as_ref().map(|l| l.ai_vis()).unwrap_or(session.ai_vis),
                "mark_count": live.as_ref().map(|l| l.mark_count()).unwrap_or(0),
                "trace_root": session.trace.root,
                "run": trace::run_root(),
                "profile_dir": eng.profile_dir(),
                "hierarchy_doc": "docs/BREADCRUMBS.md",
            });
            if let Some(info) = live.as_ref().and_then(|l| l.layout_info()) {
                if let Some(obj) = body.as_object_mut() {
                    if let Some(map) = info.as_object() {
                        for (k, v) in map {
                            obj.insert(k.clone(), v.clone());
                        }
                    }
                }
            }
            ok_json(body)
        }

        // Dual-pane geometry: auto|side|stack + page_ratio (live page majority by default).
        "layout" => {
            let Some(surface) = live.as_mut() else {
                return err(
                    "no_live",
                    "layout needs the dual-pane GUI (default chrime window on :7420)",
                );
            };
            if v.get("cycle").and_then(|c| c.as_bool()).unwrap_or(false)
                || v.get("mode").and_then(|m| m.as_str()) == Some("cycle")
            {
                return match surface.cycle_pane_layout() {
                    Ok(info) => ok_json(info),
                    Err(e) => err("layout_failed", &e),
                };
            }
            let mode = v.get("mode").and_then(|m| m.as_str());
            let page_ratio = v
                .get("page_ratio")
                .or_else(|| v.get("ratio"))
                .and_then(|r| r.as_f64());
            if mode.is_none() && page_ratio.is_none() {
                // bare layout → report current
                return match surface.layout_info() {
                    Some(info) => ok_json(info),
                    None => err("layout_failed", "no layout info"),
                };
            }
            match surface.set_pane_layout(mode, page_ratio) {
                Ok(info) => ok_json(info),
                Err(e) => err("layout_failed", &e),
            }
        }

        "navigate" => {
            let url = v.get("url").and_then(|u| u.as_str()).unwrap_or("");
            let r = eng.navigate(url);
            if r.ok {
                if let Some(u) = r.url.clone() {
                    session.push_url(u);
                }
                // Keep live pane in sync when GUI is attached
                if let (Some(surface), Some(u)) = (live.as_mut(), r.url.as_ref()) {
                    let js = format!(
                        "window.location.assign({});",
                        serde_json::to_string(u).unwrap_or_else(|_| "\"\"".into())
                    );
                    let _ = surface.eval_js(&js);
                }
            }
            serde_json::to_string(&r).unwrap()
        }

        "back" => {
            match session.go_back() {
                Some(prev) => {
                    let r = eng.navigate(&prev);
                    if r.ok {
                        if let Some(surface) = live.as_mut() {
                            let js = format!(
                                "window.location.assign({});",
                                serde_json::to_string(&prev).unwrap_or_else(|_| "\"\"".into())
                            );
                            let _ = surface.eval_js(&js);
                        }
                    }
                    serde_json::to_string(&r).unwrap()
                }
                None => err("no_history", "nothing to go back to"),
            }
        }

        "forward" => {
            match session.go_forward() {
                Some(next) => {
                    let r = eng.navigate(&next);
                    if r.ok {
                        if let Some(surface) = live.as_mut() {
                            let js = format!(
                                "window.location.assign({});",
                                serde_json::to_string(&next).unwrap_or_else(|_| "\"\"".into())
                            );
                            let _ = surface.eval_js(&js);
                        }
                    }
                    serde_json::to_string(&r).unwrap()
                }
                None => err("no_forward", "nothing to go forward to"),
            }
        }

        "current" => ok_json(serde_json::json!({ "url": eng.current_url() })),

        "snapshot" => {
            // Optional view projection: {"op":"snapshot","view":"outline"}
            let kind = v
                .get("view")
                .or_else(|| v.get("kind"))
                .and_then(|k| k.as_str())
                .and_then(ViewKind::parse)
                .unwrap_or(ViewKind::Full);
            if kind == ViewKind::Full {
                serde_json::to_string(&eng.snapshot()).unwrap()
            } else {
                serde_json::to_string(&eng.view(kind)).unwrap()
            }
        }

        // Explicit view op — same page, different lens. Memory: projection only.
        "view" => {
            let kind = v
                .get("kind")
                .or_else(|| v.get("view"))
                .or_else(|| v.get("name"))
                .and_then(|k| k.as_str())
                .and_then(ViewKind::parse);
            let Some(kind) = kind else {
                return err(
                    "bad_args",
                    "view requires kind=full|outline|links|fields|clickables|text|compact|meta",
                );
            };
            serde_json::to_string(&eng.view(kind)).unwrap()
        }

        "views" => ok_json(serde_json::json!({
            "ok": true,
            "views": ViewKind::all().iter().map(|k| {
                serde_json::json!({ "kind": k.as_str(), "label": k.label() })
            }).collect::<Vec<_>>(),
            "memory": "one HTML buffer; views are ephemeral filters (no cached copies)",
            "html_bytes": eng.html_bytes(),
        })),

        // Deterministic settle as a first-class op: drive the engine to quiescence and hand
        // back the receipt (spins/ms/quiescent), never a sleep. telos: control-surfaces.
        "settle" => serde_json::to_string(&eng.settle()).unwrap(),

        "read" => ok_json(serde_json::json!({ "text": eng.read_text() })),

        "links" => serde_json::to_string(&eng.links()).unwrap(),

        "find_text" => {
            let q = v.get("text").and_then(|t| t.as_str()).unwrap_or("");
            serde_json::to_string(&eng.find_text(q)).unwrap()
        }

        // CSS selector → matching nodes. Semantic-tree matches keep stable node_ids (click works).
        "query" => {
            let sel = v
                .get("selector")
                .or_else(|| v.get("css"))
                .or_else(|| v.get("q"))
                .and_then(|s| s.as_str())
                .unwrap_or("");
            if sel.is_empty() {
                return err("bad_args", "query requires selector (CSS)");
            }
            match eng.query(sel) {
                Ok(nodes) => ok_json(serde_json::json!({
                    "ok": true,
                    "selector": sel,
                    "count": nodes.len(),
                    "nodes": nodes,
                })),
                Err(e) => err("bad_selector", &e),
            }
        }

        "click" => {
            let id = v.get("node_id").and_then(|n| n.as_u64()).unwrap_or(0) as u32;
            let r = eng.click(id);
            if r.ok {
                if let Some(u) = r.url.clone() {
                    session.push_url(u);
                }
                if let (Some(surface), Some(u)) = (live.as_mut(), r.url.as_ref()) {
                    let js = format!(
                        "window.location.assign({});",
                        serde_json::to_string(u).unwrap_or_else(|_| "\"\"".into())
                    );
                    let _ = surface.eval_js(&js);
                }
            }
            serde_json::to_string(&r).unwrap()
        }

        // Fill a live form field without human focus/click. Requires live WebView.
        // which: "login" | "password" | CSS selector string
        "fill" | "type" => {
            let Some(surface) = live.as_mut() else {
                return err(
                    "no_live",
                    "fill/type need a live surface — run the GUI (default listens on :7420); pure headless has no form surface yet",
                );
            };
            let text = v
                .get("text")
                .or_else(|| v.get("value"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            let which = v
                .get("which")
                .or_else(|| v.get("field"))
                .or_else(|| v.get("selector"))
                .and_then(|w| w.as_str())
                .unwrap_or("login");
            let js = fill_js(which, text);
            match surface.eval_js(&js) {
                Ok(()) => ok_json(serde_json::json!({
                    "ok": true,
                    "action": "fill",
                    "which": which,
                    "secret_output": "suppressed"
                })),
                Err(e) => err("eval_failed", &e),
            }
        }

        // Submit / press Enter on active form (live only).
        "press" => {
            let Some(surface) = live.as_mut() else {
                return err("no_live", "press needs a live surface");
            };
            let key = v.get("key").and_then(|k| k.as_str()).unwrap_or("Enter");
            let js = format!(
                r#"(function(){{
                  const key = {};
                  const el = document.activeElement || document.querySelector('input,textarea,button,form');
                  if (!el) return;
                  el.dispatchEvent(new KeyboardEvent('keydown', {{key:key, bubbles:true}}));
                  el.dispatchEvent(new KeyboardEvent('keyup', {{key:key, bubbles:true}}));
                  if (key === 'Enter') {{
                    const form = el.form || el.closest && el.closest('form');
                    if (form && form.requestSubmit) form.requestSubmit();
                    else if (form) form.submit();
                  }}
                }})()"#,
                serde_json::to_string(key).unwrap_or_else(|_| "\"Enter\"".into())
            );
            match surface.eval_js(&js) {
                Ok(()) => ok_json(serde_json::json!({ "ok": true, "key": key })),
                Err(e) => err("eval_failed", &e),
            }
        }

        "eval" => {
            let Some(surface) = live.as_mut() else {
                return err("no_live", "eval needs a live surface");
            };
            let js = v
                .get("js")
                .or_else(|| v.get("script"))
                .and_then(|s| s.as_str())
                .unwrap_or("");
            if js.is_empty() {
                return err("bad_args", "js required");
            }
            match surface.eval_js(js) {
                Ok(()) => ok_json(serde_json::json!({ "ok": true, "action": "eval" })),
                Err(e) => err("eval_failed", &e),
            }
        }

        "knox_find" => {
            let q = v
                .get("query")
                .and_then(|q| q.as_str())
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty())
                .or_else(|| eng.current_url().map(|u| knox::query_from_url(&u)))
                .unwrap_or_default();
            let limit = v.get("limit").and_then(|n| n.as_u64()).unwrap_or(10) as usize;
            serde_json::to_string(&knox::find(&q, limit)).unwrap()
        }

        // Unlock Knox + inject into live page. Zero human clicks (Touch ID may still prompt —
        // that is Knox security, not a Chrime UI). Without live surface, refuses rather than
        // dumping the secret.
        "knox_fill" => {
            let q = v.get("query").and_then(|q| q.as_str()).unwrap_or("");
            let id = v.get("id").and_then(|i| i.as_str());
            let fields = v
                .get("fields")
                .or_else(|| v.get("field"))
                .and_then(|f| f.as_str())
                .unwrap_or("both");
            let Some(surface) = live.as_mut() else {
                return serde_json::to_string(&KnoxFillResult {
                    ok: false,
                    record: None,
                    field: Some(fields.into()),
                    action: None,
                    error: Some(
                        "knox_fill needs a live surface (GUI). Use knox_use with via=type-frontmost as fallback, or run default GUI (listens on :7420)".into(),
                    ),
                    secret_output: "suppressed",
                })
                .unwrap();
            };
            let want_login = fields == "login" || fields == "both";
            let want_password = fields == "password" || fields == "both" || fields.is_empty();
            let mut record = String::new();
            let mut done = Vec::new();

            if want_login {
                match knox::unlock_field(q, "login", id) {
                    Ok((title, value)) => {
                        record = title;
                        let js = knox::fill_field_js("login", &value);
                        if let Err(e) = surface.eval_js(&js) {
                            return serde_json::to_string(&KnoxFillResult {
                                ok: false,
                                record: Some(record),
                                field: Some("login".into()),
                                action: None,
                                error: Some(e),
                                secret_output: "suppressed",
                            })
                            .unwrap();
                        }
                        done.push("login");
                    }
                    Err(e) => {
                        if !want_password {
                            return serde_json::to_string(&e).unwrap();
                        }
                    }
                }
            }
            if want_password {
                match knox::unlock_field(q, "password", id) {
                    Ok((title, value)) => {
                        if record.is_empty() {
                            record = title;
                        }
                        let js = knox::fill_field_js("password", &value);
                        if let Err(e) = surface.eval_js(&js) {
                            return serde_json::to_string(&KnoxFillResult {
                                ok: false,
                                record: Some(record),
                                field: Some("password".into()),
                                action: None,
                                error: Some(e),
                                secret_output: "suppressed",
                            })
                            .unwrap();
                        }
                        done.push("password");
                    }
                    Err(e) => return serde_json::to_string(&e).unwrap(),
                }
            }
            serde_json::to_string(&KnoxFillResult {
                ok: !done.is_empty(),
                record: if record.is_empty() {
                    None
                } else {
                    Some(record)
                },
                field: Some(done.join("+")),
                action: Some("browser-fill".into()),
                error: if done.is_empty() {
                    Some("nothing filled".into())
                } else {
                    None
                },
                secret_output: "suppressed",
            })
            .unwrap()
        }

        "knox_use" => {
            let q = v.get("query").and_then(|q| q.as_str()).unwrap_or("");
            let field = v
                .get("field")
                .and_then(|f| f.as_str())
                .unwrap_or("password");
            let via = v
                .get("via")
                .and_then(|x| x.as_str())
                .unwrap_or("type-frontmost");
            let target = v.get("target_app").and_then(|t| t.as_str());
            let mode = match via {
                "paste" | "paste-frontmost" => "paste",
                "dry-run" | "dry_run" => "dry-run",
                _ => "type",
            };
            serde_json::to_string(&knox::use_frontmost(q, field, mode, target)).unwrap()
        }

        // ---- session save / shim (restore) ----
        // save: persist current page HTML + history + flags under a unique id
        // load: shim that blob into *this* session (new SESS keeps new breadcrumbs;
        //       shim_from records the saved id for full lineage)
        "session_save" | "save_session" => {
            let name = v
                .get("name")
                .or_else(|| v.get("id"))
                .and_then(|n| n.as_str())
                .unwrap_or("session");
            let note = v
                .get("note")
                .and_then(|n| n.as_str())
                .map(|s| s.to_string());
            let page = eng.export_page();
            match session_store::save(
                &session.trace.root,
                &trace::run_root(),
                name,
                &session.history,
                session.ai_vis,
                page,
                note,
            ) {
                Ok(saved) => ok_json(serde_json::json!({
                    "ok": true,
                    "action": "session_save",
                    "id": saved.id,
                    "name": saved.name,
                    "saved_at": saved.saved_at,
                    "url": saved.page.url,
                    "title": saved.page.title,
                    "html_bytes": saved.page.html.len(),
                    "history_len": saved.history.len(),
                    "source_sess": saved.source_sess,
                    "sessions_dir": session_store::sessions_dir_display(),
                    "english": format!(
                        "Saved session `{}` ({} HTML bytes). Load later with session_load id/name.",
                        saved.name,
                        saved.page.html.len()
                    ),
                })),
                Err(e) => err("session_save_failed", &e),
            }
        }

        "session_load" | "load_session" | "session_shim" | "shim_session" => {
            let id = v
                .get("id")
                .or_else(|| v.get("name"))
                .or_else(|| v.get("path"))
                .and_then(|x| x.as_str())
                .unwrap_or("");
            if id.is_empty() {
                return err("bad_args", "session_load requires id or name");
            }
            match session_store::load(id) {
                Ok(saved) => {
                    if let Err(e) = eng.import_page(&saved.page) {
                        return err("session_shim_failed", &e);
                    }
                    session.history = saved.history.clone();
                    if session.history.is_empty() {
                        if let Some(u) = saved.page.url.clone() {
                            session.history.push(u);
                        }
                    }
                    session.forward.clear();
                    session.ai_vis = saved.ai_vis;
                    if let Some(surface) = live.as_mut() {
                        surface.set_ai_vis(saved.ai_vis);
                        // Live WebView: navigate to URL (HTML shim is exact on StaticEngine;
                        // live pane re-fetches — still one coherent session for the agent).
                        if let Some(u) = saved.page.url.as_ref() {
                            let js = format!(
                                "window.location.assign({});",
                                serde_json::to_string(u).unwrap_or_else(|_| "\"\"".into())
                            );
                            let _ = surface.eval_js(&js);
                        }
                    }
                    let shim_crumb = format!("{}.SHIM.{}", session.trace.root, {
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis())
                            .unwrap_or(0)
                    });
                    trace::emit(
                        &shim_crumb,
                        Some(&session.trace.root),
                        "session_shim",
                        &format!(
                            "Shimmed saved session `{}` ({}) into current session {}.",
                            saved.name, saved.id, session.trace.root
                        ),
                        serde_json::json!({
                            "shim_from": saved.id,
                            "shim_from_sess": saved.source_sess,
                            "into_sess": session.trace.root,
                            "url": saved.page.url,
                            "html_bytes": saved.page.html.len(),
                            "history_len": session.history.len(),
                        }),
                    );
                    ok_json(serde_json::json!({
                        "ok": true,
                        "action": "session_shim",
                        "shim_from": saved.id,
                        "shim_from_name": saved.name,
                        "shim_from_sess": saved.source_sess,
                        "into_sess": session.trace.root,
                        "shim_crumb": shim_crumb,
                        "url": eng.current_url(),
                        "title": saved.page.title,
                        "html_bytes": eng.html_bytes(),
                        "history_len": session.history.len(),
                        "ai_vis": session.ai_vis,
                        "english": format!(
                            "Session `{}` shimmed into {}. Page URL: {:?}.",
                            saved.name,
                            session.trace.root,
                            eng.current_url()
                        ),
                    }))
                }
                Err(e) => err("session_load_failed", &e),
            }
        }

        "session_list" | "list_sessions" => match session_store::list() {
            Ok(list) => ok_json(serde_json::json!({
                "ok": true,
                "action": "session_list",
                "sessions_dir": session_store::sessions_dir_display(),
                "count": list.len(),
                "sessions": list,
                "english": format!("Listed {} saved session(s).", list.len()),
            })),
            Err(e) => err("session_list_failed", &e),
        },

        "session_delete" | "delete_session" => {
            let id = v
                .get("id")
                .or_else(|| v.get("name"))
                .and_then(|x| x.as_str())
                .unwrap_or("");
            if id.is_empty() {
                return err("bad_args", "session_delete requires id or name");
            }
            match session_store::delete(id) {
                Ok(path) => ok_json(serde_json::json!({
                    "ok": true,
                    "action": "session_delete",
                    "deleted": path,
                    "english": format!("Deleted saved session file {path}."),
                })),
                Err(e) => err("session_delete_failed", &e),
            }
        }

        // ---- Hancock: ask the human to sign a permission ----
        // STILL_PENDING / missing CLI is never approval. Only APPROVED_AND_RAN (or AUTO_*) means go.
        "hancock_request" | "ask_hancock" | "request_permission" => {
            let action = v
                .get("action")
                .or_else(|| v.get("chrime_action"))
                .and_then(|a| a.as_str())
                .unwrap_or("custom");
            let why = v
                .get("why")
                .or_else(|| v.get("reason"))
                .and_then(|w| w.as_str())
                .unwrap_or("");
            let risk = v.get("risk").and_then(|r| r.as_str()).unwrap_or("high");
            let telos = v.get("telos").and_then(|t| t.as_str());
            let detail = v.get("detail").cloned().unwrap_or_else(|| {
                serde_json::json!({
                    "url": eng.current_url(),
                    "sess": session.trace.root,
                })
            });
            let wait = v.get("wait").and_then(|w| w.as_bool()).unwrap_or(true);
            let timeout = v
                .get("timeout_seconds")
                .or_else(|| v.get("timeout"))
                .and_then(|t| t.as_u64())
                .unwrap_or(600);
            let r = hancock::request(
                &session.trace.root,
                action,
                why,
                risk,
                telos,
                detail,
                wait,
                timeout,
            );
            r.to_json()
        }

        "hancock_wait" => {
            let id = v
                .get("id")
                .or_else(|| v.get("hancock_id"))
                .and_then(|i| i.as_str())
                .unwrap_or("");
            if id.is_empty() {
                return err(
                    "bad_args",
                    "hancock_wait requires id (req_… from hancock_request)",
                );
            }
            let timeout = v
                .get("timeout_seconds")
                .or_else(|| v.get("timeout"))
                .and_then(|t| t.as_u64())
                .unwrap_or(600);
            let r = hancock::wait(&session.trace.root, id, timeout);
            // reuse to_json shape with action override
            let mut j: serde_json::Value =
                serde_json::from_str(&r.to_json()).unwrap_or(serde_json::json!({}));
            if let serde_json::Value::Object(ref mut m) = j {
                m.insert("action".into(), serde_json::json!("hancock_wait"));
            }
            j.to_string()
        }

        "hancock_pending" => hancock::pending(&session.trace.root).to_string(),

        "set_ai_vis" | "ai_vis" => {
            let on = v
                .get("on")
                .or_else(|| v.get("enabled"))
                .and_then(|x| x.as_bool());
            let Some(on) = on else {
                return err("bad_args", "set_ai_vis requires {\"on\": true|false}");
            };
            session.ai_vis = on;
            if let Some(surface) = live.as_mut() {
                surface.set_ai_vis(on);
            }
            ok_json(serde_json::json!({
                "ok": true,
                "ai_vis": on,
                "live": has_live,
                "note": if has_live { "applied" } else { "headless: flag stored; paint needs live surface" }
            }))
        }

        "toggle_ai_vis" => {
            let on = live
                .as_ref()
                .map(|l| !l.ai_vis())
                .unwrap_or(!session.ai_vis);
            session.ai_vis = on;
            if let Some(surface) = live.as_mut() {
                surface.set_ai_vis(on);
            }
            ok_json(serde_json::json!({ "ok": true, "ai_vis": on }))
        }

        "ai_marks" => {
            let count = live.as_ref().map(|l| l.mark_count()).unwrap_or(0);
            ok_json(serde_json::json!({
                "ok": true,
                "count": count,
                "ai_vis": live.as_ref().map(|l| l.ai_vis()).unwrap_or(session.ai_vis),
            }))
        }

        "wait" => {
            let ms = v.get("ms").and_then(|n| n.as_u64()).unwrap_or(0);
            if ms > 0 {
                thread::sleep(Duration::from_millis(ms.min(60_000)));
            }
            ok_json(serde_json::json!({ "ok": true, "waited_ms": ms.min(60_000) }))
        }

        "quit" => {
            // Don't kill GUI from a TCP client by default — only stdio --api exits.
            // Ask the loop to stop instead of exiting the process: an engine with state to
            // flush (Servo's cookie jar) needs its Drop to run.
            if v.get("force").and_then(|f| f.as_bool()) == Some(true) {
                session.quit = true;
                return ok_json(serde_json::json!({
                    "ok": true,
                    "action": "quit",
                    "note": "shutting down cleanly so engine state (cookie jar) is flushed"
                }));
            }
            ok_json(serde_json::json!({
                "ok": true,
                "action": "quit_ignored",
                "note": "pass {\"op\":\"quit\",\"force\":true} to exit the process"
            }))
        }

        "" => err("bad_json", "missing op"),
        other => err(
            "unknown_op",
            &format!("unknown op '{other}' — try {{\"op\":\"help\"}}"),
        ),
    }
}

fn fill_js(which: &str, text: &str) -> String {
    match which {
        "login" | "password" => knox::fill_field_js(which, text),
        selector => {
            let sel = serde_json::to_string(selector).unwrap_or_else(|_| "\"\"".into());
            let val = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
            format!(
                r#"(function(){{
  const sel = {sel};
  const value = {val};
  const el = document.querySelector(sel);
  if (!el) return 'no-field';
  el.focus();
  try {{
    const proto = window.HTMLInputElement && HTMLInputElement.prototype;
    const desc = proto && Object.getOwnPropertyDescriptor(proto, 'value');
    if (desc && desc.set) desc.set.call(el, value);
    else el.value = value;
  }} catch (e) {{ el.value = value; }}
  el.dispatchEvent(new Event('input', {{bubbles:true}}));
  el.dispatchEvent(new Event('change', {{bubbles:true}}));
  return 'filled';
}})()"#
            )
        }
    }
}

/// JSONL over stdin/stdout (classic agent pipe).
pub fn run_stdio(eng: &mut dyn Engine) {
    trace::run_start();
    let mut session = Session::new();
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let resp = dispatch(eng, &mut session, None, line);
        let _ = writeln!(out, "{resp}");
        let _ = out.flush();
        if session.quit {
            break;
        }
    }
}

/// Spawn a JSONL TCP server. Each line is one op; one line response.
/// `cmd_tx` sends requests to the owner; owner replies on the oneshot sender.
/// Only used by the optional `gui` feature (WebView shell).
#[cfg(feature = "gui")]
pub type ApiReply = Sender<String>;

#[cfg(feature = "gui")]
pub enum ApiCmd {
    /// Execute this JSON line; send the response string on `reply`.
    Line { line: String, reply: ApiReply },
}

/// Background listener. Non-blocking for the GUI event loop — replies are produced by the owner.
#[cfg(feature = "gui")]
pub fn spawn_listener(addr: &str, cmd_tx: Sender<ApiCmd>) -> Result<String, String> {
    let listener = TcpListener::bind(addr).map_err(|e| format!("bind {addr}: {e}"))?;
    listener.set_nonblocking(false).map_err(|e| e.to_string())?;
    let bound = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| addr.to_string());
    let bound_log = bound.clone();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let cmd_tx = cmd_tx.clone();
            thread::spawn(move || {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(600)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(60)));
                let Ok(clone) = stream.try_clone() else {
                    return;
                };
                let mut reader = BufReader::new(clone);
                let mut writer = stream;
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {}
                        Err(_) => break,
                    }
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let (reply_tx, reply_rx): (ApiReply, Receiver<String>) = mpsc::channel();
                    if cmd_tx
                        .send(ApiCmd::Line {
                            line: trimmed.to_string(),
                            reply: reply_tx,
                        })
                        .is_err()
                    {
                        break;
                    }
                    let resp = reply_rx
                        .recv_timeout(Duration::from_secs(300))
                        .unwrap_or_else(|_| err("timeout", "api handler timed out (300s)"));
                    if writeln!(writer, "{resp}").is_err() {
                        break;
                    }
                    let _ = writer.flush();
                }
            });
        }
    });
    eprintln!("chrime api listening on {bound_log} (JSONL, zero clicks)");
    Ok(bound_log)
}

/// Headless TCP server owning its own StaticEngine (no GUI).
pub fn run_tcp_headless(addr: &str, eng: &mut dyn Engine) -> Result<(), String> {
    trace::run_start();
    let listener = TcpListener::bind(addr).map_err(|e| format!("bind {addr}: {e}"))?;
    eprintln!(
        "chrime api listening on {}  (trace {})",
        listener.local_addr().unwrap(),
        trace::run_root()
    );
    // Single-threaded accept; each TCP client gets its own SESS breadcrumb.
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let mut session = Session::new();
        let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
        let mut writer = stream;
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {}
                Err(_) => break,
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let resp = dispatch(eng, &mut session, None, trimmed);
            if writeln!(writer, "{resp}").is_err() {
                break;
            }
            let _ = writer.flush();
            if session.quit {
                return Ok(());
            }
        }
    }
    Ok(())
}
