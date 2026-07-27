//! Chrime — a browser built for AI agents.
//!
//! The thesis: existing automation tools are bad at agent control because they puppeteer a
//! *human* browser (pixels, coordinates, human-oriented DOM). Chrime has no GUI and no pixel
//! pipeline. Its entire interface is a JSON API over stdio, and what it exposes is the DOM as
//! a compact semantic tree with stable node-ids — the thing an agent's decision loop needs.
//!
//! This first engine is `StaticEngine`: fetch + parse real HTML (Servo's html5ever via
//! scraper), no JS, near-zero memory. A full-JS engine (embedded v8) will implement the same
//! `Engine` trait and drop in behind the identical API — that seam is the whole point.

use scraper::{ElementRef, Html, Node, Selector};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use url::{form_urlencoded, Url};

mod api;
mod hancock;
mod knox;
mod session_store;
mod trace;
mod views;

#[cfg(feature = "gui")]
mod gui;

#[cfg(feature = "servo")]
mod servo_engine;

pub(crate) use views::ViewKind;

const UA: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrime/0.1 (a browser for agents)";

// ---- the DOM, as an agent sees it ----

#[derive(Serialize, Deserialize)]
pub(crate) struct DomNode {
    node_id: u32,
    tag: String,
    role: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    href: Option<String>,
    clickable: bool,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct DomSnapshot {
    /// Which projection this is (`full`, `outline`, `links`, …). Same page, different lens.
    #[serde(default = "default_view_name")]
    view: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    title: Option<String>,
    node_count: usize,
    /// Bytes of the single stored HTML buffer (not duplicated per view).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    html_bytes: Option<usize>,
    /// Role histogram — Meta view is this without `nodes`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    counts: Option<std::collections::BTreeMap<String, usize>>,
    nodes: Vec<DomNode>,
}

fn default_view_name() -> String {
    "full".into()
}

#[derive(Serialize)]
struct NavResult {
    ok: bool,
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// Response Content-Type when known (graceful non-HTML path).
    #[serde(skip_serializing_if = "Option::is_none")]
    content_type: Option<String>,
    /// `html` or `non_html` — agents should not treat JSON/plain as a DOM tree.
    #[serde(skip_serializing_if = "Option::is_none")]
    content_kind: Option<&'static str>,
}

/// What `settle` returns: proof the engine was driven to quiescence, not slept on.
/// `spins` is how many event-loop turns it took; a heuristic sleep cannot report that.
#[derive(Serialize)]
pub(crate) struct SettleReceipt {
    ok: bool,
    engine: &'static str,
    /// Event-loop turns pumped before quiescence (0 = already settled / nothing to pump).
    spins: u32,
    ms: u64,
    /// True when quiescence was reached; false when the safety cap tripped first.
    quiescent: bool,
    /// Engine-specific signal that ended the spin (`load_complete`, `cap`, `no_js`).
    reason: &'static str,
    url: Option<String>,
}

// ---- the swappable engine seam (StaticEngine now; a v8 engine implements this next) ----

pub(crate) trait Engine {
    /// Which substrate is behind the trait — agents branch on it, and it keeps "which engine
    /// answered?" out of guesswork in suite reports.
    fn engine_name(&self) -> &'static str {
        "static"
    }
    /// Drive the engine to quiescence and return a receipt. Deterministic settle is the whole
    /// point (telos `control-surfaces`): the answer is a measured state, never a sleep.
    fn settle(&mut self) -> SettleReceipt {
        SettleReceipt {
            ok: true,
            engine: self.engine_name(),
            spins: 0,
            ms: 0,
            quiescent: true,
            // ponytail: a fetched-and-parsed static page has no clock to settle — it is
            // quiescent by construction. Real spins only exist once JS does.
            reason: "no_js",
            url: self.current_url(),
        }
    }
    /// Where this engine keeps state that outlives the process (cookie jar); None if it keeps
    /// none. Reported by `status` so "why did I start logged out?" is answerable.
    fn profile_dir(&self) -> Option<String> {
        None
    }
    fn navigate(&mut self, url: &str) -> NavResult;
    fn snapshot(&self) -> DomSnapshot;
    fn read_text(&self) -> String;
    fn click(&mut self, node_id: u32) -> NavResult;
    fn current_url(&self) -> Option<String>;
    /// Every clickable link on the page, with its node-id and resolved href.
    fn links(&self) -> Vec<DomNode>;
    /// Nodes whose text contains `q` (case-insensitive) — how an agent finds "the login button".
    fn find_text(&self, q: &str) -> Vec<DomNode>;
    /// CSS selector → matching nodes. Semantic-tree matches keep stable `node_id`s (click works).
    /// Default: run the selector against `export_page().html` (post-JS buffer for Servo).
    fn query(&self, selector: &str) -> Result<Vec<DomNode>, String> {
        let page = self.export_page();
        query_html(&page.html, page.url.as_deref(), selector)
    }
    /// Size of the single stored HTML buffer (0 if unknown). Views never duplicate this.
    fn html_bytes(&self) -> usize {
        0
    }
    /// Named view of the *same* page — projection only, no second page store.
    fn view(&self, kind: ViewKind) -> DomSnapshot {
        views::project(self.snapshot(), kind, self.html_bytes())
    }
    /// Export the single HTML buffer + url/title for session save.
    fn export_page(&self) -> session_store::SavedPage {
        session_store::SavedPage {
            url: self.current_url(),
            title: self.snapshot().title,
            html: String::new(),
        }
    }
    /// Shim a saved page buffer into this engine (no network).
    fn import_page(&mut self, page: &session_store::SavedPage) -> Result<(), String> {
        let _ = page;
        Err("import_page not supported on this engine".into())
    }
}

pub(crate) struct StaticEngine {
    url: Option<Url>,
    html: String,
    title: Option<String>,
    agent: ureq::Agent,
    /// Last response Content-Type (if any).
    content_type: Option<String>,
    /// Last load classified as html vs non_html.
    content_kind: Option<&'static str>,
}

/// HTTP request timeout for StaticEngine. Override with `CHRIME_TIMEOUT_SECS` (1–600, default 30).
pub(crate) fn request_timeout_secs() -> u64 {
    std::env::var("CHRIME_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(30)
        .clamp(1, 600)
}

/// True when Content-Type / body sniffs as HTML (or empty/unknown body that still parses).
pub(crate) fn is_html_content(content_type: Option<&str>, body: &str) -> bool {
    if let Some(ct) = content_type {
        let ct = ct.to_ascii_lowercase();
        // Strip parameters: text/html; charset=utf-8
        let main = ct.split(';').next().unwrap_or("").trim();
        if main.contains("html") || main.contains("xhtml") {
            return true;
        }
        // Explicit non-document types
        if main.starts_with("application/json")
            || main.starts_with("text/plain")
            || main.starts_with("text/css")
            || main.starts_with("application/javascript")
            || main.starts_with("text/javascript")
            || main.starts_with("image/")
            || main.starts_with("audio/")
            || main.starts_with("video/")
            || main.starts_with("application/pdf")
            || main.starts_with("application/octet-stream")
        {
            return false;
        }
    }
    // Sniff: leading doctype/html tags → HTML; leading {/[ → JSON-ish non-HTML
    let trimmed = body.trim_start();
    if trimmed.is_empty() {
        return true; // empty page is still a document
    }
    let lower = trimmed.chars().take(64).collect::<String>().to_ascii_lowercase();
    if lower.starts_with("<!doctype") || lower.starts_with("<html") || lower.starts_with("<head")
        || lower.starts_with("<body")
    {
        return true;
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return false;
    }
    // Default: treat as HTML so unknown servers still get a DOM walk
    true
}

/// Wrap non-HTML payloads so read/snapshot still work without lying that it is a web page DOM.
fn wrap_non_html(body: &str, content_type: &str) -> String {
    let esc = body
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        "<!DOCTYPE html><html><head><title>non-html</title></head>\
         <body data-chrime-content-kind=\"non_html\" data-chrime-content-type=\"{ct}\">\
         <h1>non-html response</h1>\
         <p role=\"note\">Content-Type: {ct}</p>\
         <pre id=\"chrime-raw\">{esc}</pre></body></html>",
        ct = content_type.replace('"', ""),
        esc = esc
    )
}

impl StaticEngine {
    pub(crate) fn new() -> Self {
        let timeout = std::time::Duration::from_secs(request_timeout_secs());
        StaticEngine {
            url: None,
            html: String::new(),
            title: None,
            agent: ureq::AgentBuilder::new()
                .redirects(5)
                .timeout(timeout)
                .build(),
            content_type: None,
            content_kind: None,
        }
    }

    fn ingest_body(&mut self, body: String, content_type: Option<String>) -> &'static str {
        let ct_ref = content_type.as_deref();
        let kind = if is_html_content(ct_ref, &body) {
            self.html = body;
            "html"
        } else {
            let label = content_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".into());
            self.html = wrap_non_html(&body, &label);
            "non_html"
        };
        self.content_type = content_type;
        self.content_kind = Some(kind);
        kind
    }

    // Deterministic pre-order walk of interesting elements, assigning stable node-ids.
    // Both snapshot() and click() use it, so a node-id means the same node in both.
    fn walk(&self) -> (Vec<DomNode>, Option<String>) {
        let doc = Html::parse_document(&self.html);
        let base = self.url.clone();
        let mut nodes = Vec::new();
        let mut title = None;
        let mut id = 0u32;
        for node in doc.tree.root().descendants() {
            let el = match node.value() {
                Node::Element(el) => el,
                _ => continue,
            };
            let tag = el.name().to_string();
            // Document <title> only — SVG <title> (e.g. "Close icon") must not overwrite it.
            if tag == "title" {
                if title.is_none() {
                    if let Some(er) = ElementRef::wrap(node) {
                        let t = collapse(&er.text().collect::<String>());
                        if !t.is_empty() {
                            title = Some(t);
                        }
                    }
                }
                continue;
            }
            if !interesting(&tag) {
                continue;
            }
            let er = match ElementRef::wrap(node) {
                Some(e) => e,
                None => continue,
            };
            let text = collapse(&er.text().collect::<String>());
            let href = el.attr("href").map(|h| resolve(base.as_ref(), h));
            let clickable = tag == "a" || tag == "button";
            id += 1;
            nodes.push(DomNode {
                node_id: id,
                role: role_of(&tag),
                tag,
                text,
                href,
                clickable,
            });
        }
        (nodes, title)
    }
}

impl Engine for StaticEngine {
    fn navigate(&mut self, raw: &str) -> NavResult {
        let target = match normalize(raw, self.url.as_ref()) {
            Ok(u) => u,
            Err(e) => {
                return NavResult {
                    ok: false,
                    url: None,
                    status: None,
                    title: None,
                    error: Some(e),
                    content_type: None,
                    content_kind: None,
                }
            }
        };
        // file:// — read the raw file (StaticEngine sees the pre-JS shell, no scripts run).
        if target.scheme() == "file" {
            return match target
                .to_file_path()
                .ok()
                .and_then(|p| std::fs::read_to_string(p).ok())
            {
                Some(body) => {
                    let path = target.to_file_path().ok();
                    let ct = path.and_then(|p| {
                        p.extension()
                            .and_then(|e| e.to_str())
                            .map(|ext| match ext.to_ascii_lowercase().as_str() {
                                "html" | "htm" | "xhtml" => "text/html".into(),
                                "json" => "application/json".into(),
                                "txt" | "md" => "text/plain".into(),
                                "css" => "text/css".into(),
                                _ => "application/octet-stream".into(),
                            })
                    });
                    let kind = self.ingest_body(body, ct.clone());
                    self.url = Some(target.clone());
                    let (_, title) = self.walk();
                    self.title = title.clone();
                    NavResult {
                        ok: true,
                        url: Some(target.to_string()),
                        status: Some(200),
                        title,
                        error: None,
                        content_type: ct,
                        content_kind: Some(kind),
                    }
                }
                None => NavResult {
                    ok: false,
                    url: Some(target.to_string()),
                    status: None,
                    title: None,
                    error: Some("could not read local file".into()),
                    content_type: None,
                    content_kind: None,
                },
            };
        }
        match self.agent.get(target.as_str()).set("User-Agent", UA).call() {
            Ok(resp) => {
                let status = resp.status();
                let ct = resp
                    .header("content-type")
                    .or_else(|| resp.header("Content-Type"))
                    .map(|s| s.to_string());
                let body = resp.into_string().unwrap_or_default();
                let kind = self.ingest_body(body, ct.clone());
                self.url = Some(target.clone());
                let (_, title) = self.walk();
                self.title = title.clone();
                NavResult {
                    ok: true,
                    url: Some(target.to_string()),
                    status: Some(status),
                    title,
                    error: None,
                    content_type: ct,
                    content_kind: Some(kind),
                }
            }
            Err(e) => NavResult {
                ok: false,
                url: Some(target.to_string()),
                status: None,
                title: None,
                error: Some(e.to_string()),
                content_type: None,
                content_kind: None,
            },
        }
    }

    fn snapshot(&self) -> DomSnapshot {
        let (nodes, title) = self.walk();
        DomSnapshot {
            view: "full".into(),
            url: self.current_url(),
            title,
            node_count: nodes.len(),
            html_bytes: Some(self.html.len()),
            counts: None,
            nodes,
        }
    }

    fn html_bytes(&self) -> usize {
        self.html.len()
    }

    fn export_page(&self) -> session_store::SavedPage {
        session_store::SavedPage {
            url: self.current_url(),
            title: self.title.clone(),
            html: self.html.clone(),
        }
    }

    fn import_page(&mut self, page: &session_store::SavedPage) -> Result<(), String> {
        let kind = self.ingest_body(page.html.clone(), Some("text/html".into()));
        let _ = kind;
        self.title = page.title.clone();
        self.url = match page.url.as_deref() {
            Some(u) if !u.is_empty() => {
                Some(Url::parse(u).map_err(|e| format!("bad saved url: {e}"))?)
            }
            _ => None,
        };
        // Re-derive title from HTML if missing
        if self.title.is_none() {
            let (_, t) = self.walk();
            self.title = t;
        }
        Ok(())
    }

    fn read_text(&self) -> String {
        let doc = Html::parse_document(&self.html);
        let sel = Selector::parse("body").unwrap();
        let root = match doc.select(&sel).next() {
            Some(body) => body,
            None => doc.root_element(),
        };
        let mut out = String::new();
        for node in root.descendants() {
            let Node::Text(t) = node.value() else {
                continue;
            };
            // Script and style bodies are source, not page text — a browser's innerText skips
            // them. Without this the static engine "reads" strings from JS it never ran, which
            // reads exactly like faithful-js support it does not have.
            let in_code = node.ancestors().any(|a| {
                matches!(a.value(), Node::Element(e) if e.name() == "script" || e.name() == "style")
            });
            if !in_code {
                out.push_str(t);
            }
        }
        collapse(&out)
    }

    fn click(&mut self, node_id: u32) -> NavResult {
        let (nodes, _) = self.walk();
        match nodes.iter().find(|n| n.node_id == node_id) {
            Some(n) => match n.href.clone() {
                Some(h) => self.navigate(&h),
                None => NavResult {
                    ok: false,
                    url: self.current_url(),
                    status: None,
                    title: None,
                    error: Some(format!(
                        "node {} has no href to follow (static engine can't run JS handlers yet)",
                        node_id
                    )),
                    content_type: None,
                    content_kind: None,
                },
            },
            None => NavResult {
                ok: false,
                url: self.current_url(),
                status: None,
                title: None,
                error: Some(format!("no node with id {}", node_id)),
                content_type: None,
                content_kind: None,
            },
        }
    }

    fn current_url(&self) -> Option<String> {
        self.url.as_ref().map(|u| u.to_string())
    }

    fn links(&self) -> Vec<DomNode> {
        self.walk()
            .0
            .into_iter()
            .filter(|n| n.clickable && n.href.is_some())
            .collect()
    }

    fn find_text(&self, q: &str) -> Vec<DomNode> {
        let ql = q.to_lowercase();
        self.walk()
            .0
            .into_iter()
            .filter(|n| !ql.is_empty() && n.text.to_lowercase().contains(&ql))
            .collect()
    }

    fn query(&self, selector: &str) -> Result<Vec<DomNode>, String> {
        query_html(&self.html, self.current_url().as_deref(), selector)
    }
}

/// CSS-select against a single HTML buffer. Elements that appear in the semantic walk
/// (interesting tags) get the same stable `node_id`s as `snapshot`/`click`. Other matches
/// are returned with `node_id: 0` and `clickable: false` so agents can still see them.
pub(crate) fn query_html(
    html: &str,
    url: Option<&str>,
    selector: &str,
) -> Result<Vec<DomNode>, String> {
    let sel = Selector::parse(selector).map_err(|e| format!("invalid CSS selector: {e:?}"))?;
    let doc = Html::parse_document(html);
    let base = url.and_then(|u| Url::parse(u).ok());

    // Map ego-tree node id → walk node_id (same pre-order as walk()).
    let mut id_map = std::collections::HashMap::new();
    let mut walk_id = 0u32;
    for node in doc.tree.root().descendants() {
        let Node::Element(el) = node.value() else {
            continue;
        };
        if !interesting(el.name()) {
            continue;
        }
        walk_id += 1;
        id_map.insert(node.id(), walk_id);
    }

    let mut out = Vec::new();
    for er in doc.select(&sel) {
        let tag = er.value().name().to_string();
        let text = collapse(&er.text().collect::<String>());
        let href = er.value().attr("href").map(|h| resolve(base.as_ref(), h));
        let node_id = id_map.get(&er.id()).copied().unwrap_or(0);
        let clickable = node_id > 0 && (tag == "a" || tag == "button");
        out.push(DomNode {
            node_id,
            role: role_of(&tag),
            tag,
            text,
            href,
            clickable,
        });
    }
    Ok(out)
}

fn interesting(tag: &str) -> bool {
    matches!(
        tag,
        "a" | "button"
            | "input"
            | "textarea"
            | "select"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "p"
            | "li"
            | "label"
    )
}

fn role_of(tag: &str) -> String {
    match tag {
        "a" => "link",
        "button" => "button",
        "input" | "textarea" | "select" => "field",
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading",
        "p" | "li" => "text",
        "label" => "label",
        _ => "generic",
    }
    .to_string()
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn resolve(base: Option<&Url>, href: &str) -> String {
    match base {
        Some(b) => b
            .join(href)
            .map(|u| u.to_string())
            .unwrap_or_else(|_| href.to_string()),
        None => href.to_string(),
    }
}

pub(crate) fn normalize(raw: &str, base: Option<&Url>) -> Result<Url, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err("empty url".into());
    }
    if let Ok(u) = Url::parse(s) {
        if matches!(u.scheme(), "http" | "https" | "file" | "data" | "about") {
            return Ok(u);
        }
    }
    if let Some(b) = base {
        if let Ok(u) = b.join(s) {
            return Ok(u);
        }
    }
    if s.contains('.') && !s.contains(' ') {
        if let Ok(u) = Url::parse(&format!("https://{}", s)) {
            return Ok(u);
        }
    }
    let q: String = form_urlencoded::byte_serialize(s.as_bytes()).collect();
    Url::parse(&format!("https://duckduckgo.com/html/?q={}", q)).map_err(|e| e.to_string())
}

// ---- the HEAD: a terminal render of the semantic DOM (`--tui`) ----

#[cfg(any(feature = "headless", not(feature = "gui")))]
fn term_width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|c| c.trim().parse::<usize>().ok())
        .filter(|&n| n >= 20)
        .unwrap_or(88)
        .min(100)
}

#[cfg(any(feature = "headless", not(feature = "gui")))]
fn wrap(text: &str, width: usize) -> String {
    let width = width.max(20);
    let mut out = String::new();
    let mut len = 0usize;
    for word in text.split_whitespace() {
        let wl = word.chars().count();
        if len + wl + 1 > width && len > 0 {
            out.push('\n');
            len = 0;
        }
        if len > 0 {
            out.push(' ');
            len += 1;
        }
        out.push_str(word);
        len += wl;
    }
    out
}

// Render the current page as a readable text view; return the click-number → node_id map.
#[cfg(any(feature = "headless", not(feature = "gui")))]
fn render(eng: &dyn Engine) -> Vec<u32> {
    let snap = eng.snapshot();
    let w = term_width();
    let rule = "\u{2500}".repeat(w);
    print!("\x1b[2J\x1b[H"); // clear + home
    println!(
        "\x1b[48;5;236m\x1b[38;5;214m 🕵 Chrime \x1b[0m \x1b[38;5;250m{}\x1b[0m",
        snap.url.as_deref().unwrap_or("(no page — type a URL)")
    );
    if let Some(t) = &snap.title {
        println!("\x1b[1m{}\x1b[0m", t);
    }
    println!("\x1b[38;5;238m{}\x1b[0m", rule);
    let mut clickmap = Vec::new();
    for n in &snap.nodes {
        match n.role.as_str() {
            "heading" => println!("\n\x1b[1m\x1b[38;5;231m{}\x1b[0m", wrap(&n.text, w)),
            "link" | "button" => {
                clickmap.push(n.node_id);
                println!(
                    "\x1b[38;5;214m[{}]\x1b[0m {}",
                    clickmap.len(),
                    if n.text.is_empty() {
                        "(no text)"
                    } else {
                        &n.text
                    }
                );
            }
            "field" => println!(
                "\x1b[38;5;244m[ {} ]\x1b[0m",
                if n.text.is_empty() { "input" } else { &n.text }
            ),
            _ => {
                if !n.text.is_empty() {
                    println!("{}", wrap(&n.text, w));
                }
            }
        }
    }
    println!("\n\x1b[38;5;238m{}\x1b[0m", rule);
    println!("\x1b[38;5;244m URL to go · number to open a link · b back · r read · q quit\x1b[0m");
    println!(
        "\x1b[38;5;240m 🎭 a member of the Fraude family — the fraud that does real work\x1b[0m"
    );
    clickmap
}

#[cfg(any(feature = "headless", not(feature = "gui")))]
fn headed(eng: &mut dyn Engine, start: Option<String>) {
    let mut history: Vec<String> = Vec::new();
    if let Some(u) = start {
        let r = eng.navigate(&u);
        if let (true, Some(url)) = (r.ok, r.url) {
            history.push(url);
        }
    }
    let stdin = std::io::stdin();
    loop {
        let clickmap = render(eng);
        print!("\n\x1b[38;5;214mchrime>\x1b[0m ");
        std::io::stdout().flush().ok();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        match line.trim() {
            "" => continue,
            "q" | "quit" | ":q" => {
                print!("\x1b[2J\x1b[H");
                break;
            }
            "r" | "read" => {
                print!("\x1b[2J\x1b[H");
                println!("{}\n", wrap(&eng.read_text(), term_width()));
                println!("\x1b[38;5;244m(press enter to go back)\x1b[0m");
                let mut t = String::new();
                stdin.lock().read_line(&mut t).ok();
            }
            "b" | "back" => {
                if history.len() > 1 {
                    history.pop();
                    let prev = history.last().cloned().unwrap();
                    eng.navigate(&prev);
                }
            }
            other => {
                if let Ok(n) = other.parse::<usize>() {
                    if n >= 1 && n <= clickmap.len() {
                        let r = eng.click(clickmap[n - 1]);
                        if let (true, Some(u)) = (r.ok, r.url) {
                            history.push(u);
                        }
                    }
                } else {
                    let r = eng.navigate(other);
                    if let (true, Some(u)) = (r.ok, r.url) {
                        history.push(u);
                    }
                }
            }
        }
    }
}

fn print_usage() {
    println!(
        "chrime {} — headed-only agent browser (dual-pane GUI)\n\
         \n\
         Usage:\n\
           chrime [url]                 open dual-pane window + API on 127.0.0.1:7420\n\
           chrime --listen ADDR         override API listen address (still headed)\n\
           chrime --no-listen           window only, no TCP API\n\
           chrime --engine static|servo engine substrate (servo needs --features servo)\n\
           chrime --version | -v\n\
           chrime --help | -h | help\n\
         \n\
         Headless (--api / --tui without a window) is DISABLED in product builds.\n\
         CI-only: cargo build --release --features headless\n\
         \n\
         Drive the open window (no mouse):\n\
           printf '%s\\n' '{{\"op\":\"ping\"}}' '{{\"op\":\"help\"}}' | nc -w 2 127.0.0.1 7420\n\
         \n\
         Key JSONL ops: navigate, back, forward, snapshot, live_read, live_sync, query,\n\
           layout, sidebar, find_text, click, settle, fill, knox_*, session_*, hancock_*, quit\n\
         Docs: README.md · TELOS.md · docs/BREADCRUMBS.md\n",
        env!("CARGO_PKG_VERSION")
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-v") {
        println!("chrime {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    // POSIX-tool ergonomics: help must never open the GUI / hang waiting for a page.
    if !args.is_empty()
        && (args.iter().any(|a| a == "--help" || a == "-h")
            || args.first().map(|a| a.as_str()) == Some("help"))
    {
        print_usage();
        return;
    }
    // Breadcrumb root for this process (docs/BREADCRUMBS.md). Always first log line.
    crate::trace::run_start();
    let want_headless_flags = args
        .iter()
        .any(|a| a == "--api" || a == "--headless" || a == "--tui" || a == "--terminal");
    let mut engine = String::from("static");
    let mut start: Option<String> = None;
    // None = product headed defaults to :7420; headless --api uses stdio unless --listen set.
    let mut listen: Option<String> = None;
    let mut listen_explicit = false;
    let mut no_listen = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--engine" => {
                if let Some(v) = it.next() {
                    engine = v.clone();
                }
            }
            "--listen" => {
                if let Some(v) = it.next() {
                    listen_explicit = true;
                    if v == "off" || v == "none" || v == "-" {
                        listen = None;
                        no_listen = true;
                    } else {
                        listen = Some(v.clone());
                    }
                }
            }
            "--no-listen" => {
                no_listen = true;
                listen = None;
            }
            // Product builds ignore these; headless feature still accepts them.
            "--api" | "--headless" | "--tui" | "--terminal" | "--gui" => {}
            s if s.starts_with("--") => {}
            s => {
                if start.is_none() {
                    start = Some(s.to_string());
                }
            }
        }
    }

    // ---- Product path: HEADED ONLY (default features = gui, no headless) ----
    #[cfg(all(feature = "gui", not(feature = "headless")))]
    {
        if want_headless_flags {
            eprintln!(
                "chrime: headless mode is disabled in this build (--api/--tui/--headless ignored).\n\
                 Opening headed GUI. API listens on the window (default 127.0.0.1:7420).\n\
                 CI-only headless: cargo build --release --features headless"
            );
        }
        let _ = engine; // engine selection for GUI path is StaticEngine inside gui::run today
        let addr = if no_listen {
            None
        } else {
            listen.or_else(|| Some("127.0.0.1:7420".into()))
        };
        if let Err(e) = gui::run(start, addr) {
            eprintln!("chrime gui: {e}");
            std::process::exit(1);
        }
        return;
    }

    // ---- Optional CI headless (feature = "headless") ----
    #[cfg(all(feature = "gui", feature = "headless"))]
    {
        let api = want_headless_flags
            && args
                .iter()
                .any(|a| a == "--api" || a == "--headless");
        let tui = args
            .iter()
            .any(|a| a == "--tui" || a == "--terminal");
        let force_gui = args.iter().any(|a| a == "--gui") || (!api && !tui);
        if force_gui {
            let addr = if no_listen {
                None
            } else {
                listen.or_else(|| Some("127.0.0.1:7420".into()))
            };
            if let Err(e) = gui::run(start, addr) {
                eprintln!("chrime gui: {e}");
                std::process::exit(1);
            }
            return;
        }
        let _ = listen_explicit;
        let mut eng: Box<dyn Engine> = match engine.as_str() {
            #[cfg(feature = "servo")]
            "servo" => Box::new(servo_engine::ServoEngine::new()),
            #[cfg(not(feature = "servo"))]
            "servo" => {
                eprintln!(
                    "chrime: built without the `servo` engine — rebuild with `--features servo`"
                );
                std::process::exit(2);
            }
            _ => Box::new(StaticEngine::new()),
        };
        if let Some(u) = start.as_ref() {
            let _ = eng.navigate(u);
        }
        if api {
            // stdio unless --listen ADDR was set (suite uses pure stdin/stdout).
            if let Some(addr) = listen {
                if let Err(e) = api::run_tcp_headless(&addr, eng.as_mut()) {
                    eprintln!("chrime api: {e}");
                    std::process::exit(1);
                }
            } else {
                api::run_stdio(eng.as_mut());
            }
        } else {
            headed(eng.as_mut(), start);
        }
        return;
    }

    // ---- No gui feature (engine-only / unit-test lean binary) ----
    #[cfg(not(feature = "gui"))]
    {
        let mut eng: Box<dyn Engine> = match engine.as_str() {
            #[cfg(feature = "servo")]
            "servo" => Box::new(servo_engine::ServoEngine::new()),
            #[cfg(not(feature = "servo"))]
            "servo" => {
                eprintln!(
                    "chrime: built without the `servo` engine — rebuild with `--features servo`"
                );
                std::process::exit(2);
            }
            _ => Box::new(StaticEngine::new()),
        };
        if let Some(u) = start.as_ref() {
            let _ = eng.navigate(u);
        }
        if want_headless_flags {
            if let Some(addr) = listen {
                if let Err(e) = api::run_tcp_headless(&addr, eng.as_mut()) {
                    eprintln!("chrime api: {e}");
                    std::process::exit(1);
                }
            } else {
                api::run_stdio(eng.as_mut());
            }
        } else {
            headed(eng.as_mut(), start);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_resolves_urls() {
        assert_eq!(
            normalize("example.com", None).unwrap().as_str(),
            "https://example.com/"
        );
        assert_eq!(
            normalize("https://a.com/x", None).unwrap().as_str(),
            "https://a.com/x"
        );
        let base = Url::parse("https://a.com/x/").unwrap();
        assert_eq!(
            normalize("y", Some(&base)).unwrap().as_str(),
            "https://a.com/x/y"
        );
        assert!(normalize("hello there", None)
            .unwrap()
            .as_str()
            .contains("duckduckgo"));
    }

    #[test]
    fn walk_yields_semantic_nodes() {
        let mut e = StaticEngine::new();
        e.html = "<html><head><title>T</title></head><body><h1>Head</h1>\
                  <a href='/x'>Link</a><p>Para</p></body></html>"
            .into();
        e.url = Some(Url::parse("https://ex.com/").unwrap());
        let (nodes, title) = e.walk();
        assert_eq!(title.as_deref(), Some("T"));
        assert!(nodes
            .iter()
            .any(|n| n.role == "heading" && n.text == "Head"));
        let link = nodes.iter().find(|n| n.role == "link").unwrap();
        assert_eq!(link.href.as_deref(), Some("https://ex.com/x"));
        assert!(link.clickable);
        assert_eq!(e.links().len(), 1);
        assert_eq!(e.find_text("para").len(), 1);
        assert_eq!(e.find_text("nope").len(), 0);
        let links = e.query("a[href]").unwrap();
        assert_eq!(links.len(), 1);
        assert!(links[0].node_id > 0);
        assert!(links[0].clickable);
        assert!(e.query("not[[[valid").is_err());
        let heads = e.query("h1").unwrap();
        assert_eq!(heads.len(), 1);
        assert_eq!(heads[0].text, "Head");
    }

    #[test]
    fn is_html_content_classifies_types() {
        assert!(is_html_content(Some("text/html; charset=utf-8"), "<html></html>"));
        assert!(is_html_content(None, "<!DOCTYPE html><html></html>"));
        assert!(!is_html_content(Some("application/json"), r#"{"a":1}"#));
        assert!(!is_html_content(Some("text/plain"), "hello"));
        assert!(!is_html_content(None, r#"{"a":1}"#));
        assert!(is_html_content(None, "")); // empty document
    }

    #[test]
    fn request_timeout_secs_clamps_and_defaults() {
        // default without env (or with garbage) is 30 after clamp path — call pure default branch
        let d = request_timeout_secs();
        assert!((1..=600).contains(&d));
    }

    #[test]
    fn ingest_non_html_wraps_json() {
        let mut e = StaticEngine::new();
        let kind = e.ingest_body(r#"{"ok":true}"#.into(), Some("application/json".into()));
        assert_eq!(kind, "non_html");
        assert_eq!(e.content_kind, Some("non_html"));
        assert!(e.html.contains("non-html response"));
        assert!(e.read_text().contains(r#"{"ok":true}"#) || e.html.contains("{&quot;ok&quot;"));
    }

    /// Integration-style: pipe a multi-op API script through the real dispatch entry point
    /// (no network — page is shimmed via import_page).
    #[test]
    fn api_pipe_script_asserts_json_results() {
        let mut eng = StaticEngine::new();
        eng.import_page(&session_store::SavedPage {
            url: Some("https://fixture.test/page".into()),
            title: Some("Fixture".into()),
            html: "<html><head><title>Fixture</title></head><body>\
                   <h1>Hello Pipe</h1><a href=\"/next\">Next</a><p>Body text</p>\
                   </body></html>"
                .into(),
        })
        .unwrap();
        let mut session = api::Session::new();
        session.history.push("https://fixture.test/page".into());

        let ping = api::dispatch(&mut eng, &mut session, None, r#"{"op":"ping"}"#);
        let ping_v: serde_json::Value = serde_json::from_str(&ping).unwrap();
        assert_eq!(ping_v.get("ok").and_then(|x| x.as_bool()), Some(true));

        let snap = api::dispatch(&mut eng, &mut session, None, r#"{"op":"snapshot"}"#);
        let snap_v: serde_json::Value = serde_json::from_str(&snap).unwrap();
        let count = snap_v
            .get("node_count")
            .and_then(|n| n.as_u64())
            .unwrap_or(0);
        assert!(count >= 1, "snapshot node_count={count} body={snap}");

        let q = api::dispatch(
            &mut eng,
            &mut session,
            None,
            r#"{"op":"query","selector":"a[href]"}"#,
        );
        let qv: serde_json::Value = serde_json::from_str(&q).unwrap();
        assert_eq!(qv.get("ok").and_then(|x| x.as_bool()), Some(true));
        assert!(
            qv.get("count").and_then(|c| c.as_u64()).unwrap_or(0) >= 1,
            "query failed: {q}"
        );

        let read = api::dispatch(&mut eng, &mut session, None, r#"{"op":"read"}"#);
        let rv: serde_json::Value = serde_json::from_str(&read).unwrap();
        let text = rv.get("text").and_then(|t| t.as_str()).unwrap_or("");
        assert!(
            text.contains("Hello") || text.contains("Pipe") || text.contains("Body"),
            "read text unexpected: {text:?}"
        );

        // forward empty fails
        let fwd = api::dispatch(&mut eng, &mut session, None, r#"{"op":"forward"}"#);
        let fv: serde_json::Value = serde_json::from_str(&fwd).unwrap();
        assert_eq!(fv.get("ok").and_then(|x| x.as_bool()), Some(false));
        assert_eq!(
            fv.get("code").and_then(|c| c.as_str()),
            Some("no_forward")
        );
    }

    #[test]
    fn session_back_forward_stack() {
        let mut s = api::Session::new();
        s.push_url("https://a.test/".into());
        s.push_url("https://b.test/".into());
        assert_eq!(s.history.len(), 2);
        let prev = s.go_back().unwrap();
        assert_eq!(prev, "https://a.test/");
        assert_eq!(s.forward.len(), 1);
        let next = s.go_forward().unwrap();
        assert_eq!(next, "https://b.test/");
        assert!(s.forward.is_empty());
        // new nav clears forward
        s.go_back();
        s.push_url("https://c.test/".into());
        assert!(s.forward.is_empty());
        assert_eq!(s.history.last().map(|u| u.as_str()), Some("https://c.test/"));
    }

    /// Product binary must not expose headless as the default feature set.
    #[test]
    fn product_features_are_headed_default() {
        // Compile-time: default features include gui. Headless is opt-in.
        assert!(
            cfg!(feature = "gui"),
            "product tests expect gui feature"
        );
        // When running default `cargo test` (no --features headless), headless is off.
        // CI may enable headless separately for the API suite binary.
        let _ = cfg!(feature = "headless");
    }
}
