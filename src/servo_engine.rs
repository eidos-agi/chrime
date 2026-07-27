//! ServoEngine — the v1 engine (ADR 0001). Drives a headless Servo (SoftwareRenderingContext:
//! no window, no GPU) and reads the *post-JS* DOM by evaluating a walker script IN the page via
//! `evaluate_javascript` — in-process, not a debug protocol. Same `Engine` trait as StaticEngine,
//! so it's a drop-in that finally sees JavaScript-rendered content.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dpi::PhysicalSize;
use servo::{
    EventLoopWaker, JSValue, JavaScriptEvaluationError, LoadStatus, Opts, Preferences,
    RenderingContext, Servo, ServoBuilder, SoftwareRenderingContext, WebView, WebViewBuilder,
    WebViewDelegate,
};
use url::Url;

use crate::{normalize, session_store, DomNode, DomSnapshot, Engine, NavResult, SettleReceipt};

// Safety cap on the settle spin. A settle that hits this is reported `quiescent: false` —
// we never pretend a timeout was quiescence.
const SPIN_CAP: u32 = 30_000;

/// Where the cookie jar lives between runs. `CHRIME_PROFILE_DIR` overrides it (the suite gives
/// each run its own, so a stale jar can never make a persistence test pass).
///
/// It holds live session cookies — treat it like a credential store, never commit it.
/// Default is under `logs/`, which is git-ignored.
pub(crate) fn profile_dir() -> std::path::PathBuf {
    let dir = std::env::var("CHRIME_PROFILE_DIR").unwrap_or_else(|_| "logs/profile".into());
    let p = std::path::PathBuf::from(dir);
    let _ = std::fs::create_dir_all(&p);
    p
}

// Runs in the real page after JS, so it sees rendered content StaticEngine never could.
// Emits StaticEngine's exact schema so both engines are interchangeable behind the API.
const WALKER: &str = r#"(function(){
  const interesting=new Set(['a','button','input','textarea','select','h1','h2','h3','h4','h5','h6','p','li','label']);
  const role=t=>({a:'link',button:'button',input:'field',textarea:'field',select:'field',h1:'heading',h2:'heading',h3:'heading',h4:'heading',h5:'heading',h6:'heading',p:'text',li:'text',label:'label'}[t]||'generic');
  let id=0; const nodes=[];
  for(const el of document.querySelectorAll('*')){
    const tag=el.tagName.toLowerCase();
    if(!interesting.has(tag)) continue;
    const text=(el.textContent||'').replace(/\s+/g,' ').trim();
    const href=el.getAttribute('href');
    id++;
    const n={node_id:id,tag,role:role(tag),clickable:(tag==='a'||tag==='button')};
    if(text) n.text=text;
    if(href){ try{ n.href=new URL(href,document.baseURI).href }catch(e){ n.href=href } }
    nodes.push(n);
  }
  return JSON.stringify({url:document.location.href,title:document.title||null,node_count:nodes.length,nodes});
})()"#;

#[derive(Clone)]
struct Waker(Arc<AtomicBool>);
impl EventLoopWaker for Waker {
    fn clone_box(&self) -> Box<dyn EventLoopWaker> {
        Box::new(self.clone())
    }
    fn wake(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// The engine tells us when a load starts and finishes; we count the finishes.
///
/// Why a counter and not `webview.load_status()`: right after `load()` the webview still
/// reports `Complete` from the *previous* document, so a settle that trusts it returns
/// instantly and snapshots the old page (about:blank). A completion counter cannot lie —
/// the load we asked for is done only when the count moves past where it was.
#[derive(Default)]
struct Delegate {
    loads_completed: std::cell::Cell<u64>,
    loading: std::cell::Cell<bool>,
}

impl WebViewDelegate for Delegate {
    fn notify_load_status_changed(&self, _webview: WebView, status: LoadStatus) {
        match status {
            LoadStatus::Started => self.loading.set(true),
            LoadStatus::Complete => {
                self.loading.set(false);
                self.loads_completed.set(self.loads_completed.get() + 1);
            }
            _ => {}
        }
    }
}

pub struct ServoEngine {
    servo: Servo,
    #[allow(dead_code)]
    rendering_context: Rc<dyn RenderingContext>,
    webview: WebView,
    delegate: Rc<Delegate>,
    url: Option<Url>,
}

impl ServoEngine {
    pub fn new() -> Self {
        // Servo's rustls needs a process-default crypto provider or its ResourceManager panics.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let rendering_context = Rc::new(
            SoftwareRenderingContext::new(PhysicalSize {
                width: 1024,
                height: 768,
            })
            .expect("SoftwareRenderingContext"),
        );
        let _ = rendering_context.make_current();

        let mut preferences = Preferences::default();
        preferences.network_http_proxy_uri = String::new();
        preferences.network_https_proxy_uri = String::new();

        // The profile dir IS the session that survives a restart: Servo reads `cookie_jar.json`
        // (plus hsts/auth caches) from it at startup and writes them back when the resource
        // thread exits. Without it every process starts logged out.
        let opts = Opts {
            config_dir: Some(profile_dir()),
            ..Default::default()
        };

        let servo = ServoBuilder::default()
            .opts(opts)
            .preferences(preferences)
            .event_loop_waker(Box::new(Waker(Arc::new(AtomicBool::new(false)))))
            .build();
        // Opt-in engine logging: `RUST_LOG=warn chrime --api --engine servo`. Off by default so
        // the JSONL API stays clean; on, it is the only way to see why a load failed.
        if std::env::var_os("RUST_LOG").is_some() {
            servo.setup_logging();
        }

        let delegate = Rc::new(Delegate::default());
        let webview = WebViewBuilder::new(&servo, rendering_context.clone())
            .delegate(delegate.clone())
            .url(Url::parse("about:blank").unwrap())
            .build();
        // Servo only drives a webview that is shown+focused (servoshell does this on activate);
        // an unshown webview accepts a load and never runs it.
        webview.show();
        webview.focus();
        // Pump until the initial about:blank load completes. Until then the constellation has no
        // browsing context for this webview and drops navigations on the floor
        // ("LoadUrl for unknown browsing context") — a load that looks accepted and never runs.
        let mut n = 0;
        while delegate.loads_completed.get() == 0 && n < SPIN_CAP {
            servo.spin_event_loop();
            std::thread::sleep(Duration::from_millis(1));
            n += 1;
        }

        ServoEngine {
            servo,
            rendering_context,
            webview,
            delegate,
            url: None,
        }
    }

    // Pump Servo's event loop until `cond` is false (or the safety cap). This IS the settle:
    // no live compositor loop, we drive the engine to quiescence on demand. Returns the number
    // of turns pumped — that count is what makes the settle receipt evidence, not a claim.
    fn spin(&self, cond: impl Fn() -> bool) -> u32 {
        let mut n = 0;
        while cond() && n < SPIN_CAP {
            self.servo.spin_event_loop();
            std::thread::sleep(Duration::from_millis(1));
            n += 1;
        }
        n
    }

    // Evaluate JS in the page; return the string result (walker/read return JSON or text).
    fn eval(&self, script: &str) -> Option<String> {
        let saved: Rc<RefCell<Option<Result<JSValue, JavaScriptEvaluationError>>>> =
            Rc::new(RefCell::new(None));
        let cb = saved.clone();
        self.webview
            .evaluate_javascript(script, move |r| *cb.borrow_mut() = Some(r));
        let s = saved.clone();
        self.spin(move || s.borrow().is_none());
        let result = saved.borrow().clone(); // bind first so the Ref drops before the match
        match result {
            Some(Ok(JSValue::String(s))) => Some(s),
            _ => None,
        }
    }

    // Drive the event loop until the load that was in flight completes. `since` is the
    // completion count taken *before* the load was kicked off. Returns (spins, quiescent).
    fn wait_for_load(&self, since: u64) -> (u32, bool) {
        let d = self.delegate.clone();
        let n = self.spin(move || d.loads_completed.get() <= since || d.loading.get());
        (
            n,
            self.loads_completed() > since && !self.delegate.loading.get(),
        )
    }

    fn loads_completed(&self) -> u64 {
        self.delegate.loads_completed.get()
    }
}

impl Engine for ServoEngine {
    fn engine_name(&self) -> &'static str {
        "servo"
    }

    fn profile_dir(&self) -> Option<String> {
        Some(profile_dir().display().to_string())
    }

    // The real settle: if a load is in flight, pump the engine's own event loop until IT says
    // the load completed, and report how many turns that took. Not a sleep, not a poll from
    // outside the loop. If nothing is in flight the page is already past load-complete and the
    // receipt says so rather than inventing work.
    fn settle(&mut self) -> SettleReceipt {
        let t0 = Instant::now();
        let in_flight = self.delegate.loading.get();
        let (spins, quiescent) = if in_flight {
            self.wait_for_load(self.loads_completed())
        } else {
            // One turn so anything the last op queued (timers, microtasks) drains first.
            self.servo.spin_event_loop();
            (1, true)
        };
        SettleReceipt {
            ok: true,
            engine: "servo",
            spins,
            ms: t0.elapsed().as_millis() as u64,
            quiescent,
            reason: match (in_flight, quiescent) {
                (_, false) => "cap",
                (true, true) => "load_complete",
                (false, true) => "already_complete",
            },
            url: self.current_url(),
        }
    }

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
        // Snapshot the completion count *before* kicking the load off, so the wait can only be
        // satisfied by THIS load finishing — never by the previous document's stale Complete.
        let before = self.loads_completed();
        self.webview.load(target.clone());
        let (_, ok) = self.wait_for_load(before);
        if !ok {
            return NavResult {
                ok: false,
                url: Some(target.to_string()),
                status: None,
                title: None,
                error: Some("navigation did not complete before the settle cap".into()),
                content_type: None,
                content_kind: None,
            };
        }
        self.url = Some(target.clone());
        let title = self.eval("document.title").filter(|s| !s.is_empty());
        NavResult {
            ok: true,
            url: Some(target.to_string()),
            status: None,
            title,
            error: None,
            content_type: Some("text/html".into()),
            content_kind: Some("html"),
        }
    }

    fn snapshot(&self) -> DomSnapshot {
        self.eval(WALKER)
            .and_then(|j| serde_json::from_str::<DomSnapshot>(&j).ok())
            .unwrap_or_else(|| DomSnapshot {
                view: "full".into(),
                url: self.url.as_ref().map(|u| u.to_string()),
                title: None,
                node_count: 0,
                html_bytes: None,
                counts: None,
                nodes: vec![],
            })
    }

    fn read_text(&self) -> String {
        self.eval("document.body ? document.body.innerText : ''")
            .unwrap_or_default()
    }

    fn click(&mut self, node_id: u32) -> NavResult {
        // Click the node_id-th interesting element (same order as the walker), then settle —
        // this runs the page's real JS click handlers, unlike the static engine.
        let script = format!(
            r#"(function(){{
              const interesting=new Set(['a','button','input','textarea','select','h1','h2','h3','h4','h5','h6','p','li','label']);
              let i=0; for(const el of document.querySelectorAll('*')){{
                if(!interesting.has(el.tagName.toLowerCase())) continue; i++;
                if(i==={node_id}){{ el.click(); break; }}
              }}
              return 'ok';
            }})()"#
        );
        let before = self.loads_completed();
        self.eval(&script);
        // The handler may or may not navigate. Only wait when the engine says a load started —
        // waiting unconditionally would hang on a click that just mutates the DOM.
        if self.delegate.loading.get() {
            let _ = self.wait_for_load(before);
        }
        let now = self.eval("document.location.href");
        if let Some(u) = &now {
            if let Ok(parsed) = Url::parse(u) {
                self.url = Some(parsed);
            }
        }
        NavResult {
            ok: true,
            url: now,
            status: None,
            title: self.eval("document.title").filter(|s| !s.is_empty()),
            error: None,
            content_type: None,
            content_kind: Some("html"),
        }
    }

    fn current_url(&self) -> Option<String> {
        self.url.as_ref().map(|u| u.to_string())
    }

    fn links(&self) -> Vec<DomNode> {
        self.snapshot()
            .nodes
            .into_iter()
            .filter(|n| n.clickable && n.href.is_some())
            .collect()
    }

    // The stored buffer for a live engine is the *serialized post-JS DOM* — the only page body
    // we keep, same single-buffer rule as StaticEngine.
    fn html_bytes(&self) -> usize {
        self.eval("''+document.documentElement.outerHTML.length")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }

    fn export_page(&self) -> session_store::SavedPage {
        session_store::SavedPage {
            url: self.current_url(),
            title: self.eval("document.title").filter(|s| !s.is_empty()),
            html: self
                .eval("document.documentElement.outerHTML")
                .unwrap_or_default(),
        }
    }

    // Shim a saved buffer back in without the network. Servo has no "set document" API, so the
    // buffer round-trips through a temp file:// load — still zero network, still the same DOM.
    // ponytail: temp file over a data: URL because top-level data: navigation is commonly
    // blocked; swap if Servo ever exposes a direct document-set.
    fn import_page(&mut self, page: &session_store::SavedPage) -> Result<(), String> {
        let path = std::env::temp_dir().join(format!("chrime-shim-{}.html", std::process::id()));
        std::fs::write(&path, &page.html).map_err(|e| format!("shim write failed: {e}"))?;
        let file_url = Url::from_file_path(&path).map_err(|_| "shim: bad temp path".to_string())?;
        let before = self.loads_completed();
        self.webview.load(file_url);
        let _ = self.wait_for_load(before);
        // Lineage is the *saved* url, not the temp file — an agent asking "where am I" must see
        // the page it shimmed, not our scratch path.
        self.url = page.url.as_deref().and_then(|u| Url::parse(u).ok());
        Ok(())
    }

    fn find_text(&self, q: &str) -> Vec<DomNode> {
        let ql = q.to_lowercase();
        self.snapshot()
            .nodes
            .into_iter()
            .filter(|n| !ql.is_empty() && n.text.to_lowercase().contains(&ql))
            .collect()
    }
}
