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
use serde::Serialize;
use std::io::{BufRead, Write};
use url::{form_urlencoded, Url};

const UA: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrime/0.1 (a browser for agents)";

// ---- the DOM, as an agent sees it ----

#[derive(Serialize)]
struct DomNode {
    node_id: u32,
    tag: String,
    role: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    href: Option<String>,
    clickable: bool,
}

#[derive(Serialize)]
struct DomSnapshot {
    url: Option<String>,
    title: Option<String>,
    node_count: usize,
    nodes: Vec<DomNode>,
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
}

// ---- the swappable engine seam (StaticEngine now; a v8 engine implements this next) ----

trait Engine {
    fn navigate(&mut self, url: &str) -> NavResult;
    fn snapshot(&self) -> DomSnapshot;
    fn read_text(&self) -> String;
    fn click(&mut self, node_id: u32) -> NavResult;
    fn current_url(&self) -> Option<String>;
    /// Every clickable link on the page, with its node-id and resolved href.
    fn links(&self) -> Vec<DomNode>;
    /// Nodes whose text contains `q` (case-insensitive) — how an agent finds "the login button".
    fn find_text(&self, q: &str) -> Vec<DomNode>;
}

struct StaticEngine {
    url: Option<Url>,
    html: String,
    title: Option<String>,
    agent: ureq::Agent,
}

impl StaticEngine {
    fn new() -> Self {
        StaticEngine {
            url: None,
            html: String::new(),
            title: None,
            agent: ureq::AgentBuilder::new().redirects(5).build(),
        }
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
            if tag == "title" {
                if let Some(er) = ElementRef::wrap(node) {
                    let t = collapse(&er.text().collect::<String>());
                    if !t.is_empty() {
                        title = Some(t);
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
                }
            }
        };
        match self.agent.get(target.as_str()).set("User-Agent", UA).call() {
            Ok(resp) => {
                let status = resp.status();
                self.html = resp.into_string().unwrap_or_default();
                self.url = Some(target.clone());
                let (_, title) = self.walk();
                self.title = title.clone();
                NavResult {
                    ok: true,
                    url: Some(target.to_string()),
                    status: Some(status),
                    title,
                    error: None,
                }
            }
            Err(e) => NavResult {
                ok: false,
                url: Some(target.to_string()),
                status: None,
                title: None,
                error: Some(e.to_string()),
            },
        }
    }

    fn snapshot(&self) -> DomSnapshot {
        let (nodes, title) = self.walk();
        DomSnapshot {
            url: self.current_url(),
            title,
            node_count: nodes.len(),
            nodes,
        }
    }

    fn read_text(&self) -> String {
        let doc = Html::parse_document(&self.html);
        let sel = Selector::parse("body").unwrap();
        match doc.select(&sel).next() {
            Some(body) => collapse(&body.text().collect::<String>()),
            None => collapse(&doc.root_element().text().collect::<String>()),
        }
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
                },
            },
            None => NavResult {
                ok: false,
                url: self.current_url(),
                status: None,
                title: None,
                error: Some(format!("no node with id {}", node_id)),
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
}

fn interesting(tag: &str) -> bool {
    matches!(
        tag,
        "a" | "button" | "input" | "textarea" | "select" | "h1" | "h2" | "h3" | "h4" | "h5"
            | "h6" | "p" | "li" | "label"
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
        Some(b) => b.join(href).map(|u| u.to_string()).unwrap_or_else(|_| href.to_string()),
        None => href.to_string(),
    }
}

fn normalize(raw: &str, base: Option<&Url>) -> Result<Url, String> {
    let s = raw.trim();
    if let Ok(u) = Url::parse(s) {
        if u.scheme() == "http" || u.scheme() == "https" {
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

// ---- 100%-API surface: JSON commands in, JSON results out ----

fn handle(eng: &mut dyn Engine, line: &str) -> String {
    let v: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return err("bad_json", &format!("bad json: {}", e)),
    };
    match v.get("op").and_then(|o| o.as_str()).unwrap_or("") {
        "navigate" => {
            let url = v.get("url").and_then(|u| u.as_str()).unwrap_or("");
            serde_json::to_string(&eng.navigate(url)).unwrap()
        }
        "snapshot" => serde_json::to_string(&eng.snapshot()).unwrap(),
        "read" => serde_json::json!({ "text": eng.read_text() }).to_string(),
        "links" => serde_json::to_string(&eng.links()).unwrap(),
        "find_text" => {
            let q = v.get("text").and_then(|t| t.as_str()).unwrap_or("");
            serde_json::to_string(&eng.find_text(q)).unwrap()
        }
        "click" => {
            let id = v.get("node_id").and_then(|n| n.as_u64()).unwrap_or(0) as u32;
            serde_json::to_string(&eng.click(id)).unwrap()
        }
        "current" => serde_json::json!({ "url": eng.current_url() }).to_string(),
        "quit" => std::process::exit(0),
        other => err("unknown_op", &format!("unknown op '{}'", other)),
    }
}

fn err(code: &str, msg: &str) -> String {
    serde_json::json!({ "ok": false, "code": code, "error": msg }).to_string()
}

// ---- the HEAD: a terminal render of the semantic DOM (default when a human runs chrime) ----

fn term_width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|c| c.trim().parse::<usize>().ok())
        .filter(|&n| n >= 20)
        .unwrap_or(88)
        .min(100)
}

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
                    if n.text.is_empty() { "(no text)" } else { &n.text }
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
    println!(
        "\x1b[38;5;244m URL to go · number to open a link · b back · r read · q quit\x1b[0m"
    );
    println!(
        "\x1b[38;5;240m 🎭 a member of the Fraude family — the fraud that does real work\x1b[0m"
    );
    clickmap
}

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

fn run_api(eng: &mut dyn Engine) {
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
        writeln!(out, "{}", handle(eng, line)).ok();
        out.flush().ok();
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-v") {
        println!("chrime {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    // Headed by default. Agents opt into the raw JSON API with --api (alias --headless).
    let api = args.iter().any(|a| a == "--api" || a == "--headless");
    let start = args.iter().find(|a| !a.starts_with("--")).cloned();
    let mut eng = StaticEngine::new();
    if api {
        run_api(&mut eng);
    } else {
        headed(&mut eng, start);
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
        assert!(nodes.iter().any(|n| n.role == "heading" && n.text == "Head"));
        let link = nodes.iter().find(|n| n.role == "link").unwrap();
        assert_eq!(link.href.as_deref(), Some("https://ex.com/x"));
        assert!(link.clickable);
        assert_eq!(e.links().len(), 1);
        assert_eq!(e.find_text("para").len(), 1);
        assert_eq!(e.find_text("nope").len(), 0);
    }
}
