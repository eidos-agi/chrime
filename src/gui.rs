//! Dual-pane desktop app: live WebKit render (left) + Chrime semantic DOM (right).
//!
//! The left pane is a real system WebView so you can *see* the page (CNN, etc.).
//! The right pane is Chrime's agent view — the same settle-and-snapshot tree as the TUI/API.
//! Address bar drives both; clicks on numbered links on the right navigate both.
//!
//! **AI visibility mode** paints Set-of-Mark boxes + numbers on every live clickable in the
//! left pane — so a screenshot-using agent (or a human debugging one) can see what is worth
//! clicking, and draw/refer to those marks.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};
use wry::{
    dpi::{LogicalPosition, LogicalSize},
    http::Request,
    NewWindowResponse, PageLoadEvent, Rect, WebView, WebViewBuilder,
};

use crate::api::{self, ApiCmd, LiveSurface, Session};
use crate::knox::{self, KnoxMatch};
use crate::views::ViewKind;
use crate::{normalize, Engine, StaticEngine};

const CHROME_H: u32 = 52;
/// Live page share of the split (not 50% — half-width forces mobile/"vertical" site layouts).
const DEFAULT_PAGE_RATIO: f64 = 0.68;
/// Auto mode chooses side-by-side only when the window is this wide (and landscape).
const AUTO_SIDE_MIN_WIDTH: u32 = 1100;

/// Dual-pane geometry. Default `Auto` prefers a **wide page** (side) on desktop monitors
/// instead of a permanent 50/50 phone column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneMode {
    /// Side when wide+landscape; stack when narrow/portrait.
    Auto,
    /// Page | agent (side-by-side) — desktop / horizontal reading.
    Side,
    /// Page / agent (stacked) — narrow windows; page still gets majority height.
    Stack,
}

impl PaneMode {
    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(PaneMode::Auto),
            "side" | "horizontal" | "landscape" | "lr" | "left-right" => Some(PaneMode::Side),
            "stack" | "vertical" | "portrait" | "tb" | "top-bottom" => Some(PaneMode::Stack),
            _ => None,
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            PaneMode::Auto => "auto",
            PaneMode::Side => "side",
            PaneMode::Stack => "stack",
        }
    }
    fn cycle(self) -> Self {
        match self {
            PaneMode::Auto => PaneMode::Side,
            PaneMode::Side => PaneMode::Stack,
            PaneMode::Stack => PaneMode::Auto,
        }
    }
    fn label(self) -> &'static str {
        match self {
            PaneMode::Auto => "Layout · auto",
            PaneMode::Side => "Layout · side",
            PaneMode::Stack => "Layout · stack",
        }
    }
}

/// Product rule: **no pop-ups for Chrime features.**
/// Features are always-visible chrome toggles or JSON ops. Never a settings modal,
/// never a multi-step dialog a human must navigate to flip a switch.
///
/// Web-originated UI that blocks an agent is also suppressed:
/// - alert / confirm / prompt / print / showModalDialog → no-ops (confirm auto-true)
/// - window.open → same-tab navigation (or ignored)
/// - target=_blank new-window requests → denied; URL loads in the main left pane
/// - downloads → denied (no save-file dialog)
///
/// Exception: OS-level Knox Touch ID is Knox's unlock boundary for *secrets*, not a
/// Chrime feature toggle. Chrime itself never invents a modal to change a feature.
///
/// `NO_POPUPS_JS` below runs at document start on every page load — it kills the human-modal
/// web APIs listed above.
const NO_POPUPS_JS: &str = r#"(function(){
  if (window.__chrimeNoPopups) return;
  window.__chrimeNoPopups = true;
  var noop = function(){};
  try { window.alert = noop; } catch(e){}
  try { window.confirm = function(){ return true; }; } catch(e){}
  try { window.prompt = function(){ return null; }; } catch(e){}
  try { window.print = noop; } catch(e){}
  try { window.showModalDialog = function(){ return null; }; } catch(e){}
  try {
    window.open = function(url){
      if (url && typeof url === 'string' && url !== 'about:blank' && url !== '') {
        try { window.location.assign(url); } catch(e) {
          try { window.location.href = url; } catch(e2){}
        }
      }
      return null;
    };
  } catch(e){}
  // target=_blank without opener still tries new windows; we also deny at the host layer.
  try {
    document.addEventListener('click', function(ev){
      var a = ev.target && ev.target.closest && ev.target.closest('a[target="_blank"]');
      if (!a) return;
      var href = a.href;
      if (!href) return;
      ev.preventDefault();
      ev.stopPropagation();
      window.location.assign(href);
    }, true);
  } catch(e){}
})()"#;

/// Injected into the live page: draw numbered boxes on clickable targets (Set-of-Marks).
/// Re-runs on scroll/resize/DOM mutation. pointer-events:none so the page stays usable.
const AI_VIS_ON_JS: &str = r#"(function(){
  if (window.__chrimeAiVis && window.__chrimeAiVis.alive) {
    window.__chrimeAiVis.paint();
    return 'already-on';
  }
  const ROOT_ID = 'chrime-ai-vis-root';
  const STYLE_ID = 'chrime-ai-vis-style';
  const MAX = 150;
  const SEL = [
    'a[href]',
    'button',
    'input:not([type="hidden"])',
    'select',
    'textarea',
    'summary',
    '[role="button"]',
    '[role="link"]',
    '[role="tab"]',
    '[role="menuitem"]',
    '[role="checkbox"]',
    '[role="radio"]',
    '[role="switch"]',
    '[onclick]',
    '[tabindex]:not([tabindex="-1"])',
    'label[for]'
  ].join(',');

  function ensureStyle(){
    if (document.getElementById(STYLE_ID)) return;
    const s = document.createElement('style');
    s.id = STYLE_ID;
    s.textContent = `
      #${ROOT_ID}{position:fixed;inset:0;z-index:2147483646;pointer-events:none;overflow:hidden}
      #${ROOT_ID} .c-box{
        position:fixed;border:2px solid #ff9f0a;border-radius:0;
        background:rgba(255,159,10,0.10);box-sizing:border-box;
      }
      #${ROOT_ID} .c-num{
        position:fixed;min-width:18px;height:16px;padding:0 4px;
        background:#ff9f0a;color:#1c1c1e;font:700 11px/16px -apple-system,BlinkMacSystemFont,sans-serif;
        border-radius:0;text-align:center;box-shadow:0 1px 3px rgba(0,0,0,.45);
        white-space:nowrap;
      }
      #${ROOT_ID} .c-hud{
        position:fixed;top:8px;left:8px;z-index:1;
        background:rgba(28,28,30,.92);color:#ff9f0a;
        font:600 11px/1.2 -apple-system,BlinkMacSystemFont,sans-serif;
        padding:6px 10px;border-radius:0;border:1px solid #ff9f0a;
        max-width:70vw;
      }
    `;
    document.documentElement.appendChild(s);
  }

  function root(){
    let r = document.getElementById(ROOT_ID);
    if (!r){
      r = document.createElement('div');
      r.id = ROOT_ID;
      (document.body || document.documentElement).appendChild(r);
    }
    return r;
  }

  function visible(el){
    if (!el || el.closest('#' + ROOT_ID)) return false;
    const st = window.getComputedStyle(el);
    if (st.display === 'none' || st.visibility === 'hidden' || Number(st.opacity) === 0) return false;
    const r = el.getBoundingClientRect();
    if (r.width < 6 || r.height < 6) return false;
    if (r.bottom < 0 || r.right < 0 || r.top > innerHeight || r.left > innerWidth) return false;
    return true;
  }

  function candidates(){
    const all = Array.from(document.querySelectorAll(SEL));
    // Prefer leaf-ish targets: drop containers that only wrap another match.
    const out = [];
    for (const el of all){
      if (!visible(el)) continue;
      // skip if a more specific child is also a candidate and fills most of the box
      let covered = false;
      for (const child of el.querySelectorAll(SEL)){
        if (child === el || !visible(child)) continue;
        const a = el.getBoundingClientRect();
        const b = child.getBoundingClientRect();
        const overlap = Math.max(0, Math.min(a.right,b.right) - Math.max(a.left,b.left))
                      * Math.max(0, Math.min(a.bottom,b.bottom) - Math.max(a.top,b.top));
        const area = Math.max(1, a.width * a.height);
        if (overlap / area > 0.85){ covered = true; break; }
      }
      if (!covered) out.push(el);
    }
    // Largest first, then cap — agents care about big targets first but keep reading order.
    out.sort((a,b)=>{
      const ra = a.getBoundingClientRect(), rb = b.getBoundingClientRect();
      if (Math.abs(ra.top - rb.top) > 12) return ra.top - rb.top;
      return ra.left - rb.left;
    });
    return out.slice(0, MAX);
  }

  function paint(){
    ensureStyle();
    const r = root();
    r.innerHTML = '';
    const els = candidates();
    const hud = document.createElement('div');
    hud.className = 'c-hud';
    hud.textContent = 'AI vis · ' + els.length + ' click targets (screenshot marks)';
    r.appendChild(hud);
    const marks = [];
    els.forEach((el, i)=>{
      const n = i + 1;
      const rect = el.getBoundingClientRect();
      const box = document.createElement('div');
      box.className = 'c-box';
      box.style.left = rect.left + 'px';
      box.style.top = rect.top + 'px';
      box.style.width = rect.width + 'px';
      box.style.height = rect.height + 'px';
      const badge = document.createElement('div');
      badge.className = 'c-num';
      badge.textContent = String(n);
      let bx = rect.left - 2;
      let by = rect.top - 16;
      if (by < 0) by = rect.top + 2;
      if (bx < 0) bx = rect.left + 2;
      badge.style.left = bx + 'px';
      badge.style.top = by + 'px';
      r.appendChild(box);
      r.appendChild(badge);
      const label = (el.innerText || el.getAttribute('aria-label') || el.getAttribute('title')
        || el.getAttribute('placeholder') || el.getAttribute('name') || el.tagName || '')
        .replace(/\s+/g,' ').trim().slice(0, 80);
      marks.push({
        n: n,
        tag: (el.tagName || '').toLowerCase(),
        role: el.getAttribute('role') || '',
        text: label,
        href: el.href || el.getAttribute('href') || null,
        rect: {x: rect.left, y: rect.top, w: rect.width, h: rect.height,
               cx: rect.left + rect.width/2, cy: rect.top + rect.height/2}
      });
    });
    window.__chrimeAiVis.marks = marks;
    try {
      if (window.ipc && window.ipc.postMessage) {
        window.ipc.postMessage(JSON.stringify({op:'ai_marks', count: marks.length, marks: marks}));
      }
    } catch (e) {}
    return marks.length;
  }

  let t = null;
  function schedule(){
    if (t) cancelAnimationFrame(t);
    t = requestAnimationFrame(()=>{ t = null; paint(); });
  }

  const mo = new MutationObserver(schedule);
  mo.observe(document.documentElement, {childList:true, subtree:true, attributes:true});
  window.addEventListener('scroll', schedule, true);
  window.addEventListener('resize', schedule);
  window.__chrimeAiVis = { alive: true, paint, schedule, mo, marks: [], off: function(){
    this.alive = false;
    try { this.mo.disconnect(); } catch(e) {}
    window.removeEventListener('scroll', schedule, true);
    window.removeEventListener('resize', schedule);
    const r = document.getElementById(ROOT_ID);
    if (r) r.remove();
    const s = document.getElementById(STYLE_ID);
    if (s) s.remove();
    window.__chrimeAiVis = null;
  }};
  paint();
  // late SPA content
  setTimeout(paint, 400);
  setTimeout(paint, 1200);
  return 'on';
})()"#;

const AI_VIS_OFF_JS: &str = r#"(function(){
  if (window.__chrimeAiVis && window.__chrimeAiVis.off) window.__chrimeAiVis.off();
  else {
    const r = document.getElementById('chrime-ai-vis-root');
    if (r) r.remove();
    const s = document.getElementById('chrime-ai-vis-style');
    if (s) s.remove();
  }
  return 'off';
})()"#;

enum Msg {
    Navigate(String),
    Click(usize),
    PageLoaded(String),
    Back,
    Read,
    ToggleAiVis,
    /// Set AI visibility directly — never a settings dialog.
    SetAiVis(bool),
    /// Live-page mark inventory (optional; from injected script).
    AiMarks {
        count: usize,
    },
    /// Search Knox using current host (or explicit query). Metadata only.
    KnoxFind {
        query: Option<String>,
    },
    /// Unlock + inject login and/or password into the live page. Secrets never surface.
    KnoxFill {
        query: String,
        id: Option<String>,
        /// "login" | "password" | "both"
        fields: String,
    },
    /// JSONL API from the local listener — full drive without human clicks.
    Api(ApiCmd),
    /// Switch right-pane projection of the *same* page (no second page store).
    SetView(ViewKind),
    /// Cycle dual-pane geometry (auto → side → stack).
    CycleLayout,
    /// Collapse / expand the agent DOM sidebar.
    ToggleSidebar,
    /// Run a full API JSON line on the GUI session (Hancock, knox_fill, etc.).
    ApiLine(String),
}

/// Dual-pane app. Optional `listen` (default `127.0.0.1:7420`) exposes the full JSONL API
/// so agents operate the window with zero mouse/keyboard input.
pub fn run(start: Option<String>, listen: Option<String>) -> wry::Result<()> {
    let (tx, rx) = mpsc::channel::<Msg>();
    let mut eng = StaticEngine::new();
    let mut history: Vec<String> = Vec::new();

    if let Some(raw) = start {
        if let Ok(u) = normalize(&raw, None) {
            let r = eng.navigate(u.as_str());
            if let (true, Some(url)) = (r.ok, r.url) {
                history.push(url);
            }
        }
    }

    if let Some(addr) = listen.as_ref() {
        let (cmd_tx, cmd_rx) = mpsc::channel::<ApiCmd>();
        match api::spawn_listener(addr, cmd_tx) {
            Ok(_) => {
                let api_tx = tx.clone();
                thread::spawn(move || {
                    while let Ok(cmd) = cmd_rx.recv() {
                        if api_tx.send(Msg::Api(cmd)).is_err() {
                            break;
                        }
                    }
                });
            }
            Err(e) => eprintln!("chrime: api listen failed ({e}) — GUI only"),
        }
    }

    let event_loop = EventLoop::new().expect("event loop");
    let mut app = App {
        window: None,
        chrome: None,
        page: None,
        side: None,
        eng,
        history,
        clickmap: Vec::new(),
        tx,
        rx,
        suppress_page_load: false,
        ai_vis: true,
        mark_count: 0,
        knox_matches: Vec::new(),
        knox_status: None,
        knox_query: String::new(),
        api_session: Session::new(),
        view_kind: ViewKind::Full,
        pane_mode: PaneMode::Auto,
        page_ratio: DEFAULT_PAGE_RATIO,
        sidebar_visible: true,
    };
    if let Some(u) = app.eng.current_url() {
        app.api_session.history.push(u);
    }
    app.api_session.ai_vis = app.ai_vis;
    event_loop.run_app(&mut app).expect("run app");
    Ok(())
}

struct App {
    window: Option<Window>,
    chrome: Option<WebView>,
    page: Option<WebView>,
    side: Option<WebView>,
    eng: StaticEngine,
    history: Vec<String>,
    clickmap: Vec<u32>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    /// When we drive navigation ourselves, ignore the echo PageLoaded once.
    suppress_page_load: bool,
    /// Paint Set-of-Mark boxes on the live left pane.
    ai_vis: bool,
    mark_count: usize,
    knox_matches: Vec<KnoxMatch>,
    knox_status: Option<String>,
    knox_query: String,
    api_session: Session,
    /// Right-pane projection of the single stored page (enum only — no cached node lists).
    view_kind: ViewKind,
    /// Dual-pane geometry preference (auto adapts; never stuck at 50/50 phone column).
    pane_mode: PaneMode,
    /// Fraction of the split given to the live page (0.45–0.85). Default 0.68.
    page_ratio: f64,
    /// Agent DOM sidebar. When false, live page fills the full body under chrome.
    sidebar_visible: bool,
}

impl LiveSurface for App {
    fn eval_js(&mut self, js: &str) -> Result<(), String> {
        let page = self
            .page
            .as_ref()
            .ok_or_else(|| "no page webview".to_string())?;
        page.evaluate_script(js)
            .map_err(|e| format!("evaluate_script: {e}"))
    }

    fn eval_js_result(&mut self, js: &str) -> Result<String, String> {
        let page = self
            .page
            .as_ref()
            .ok_or_else(|| "no page webview".to_string())?;
        // Expression form: result is JSON-serialized into the callback (wry 0.55).
        let (tx, rx) = std::sync::mpsc::sync_channel::<String>(1);
        page.evaluate_script_with_callback(js, move |result| {
            let _ = tx.send(result);
        })
        .map_err(|e| format!("evaluate_script_with_callback: {e}"))?;
        // On macOS WKWebView the callback is usually invoked before eval returns.
        // Timeout protects against rare async delivery without deadlocking forever.
        rx.recv_timeout(std::time::Duration::from_secs(8))
            .map_err(|_| {
                "live eval timed out waiting for WebView callback (8s) — page may still be loading"
                    .into()
            })
    }

    fn set_ai_vis(&mut self, on: bool) {
        self.apply_ai_vis_flag(on);
    }

    fn ai_vis(&self) -> bool {
        self.ai_vis
    }

    fn mark_count(&self) -> usize {
        self.mark_count
    }

    fn layout_info(&self) -> Option<serde_json::Value> {
        Some(self.layout_report_json())
    }

    fn set_pane_layout(
        &mut self,
        mode: Option<&str>,
        page_ratio: Option<f64>,
    ) -> Result<serde_json::Value, String> {
        if let Some(m) = mode {
            self.pane_mode = PaneMode::parse(m).ok_or_else(|| {
                format!("unknown layout mode `{m}` — use auto|side|stack")
            })?;
        }
        if let Some(r) = page_ratio {
            if !(0.45..=0.85).contains(&r) {
                return Err("page_ratio must be between 0.45 and 0.85".into());
            }
            self.page_ratio = r;
        }
        self.apply_bounds();
        self.refresh_chrome();
        // AI marks re-measure after geometry change.
        self.apply_ai_vis();
        Ok(self.layout_report_json())
    }

    fn cycle_pane_layout(&mut self) -> Result<serde_json::Value, String> {
        self.pane_mode = self.pane_mode.cycle();
        self.apply_bounds();
        self.refresh_chrome();
        self.apply_ai_vis();
        Ok(self.layout_report_json())
    }

    fn set_sidebar_visible(&mut self, visible: bool) -> Result<serde_json::Value, String> {
        self.sidebar_visible = visible;
        self.apply_bounds();
        self.refresh_chrome();
        self.apply_ai_vis();
        Ok(self.layout_report_json())
    }

    fn toggle_sidebar(&mut self) -> Result<serde_json::Value, String> {
        self.set_sidebar_visible(!self.sidebar_visible)
    }
}

impl App {
    fn window_logical_size(&self) -> Option<(u32, u32)> {
        let window = self.window.as_ref()?;
        let size = window.inner_size().to_logical::<u32>(window.scale_factor());
        Some((size.width.max(2), size.height.max(CHROME_H + 2)))
    }

    fn effective_mode(&self, w: u32, h: u32) -> PaneMode {
        match self.pane_mode {
            PaneMode::Auto => {
                // Wide landscape → side-by-side (desktop page width). Narrow/portrait → stack
                // so the page is still full width instead of a skinny half column.
                if w >= AUTO_SIDE_MIN_WIDTH && w >= h {
                    PaneMode::Side
                } else {
                    PaneMode::Stack
                }
            }
            other => other,
        }
    }

    fn layout_report_json(&self) -> serde_json::Value {
        let (w, h) = self.window_logical_size().unwrap_or((0, 0));
        let effective = self.effective_mode(w, h);
        let body_h = h.saturating_sub(CHROME_H);
        let ratio = self.page_ratio.clamp(0.45, 0.85);
        let (page_w, page_h) = if !self.sidebar_visible {
            (w, body_h)
        } else {
            match effective {
                PaneMode::Side => {
                    let pw = ((w as f64) * ratio).round() as u32;
                    let pw = pw.max(320).min(w.saturating_sub(280).max(320));
                    (pw.min(w), body_h)
                }
                PaneMode::Stack | PaneMode::Auto => {
                    let ph = ((body_h as f64) * ratio).round() as u32;
                    let ph = ph.max(200).min(body_h.saturating_sub(160).max(200));
                    (w, ph.min(body_h))
                }
            }
        };
        let share_pct = if !self.sidebar_visible {
            100.0
        } else {
            ratio * 100.0
        };
        serde_json::json!({
            "ok": true,
            "action": "layout",
            "layout_mode": self.pane_mode.as_str(),
            "layout_effective": if self.sidebar_visible { effective.as_str() } else { "page-only" },
            "page_ratio": ratio,
            "sidebar_visible": self.sidebar_visible,
            "window_width": w,
            "window_height": h,
            "page_width": page_w,
            "page_height": page_h,
            "english": format!(
                "Layout {} (effective {}): live page {}×{} · sidebar {} · {:.0}% page share.",
                self.pane_mode.as_str(),
                if self.sidebar_visible { effective.as_str() } else { "page-only" },
                page_w,
                page_h,
                if self.sidebar_visible { "open" } else { "collapsed" },
                share_pct
            ),
        })
    }

    fn layout(&self) -> Option<(Rect, Rect, Rect)> {
        let (w, h) = self.window_logical_size()?;
        let body_h = h - CHROME_H;
        let ratio = self.page_ratio.clamp(0.45, 0.85);
        let effective = self.effective_mode(w, h);
        let chrome = Rect {
            position: LogicalPosition::new(0, 0).into(),
            size: LogicalSize::new(w, CHROME_H).into(),
        };
        // Collapsed sidebar: live page owns the entire body; side webview is 1×1 off-corner
        // (wry has no hide API — zero-area bounds remove it from the visual surface).
        if !self.sidebar_visible {
            return Some((
                chrome,
                Rect {
                    position: LogicalPosition::new(0, CHROME_H).into(),
                    size: LogicalSize::new(w, body_h).into(),
                },
                Rect {
                    position: LogicalPosition::new(w.saturating_sub(1), h.saturating_sub(1)).into(),
                    size: LogicalSize::new(1, 1).into(),
                },
            ));
        }
        match effective {
            PaneMode::Side => {
                let page_w = ((w as f64) * ratio).round() as u32;
                let page_w = page_w.max(320).min(w.saturating_sub(280).max(320)).min(w);
                let side_w = w.saturating_sub(page_w).max(1);
                Some((
                    chrome,
                    Rect {
                        position: LogicalPosition::new(0, CHROME_H).into(),
                        size: LogicalSize::new(page_w, body_h).into(),
                    },
                    Rect {
                        position: LogicalPosition::new(page_w, CHROME_H).into(),
                        size: LogicalSize::new(side_w, body_h).into(),
                    },
                ))
            }
            PaneMode::Stack | PaneMode::Auto => {
                let page_h = ((body_h as f64) * ratio).round() as u32;
                let page_h = page_h
                    .max(200)
                    .min(body_h.saturating_sub(160).max(200))
                    .min(body_h);
                let side_h = body_h.saturating_sub(page_h).max(1);
                Some((
                    chrome,
                    Rect {
                        position: LogicalPosition::new(0, CHROME_H).into(),
                        size: LogicalSize::new(w, page_h).into(),
                    },
                    Rect {
                        position: LogicalPosition::new(0, CHROME_H + page_h).into(),
                        size: LogicalSize::new(w, side_h).into(),
                    },
                ))
            }
        }
    }

    fn apply_bounds(&self) {
        if let Some((c, p, s)) = self.layout() {
            // Page then side, chrome LAST so the toolbar stays topmost in z-order
            // (macOS WKWebView: later child views receive hits when bounds overlap).
            if let Some(v) = &self.page {
                let _ = v.set_bounds(p);
            }
            if let Some(v) = &self.side {
                let _ = v.set_bounds(s);
            }
            if let Some(v) = &self.chrome {
                let _ = v.set_bounds(c);
            }
        }
    }

    fn go(&mut self, raw: &str) {
        let r = self.eng.navigate(raw);
        if !r.ok {
            self.reload_side_error(r.error.as_deref().unwrap_or("navigate failed"));
            return;
        }
        let url = r.url.clone().unwrap_or_else(|| raw.to_string());
        if self.history.last().map(|u| u.as_str()) != Some(url.as_str()) {
            self.history.push(url.clone());
        }
        self.suppress_page_load = true;
        if let Some(page) = &self.page {
            let _ = page.load_url(&url);
        }
        self.refresh_side();
        self.refresh_chrome();
        // overlay re-applied on PageLoaded
    }

    fn click_link(&mut self, n: usize) {
        if n == 0 || n > self.clickmap.len() {
            return;
        }
        let id = self.clickmap[n - 1];
        let r = self.eng.click(id);
        if !r.ok {
            self.reload_side_error(r.error.as_deref().unwrap_or("click failed"));
            return;
        }
        if let Some(url) = r.url {
            if self.history.last().map(|u| u.as_str()) != Some(url.as_str()) {
                self.history.push(url.clone());
            }
            self.suppress_page_load = true;
            if let Some(page) = &self.page {
                let _ = page.load_url(&url);
            }
        }
        self.refresh_side();
        self.refresh_chrome();
    }

    fn back(&mut self) {
        if self.history.len() > 1 {
            self.history.pop();
            if let Some(prev) = self.history.last().cloned() {
                let _ = self.eng.navigate(&prev);
                self.suppress_page_load = true;
                if let Some(page) = &self.page {
                    let _ = page.load_url(&prev);
                }
                self.refresh_side();
                self.refresh_chrome();
            }
        }
    }

    fn on_page_loaded(&mut self, url: &str) {
        if self.suppress_page_load {
            self.suppress_page_load = false;
            // Still re-snapshot in case redirects changed the final URL.
        }
        if url.is_empty() || url == "about:blank" {
            return;
        }
        if self.eng.current_url().as_deref() == Some(url) {
            self.refresh_side();
            self.refresh_chrome();
            self.apply_ai_vis();
            return;
        }
        let r = self.eng.navigate(url);
        if r.ok {
            if let Some(u) = r.url {
                if self.history.last().map(|x| x.as_str()) != Some(u.as_str()) {
                    self.history.push(u);
                }
            }
            self.refresh_side();
            self.refresh_chrome();
        }
        self.apply_ai_vis();
    }

    fn toggle_ai_vis(&mut self) {
        self.apply_ai_vis_flag(!self.ai_vis);
    }

    fn apply_ai_vis_flag(&mut self, on: bool) {
        self.ai_vis = on;
        self.api_session.ai_vis = on;
        if !self.ai_vis {
            self.mark_count = 0;
        }
        self.apply_ai_vis();
        self.refresh_chrome();
        self.refresh_side();
    }

    /// Handle one JSONL API line on the GUI thread (engine + live WebView).
    fn handle_api_line(&mut self, line: &str) -> String {
        // dispatch needs &mut Engine and &mut LiveSurface. Both are fields of App.
        // LiveSurface methods only touch page/ai_vis/mark_count — never eng/session.
        let eng = &mut self.eng as *mut StaticEngine;
        let session = &mut self.api_session as *mut Session;
        // SAFETY: eng + session are disjoint from the fields LiveSurface mutates.
        let eng = unsafe { &mut *eng };
        let session = unsafe { &mut *session };
        api::dispatch(eng, session, Some(self), line)
    }

    fn apply_ai_vis(&self) {
        let Some(page) = &self.page else { return };
        // Re-assert no-popup shims after navigation (init script covers most cases).
        let _ = page.evaluate_script(NO_POPUPS_JS);
        let js = if self.ai_vis {
            AI_VIS_ON_JS
        } else {
            AI_VIS_OFF_JS
        };
        let _ = page.evaluate_script(js);
    }

    fn refresh_chrome(&self) {
        let url = self.eng.current_url().unwrap_or_default();
        let Some(chrome) = &self.chrome else {
            return;
        };
        // Prefer in-place DOM updates — full load_html destroys the toolbar document and
        // drops in-flight clicks (major UX failure: "buttons not clickable").
        let url_js = serde_json::to_string(&url).unwrap_or_else(|_| "\"\"".into());
        let ai_label = if self.ai_vis {
            if self.mark_count > 0 {
                format!("AI vis · {}", self.mark_count)
            } else {
                "AI vis · ON".into()
            }
        } else {
            "AI vis".into()
        };
        let ai_label_js = serde_json::to_string(&ai_label).unwrap_or_else(|_| "\"AI vis\"".into());
        let layout_label_js =
            serde_json::to_string(self.pane_mode.label()).unwrap_or_else(|_| "\"Layout\"".into());
        let sidebar_label = if self.sidebar_visible {
            "Sidebar · on"
        } else {
            "Sidebar · off"
        };
        let sidebar_label_js =
            serde_json::to_string(sidebar_label).unwrap_or_else(|_| "\"Sidebar\"".into());
        let js = format!(
            r#"(function(){{
  var u = document.getElementById('url');
  if (u) u.value = {url};
  var ai = document.getElementById('btn-ai');
  if (ai) {{
    ai.textContent = {ai_label};
    if ({ai_on}) ai.classList.add('on'); else ai.classList.remove('on');
  }}
  var lay = document.getElementById('btn-layout');
  if (lay) lay.textContent = {layout_label};
  var sb = document.getElementById('btn-sidebar');
  if (sb) {{
    sb.textContent = {sidebar_label};
    if ({sidebar_on}) {{ sb.classList.remove('on'); sb.classList.add('ghost'); }}
    else {{ sb.classList.add('on'); sb.classList.remove('ghost'); }}
  }}
}})()"#,
            url = url_js,
            ai_label = ai_label_js,
            ai_on = if self.ai_vis { "true" } else { "false" },
            layout_label = layout_label_js,
            sidebar_label = sidebar_label_js,
            sidebar_on = if self.sidebar_visible {
                "true"
            } else {
                "false"
            },
        );
        if chrome.evaluate_script(&js).is_err() {
            let _ = chrome.load_html(&chrome_html(
                &url,
                self.ai_vis,
                self.mark_count,
                self.pane_mode,
                self.sidebar_visible,
            ));
        }
    }

    fn refresh_side(&mut self) {
        let (html, clickmap) = side_html(
            &self.eng,
            self.view_kind,
            self.ai_vis,
            self.mark_count,
            &self.knox_query,
            &self.knox_matches,
            self.knox_status.as_deref(),
        );
        self.clickmap = clickmap;
        if let Some(side) = &self.side {
            let _ = side.load_html(&html);
        }
    }

    fn set_view(&mut self, kind: ViewKind) {
        self.view_kind = kind;
        self.refresh_side();
    }

    fn knox_find(&mut self, query: Option<String>) {
        let q = query
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                self.eng
                    .current_url()
                    .map(|u| knox::query_from_url(&u))
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_default();
        if q.is_empty() {
            self.knox_status = Some("Knox: type a query or open a site first".into());
            self.knox_matches.clear();
            self.refresh_side();
            return;
        }
        self.knox_query = q.clone();
        self.knox_status = Some(format!("Knox: searching “{q}”… (Touch ID if needed)"));
        self.refresh_side();
        let res = knox::find(&q, 10);
        if res.ok {
            self.knox_matches = res.matches;
            self.knox_status = Some(format!(
                "Knox: {} match(es) for “{}” — secret output suppressed",
                self.knox_matches.len(),
                q
            ));
        } else {
            self.knox_matches.clear();
            self.knox_status = Some(format!(
                "Knox: {}",
                res.error.unwrap_or_else(|| "find failed".into())
            ));
        }
        self.refresh_side();
    }

    fn knox_fill(&mut self, query: String, id: Option<String>, fields: String) {
        if self.page.is_none() {
            self.knox_status = Some("Knox: no page webview".into());
            self.refresh_side();
            return;
        }
        let want_login = fields == "login" || fields == "both";
        let want_password = fields == "password" || fields == "both" || fields.is_empty();

        // Unlock first (may Touch-ID prompt). Build inject scripts; never store secrets on self.
        let mut record_name = String::new();
        let mut scripts: Vec<String> = Vec::new();
        let mut filled: Vec<&str> = Vec::new();
        let mut err_status: Option<String> = None;

        if want_login {
            match knox::unlock_field(&query, "login", id.as_deref()) {
                Ok((title, value)) => {
                    record_name = title;
                    scripts.push(knox::fill_field_js("login", &value));
                    // value dropped here
                    filled.push("login");
                }
                Err(e) => {
                    err_status = Some(format!(
                        "Knox login: {}",
                        e.error.unwrap_or_else(|| "failed".into())
                    ));
                    if !want_password {
                        self.knox_status = err_status;
                        self.refresh_side();
                        return;
                    }
                }
            }
        }

        if want_password {
            match knox::unlock_field(&query, "password", id.as_deref()) {
                Ok((title, value)) => {
                    if record_name.is_empty() {
                        record_name = title;
                    }
                    scripts.push(knox::fill_field_js("password", &value));
                    filled.push("password");
                }
                Err(e) => {
                    self.knox_status = Some(format!(
                        "Knox password: {}",
                        e.error.unwrap_or_else(|| "failed".into())
                    ));
                    self.refresh_side();
                    return;
                }
            }
        }

        if let Some(page) = &self.page {
            for js in scripts {
                let _ = page.evaluate_script(&js);
                // js (with escaped secret) drops at end of iteration
            }
        }

        if filled.is_empty() {
            self.knox_status = err_status.or_else(|| Some("Knox: nothing filled".into()));
        } else {
            self.knox_status = Some(format!(
                "Knox: filled {} from “{}” · secret output suppressed",
                filled.join("+"),
                if record_name.is_empty() {
                    query.as_str()
                } else {
                    record_name.as_str()
                }
            ));
        }
        self.refresh_side();
        self.apply_ai_vis();
    }

    fn reload_side_error(&mut self, msg: &str) {
        self.clickmap.clear();
        if let Some(side) = &self.side {
            let _ = side.load_html(&error_html(msg));
        }
    }

    fn show_read(&mut self) {
        let text = self.eng.read_text();
        if let Some(side) = &self.side {
            let _ = side.load_html(&read_html(&text));
        }
    }

    fn drain_msgs(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Navigate(u) => self.go(&u),
                Msg::Click(n) => self.click_link(n),
                Msg::PageLoaded(u) => self.on_page_loaded(&u),
                Msg::Back => self.back(),
                Msg::Read => self.show_read(),
                Msg::ToggleAiVis => self.toggle_ai_vis(),
                Msg::SetAiVis(on) => self.apply_ai_vis_flag(on),
                Msg::AiMarks { count } => {
                    self.mark_count = count;
                    self.refresh_chrome();
                }
                Msg::KnoxFind { query } => self.knox_find(query),
                Msg::KnoxFill { query, id, fields } => self.knox_fill(query, id, fields),
                Msg::Api(ApiCmd::Line { line, reply }) => {
                    let resp = self.handle_api_line(&line);
                    // Sync human panes after agent-driven changes (still zero clicks).
                    if self.history.last() != self.eng.current_url().as_ref() {
                        if let Some(u) = self.eng.current_url() {
                            if self.history.last().map(|h| h.as_str()) != Some(u.as_str()) {
                                self.history.push(u);
                            }
                        }
                    }
                    self.refresh_side();
                    self.refresh_chrome();
                    let _ = reply.send(resp);
                }
                Msg::SetView(kind) => self.set_view(kind),
                Msg::CycleLayout => {
                    let _ = self.cycle_pane_layout();
                }
                Msg::ToggleSidebar => {
                    let _ = self.toggle_sidebar();
                }
                Msg::ApiLine(line) => {
                    let resp = self.handle_api_line(&line);
                    // Surface outcome in Knox status strip (no modal).
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&resp) {
                        let eng = v
                            .get("english")
                            .and_then(|e| e.as_str())
                            .or_else(|| v.get("error").and_then(|e| e.as_str()))
                            .unwrap_or("API op finished");
                        let outcome = v.get("outcome").and_then(|o| o.as_str()).unwrap_or("");
                        self.knox_status = Some(if outcome.is_empty() {
                            format!("API: {eng}")
                        } else {
                            format!("Hancock {outcome}: {eng}")
                        });
                    }
                    self.refresh_side();
                    self.refresh_chrome();
                }
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let mut attrs = Window::default_attributes();
        attrs.title = "Chrime".into();
        // Landscape desktop default — auto layout chooses side-by-side with ~68% page width.
        attrs.inner_size = Some(LogicalSize::new(1600.0, 1000.0).into());
        let window = event_loop.create_window(attrs).expect("create window");

        let start_url = self
            .eng
            .current_url()
            .unwrap_or_else(|| "about:blank".into());

        // Stash window so layout() can measure; use placeholder bounds then apply_bounds.
        self.window = Some(window);
        let (chrome_r, page_r, side_r) = self
            .layout()
            .expect("window set; layout must resolve");

        // Build order = z-order on macOS: first = bottom, last = top (receives clicks).
        // Page (bottom) → side → chrome toolbar (top). Chrome was built first before and
        // sat *under* page/side, so toolbar buttons were not clickable.
        let tx_page = self.tx.clone();
        let tx_new_win = self.tx.clone();
        let page = WebViewBuilder::new()
            .with_bounds(page_r)
            .with_accept_first_mouse(true)
            .with_initialization_script(NO_POPUPS_JS)
            .with_url(if start_url == "about:blank" {
                "about:blank"
            } else {
                &start_url
            })
            .with_ipc_handler({
                let tx = self.tx.clone();
                move |req: Request<String>| {
                    handle_ipc(req.body(), &tx);
                }
            })
            // No popup windows: load the URL in the main live pane instead.
            .with_new_window_req_handler(move |url, _features| {
                if !url.is_empty() && url != "about:blank" {
                    let _ = tx_new_win.send(Msg::Navigate(url));
                }
                NewWindowResponse::Deny
            })
            // No save-file dialogs. Agents don't navigate download pickers.
            .with_download_started_handler(|_url, _path| false)
            .with_on_page_load_handler(move |ev, url| {
                if matches!(ev, PageLoadEvent::Finished) {
                    let _ = tx_page.send(Msg::PageLoaded(url));
                }
            })
            .build_as_child(self.window.as_ref().unwrap())
            .expect("page webview");

        let tx_side = self.tx.clone();
        let (side_html_str, clickmap) = side_html(
            &self.eng,
            self.view_kind,
            self.ai_vis,
            self.mark_count,
            &self.knox_query,
            &self.knox_matches,
            self.knox_status.as_deref(),
        );
        self.clickmap = clickmap;
        let side = WebViewBuilder::new()
            .with_bounds(side_r)
            .with_accept_first_mouse(true)
            .with_initialization_script(NO_POPUPS_JS)
            .with_html(side_html_str)
            .with_ipc_handler(move |req: Request<String>| {
                handle_ipc(req.body(), &tx_side);
            })
            .with_new_window_req_handler(|_url, _| NewWindowResponse::Deny)
            .with_download_started_handler(|_url, _path| false)
            .build_as_child(self.window.as_ref().unwrap())
            .expect("side webview");

        let tx_chrome = self.tx.clone();
        // Chrome LAST = topmost hit target for toolbar buttons.
        let chrome = WebViewBuilder::new()
            .with_bounds(chrome_r)
            .with_accept_first_mouse(true)
            .with_focused(true)
            // Do NOT inject NO_POPUPS_JS into the toolbar — keep chrome event handlers simple.
            .with_html(chrome_html(
                &start_url,
                self.ai_vis,
                self.mark_count,
                self.pane_mode,
                self.sidebar_visible,
            ))
            .with_ipc_handler(move |req: Request<String>| {
                handle_ipc(req.body(), &tx_chrome);
            })
            .with_new_window_req_handler(|_url, _| NewWindowResponse::Deny)
            .with_download_started_handler(|_url, _path| false)
            .build_as_child(self.window.as_ref().unwrap())
            .expect("chrome webview");

        self.page = Some(page);
        self.side = Some(side);
        self.chrome = Some(chrome);
        self.apply_bounds();
        // First paint overlay after a beat (page may still be settling).
        if self.ai_vis {
            let _ = self.tx.send(Msg::PageLoaded(start_url.clone()));
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::Resized(_) => self.apply_bounds(),
            WindowEvent::CloseRequested => event_loop.exit(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        self.drain_msgs();
    }
}

fn handle_ipc(body: &str, tx: &Sender<Msg>) {
    let v: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => {
            // bare URL typed from chrome fallback
            if !body.is_empty() {
                let _ = tx.send(Msg::Navigate(body.to_string()));
            }
            return;
        }
    };
    match v.get("op").and_then(|o| o.as_str()).unwrap_or("") {
        "navigate" => {
            if let Some(u) = v.get("url").and_then(|u| u.as_str()) {
                let _ = tx.send(Msg::Navigate(u.to_string()));
            }
        }
        "click" => {
            if let Some(n) = v.get("n").and_then(|n| n.as_u64()) {
                let _ = tx.send(Msg::Click(n as usize));
            }
        }
        "back" => {
            let _ = tx.send(Msg::Back);
        }
        "read" => {
            let _ = tx.send(Msg::Read);
        }
        "toggle_ai_vis" => {
            let _ = tx.send(Msg::ToggleAiVis);
        }
        // Explicit set — preferred over toggle so agents/humans never guess state.
        "set_ai_vis" | "ai_vis" => {
            if let Some(on) = v.get("on").and_then(|x| x.as_bool()) {
                let _ = tx.send(Msg::SetAiVis(on));
            } else if let Some(on) = v.get("enabled").and_then(|x| x.as_bool()) {
                let _ = tx.send(Msg::SetAiVis(on));
            } else {
                // bare "ai_vis" with no arg still toggles (chrome button)
                let _ = tx.send(Msg::ToggleAiVis);
            }
        }
        "ai_marks" => {
            let count = v.get("count").and_then(|c| c.as_u64()).unwrap_or(0) as usize;
            let _ = tx.send(Msg::AiMarks { count });
        }
        "knox_find" => {
            let query = v
                .get("query")
                .and_then(|q| q.as_str())
                .map(|s| s.to_string());
            let _ = tx.send(Msg::KnoxFind { query });
        }
        "knox_fill" => {
            let query = v
                .get("query")
                .and_then(|q| q.as_str())
                .unwrap_or("")
                .to_string();
            let id = v.get("id").and_then(|i| i.as_str()).map(|s| s.to_string());
            let fields = v
                .get("fields")
                .and_then(|f| f.as_str())
                .unwrap_or("both")
                .to_string();
            let _ = tx.send(Msg::KnoxFill { query, id, fields });
        }
        "set_view" | "view" => {
            if let Some(kind) = v
                .get("kind")
                .or_else(|| v.get("view"))
                .and_then(|k| k.as_str())
                .and_then(ViewKind::parse)
            {
                let _ = tx.send(Msg::SetView(kind));
            }
        }
        "layout" | "cycle_layout" => {
            // Chrome button cycles; full set via API JSONL on :7420.
            let _ = tx.send(Msg::CycleLayout);
        }
        "toggle_sidebar" | "sidebar_toggle" => {
            let _ = tx.send(Msg::ToggleSidebar);
        }
        "sidebar" | "panel" => {
            // Chrome always toggles; agents set visible via TCP JSONL (handled in dispatch).
            let _ = tx.send(Msg::ToggleSidebar);
        }
        // Hancock / any full API line from chrome (wait=false so the window does not freeze).
        "hancock_request" | "ask_hancock" | "request_permission" => {
            let _ = tx.send(Msg::ApiLine(body.to_string()));
        }
        _ => {}
    }
}

fn esc(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\'' => "&#39;".into(),
            c => c.to_string(),
        })
        .collect()
}

fn chrome_html(
    url: &str,
    ai_vis: bool,
    mark_count: usize,
    pane_mode: PaneMode,
    sidebar_visible: bool,
) -> String {
    let ai_class = if ai_vis { "ai on" } else { "ai" };
    let ai_label = if ai_vis {
        if mark_count > 0 {
            format!("AI vis · {mark_count}")
        } else {
            "AI vis · ON".into()
        }
    } else {
        "AI vis".into()
    };
    let layout_label = pane_mode.label();
    let sidebar_class = if sidebar_visible { "ghost" } else { "ai on" };
    let sidebar_label = if sidebar_visible {
        "Sidebar · on"
    } else {
        "Sidebar · off"
    };
    format!(
        r##"<!DOCTYPE html>
<html><head><meta charset="utf-8">
<style>
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  html, body {{ height: 100%; overflow: hidden; }}
  body {{
    display: flex; align-items: center; gap: 6px;
    padding: 0 8px;
    background: #1c1c1e;
    font: 13px/1.2 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    color: #f5f5f7;
    -webkit-user-select: none;
    user-select: none;
  }}
  .brand {{
    flex: 0 0 auto;
    font-weight: 700;
    color: #ff9f0a;
    letter-spacing: 0.02em;
    white-space: nowrap;
  }}
  .brand span {{ opacity: 0.85; }}
  form {{
    flex: 1 1 auto;
    display: flex; align-items: center; gap: 5px;
    min-width: 0;
  }}
  input {{
    flex: 1 1 auto;
    min-width: 0;
    height: 32px;
    border: 1px solid #3a3a3c;
    border-radius: 8px;
    background: #2c2c2e;
    color: #f5f5f7;
    padding: 0 12px;
    font: inherit;
    outline: none;
  }}
  input:focus {{ border-color: #ff9f0a; }}
  /* Product rule: buttons are square — never rounded. */
  button {{
    flex: 0 0 auto;
    height: 32px;
    padding: 0 10px;
    border: 0;
    border-radius: 0;
    background: #ff9f0a;
    color: #1c1c1e;
    font: 600 11px/1 -apple-system, sans-serif;
    cursor: pointer;
  }}
  button.ghost {{
    background: #3a3a3c;
    color: #f5f5f7;
    border-radius: 0;
  }}
  button.ai {{
    background: #3a3a3c;
    color: #f5f5f7;
    border: 1px solid #636366;
    border-radius: 0;
  }}
  button.ai.on {{
    background: #ff9f0a;
    color: #1c1c1e;
    border-color: #ff9f0a;
    box-shadow: 0 0 0 2px rgba(255,159,10,.35);
    border-radius: 0;
  }}
  .hint {{
    flex: 0 0 auto;
    color: #8e8e93;
    font-size: 10px;
    white-space: nowrap;
  }}
</style></head>
<body>
  <div class="brand">🕵 Chrime <span>· Fraude family</span></div>
  <form id="f" onsubmit="return go()">
    <button type="button" class="ghost" id="btn-back" title="Back">←</button>
    <input id="url" type="text" value="{url}" spellcheck="false" autocomplete="off" />
    <button type="submit" id="btn-go">Go</button>
    <button type="button" class="ghost" id="btn-read">Read</button>
    <button type="button" class="{ai_class}" id="btn-ai" title="Toggle AI visibility marks on the rendered page">{ai_label}</button>
    <button type="button" class="ghost" id="btn-layout" title="Cycle layout: auto · side · stack">{layout_label}</button>
    <button type="button" class="{sidebar_class}" id="btn-sidebar" title="Collapse agent sidebar for full-width live page">{sidebar_label}</button>
    <button type="button" class="ghost" id="btn-knox" title="Find credentials in Knox for this site">Knox</button>
    <button type="button" class="ghost" id="btn-hancock" title="Ask Hancock for permission (human sign)">Hancock</button>
  </form>
  <div class="hint">Click toolbar · sidebar collapses for full-width page</div>
  <script>
    function post(obj) {{
      try {{
        if (window.ipc && window.ipc.postMessage) {{
          window.ipc.postMessage(JSON.stringify(obj));
        }} else if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.ipc) {{
          window.webkit.messageHandlers.ipc.postMessage(JSON.stringify(obj));
        }}
      }} catch (e) {{}}
      return false;
    }}
    function go() {{
      return post({{op:'navigate', url: document.getElementById('url').value}});
    }}
    function back() {{ return post({{op:'back'}}); }}
    function read() {{ return post({{op:'read'}}); }}
    function toggleAi() {{ return post({{op:'toggle_ai_vis'}}); }}
    function cycleLayout() {{ return post({{op:'cycle_layout'}}); }}
    function toggleSidebar() {{ return post({{op:'toggle_sidebar'}}); }}
    function knoxFind() {{ return post({{op:'knox_find'}}); }}
    function askHancock() {{
      var url = document.getElementById('url').value || '';
      return post({{
        op: 'hancock_request',
        action: 'navigate',
        why: 'Human-initiated permission from Chrime chrome for current page',
        risk: 'high',
        wait: false,
        detail: {{ url: url }}
      }});
    }}
    // Bind via addEventListener (more reliable than inline onclick under WKWebView child views).
    document.getElementById('btn-back').addEventListener('click', function(e) {{ e.preventDefault(); back(); }});
    document.getElementById('btn-read').addEventListener('click', function(e) {{ e.preventDefault(); read(); }});
    document.getElementById('btn-ai').addEventListener('click', function(e) {{ e.preventDefault(); toggleAi(); }});
    document.getElementById('btn-layout').addEventListener('click', function(e) {{ e.preventDefault(); cycleLayout(); }});
    document.getElementById('btn-sidebar').addEventListener('click', function(e) {{ e.preventDefault(); toggleSidebar(); }});
    document.getElementById('btn-knox').addEventListener('click', function(e) {{ e.preventDefault(); knoxFind(); }});
    document.getElementById('btn-hancock').addEventListener('click', function(e) {{ e.preventDefault(); askHancock(); }});
    document.getElementById('url').addEventListener('keydown', function(e) {{
      if (e.key === 'Enter') {{ e.preventDefault(); go(); }}
    }});
  </script>
</body></html>"##,
        url = esc(url),
        ai_class = ai_class,
        ai_label = esc(&ai_label),
        layout_label = esc(layout_label),
        sidebar_class = sidebar_class,
        sidebar_label = esc(sidebar_label),
    )
}

fn side_html(
    eng: &dyn Engine,
    view_kind: ViewKind,
    ai_vis: bool,
    mark_count: usize,
    knox_query: &str,
    knox_matches: &[KnoxMatch],
    knox_status: Option<&str>,
) -> (String, Vec<u32>) {
    // One projection of the single page buffer — never a second HTML store.
    let snap = eng.view(view_kind);
    let mut clickmap = Vec::new();
    let mut body = String::new();

    // View switcher — same page, different lens (tabs, not separate stored trees).
    body.push_str(r#"<div class="view-tabs">"#);
    for k in ViewKind::all() {
        let class = if *k == view_kind { "tab on" } else { "tab" };
        body.push_str(&format!(
            r#"<button type="button" class="{class}" onclick="setView('{kind}')">{label}</button>"#,
            class = class,
            kind = k.as_str(),
            label = k.label(),
        ));
    }
    body.push_str("</div>");
    body.push_str(&format!(
        r#"<div class="view-meta">view · <b>{}</b> · {} nodes · html {} KB · one buffer, no copies</div>"#,
        snap.view,
        snap.node_count,
        eng.html_bytes() / 1024
    ));

    if ai_vis {
        body.push_str(&format!(
            "<div class=\"ai-banner\">AI visibility ON — orange boxes on the left are live click targets{}</div>",
            if mark_count > 0 {
                format!(" · {mark_count} marked")
            } else {
                String::new()
            }
        ));
    }

    // Knox panel — metadata only; fill injects secrets into the left pane, never lists them.
    body.push_str(r#"<div class="knox-panel">"#);
    body.push_str("<div class=\"knox-title\">🔐 Knox</div>");
    if let Some(st) = knox_status {
        body.push_str(&format!("<div class=\"knox-status\">{}</div>", esc(st)));
    } else {
        body.push_str(
            "<div class=\"knox-status\">Find credentials for this site, then fill login+password into the left pane. Secrets never appear here.</div>",
        );
    }
    body.push_str(&format!(
        r#"<div class="knox-row">
      <input id="knox-q" type="text" value="{q}" placeholder="query (host or title)" spellcheck="false" />
      <button onclick="knoxSearch()">Find</button>
    </div>"#,
        q = esc(knox_query)
    ));
    if !knox_matches.is_empty() {
        body.push_str("<ul class=\"knox-list\">");
        for m in knox_matches {
            let q_attr = esc(if knox_query.is_empty() {
                m.title.as_str()
            } else {
                knox_query
            });
            let id_attr = esc(m.id.as_deref().unwrap_or(""));
            let login = m.login.as_deref().unwrap_or("—");
            body.push_str(&format!(
                r#"<li>
          <div class="knox-item-title">{title}</div>
          <div class="knox-item-meta">{login}{url}</div>
          <div class="knox-actions">
            <button onclick="knoxFill('{q}','{id}','both')">Fill both</button>
            <button class="ghost" onclick="knoxFill('{q}','{id}','login')">Login</button>
            <button class="ghost" onclick="knoxFill('{q}','{id}','password')">Password</button>
          </div>
        </li>"#,
                title = esc(&m.title),
                login = esc(login),
                url = m
                    .url
                    .as_ref()
                    .map(|u| format!(" · {}", esc(u)))
                    .unwrap_or_default(),
                q = q_attr,
                id = id_attr,
            ));
        }
        body.push_str("</ul>");
    }
    body.push_str("</div>");

    if let Some(t) = &snap.title {
        body.push_str(&format!("<h1 class=\"title\">{}</h1>", esc(t)));
    }
    if let Some(u) = &snap.url {
        body.push_str(&format!("<div class=\"url\">{}</div>", esc(u)));
    }

    if view_kind == ViewKind::Meta {
        body.push_str("<div class=\"meta-grid\">");
        if let Some(counts) = &snap.counts {
            for (role, n) in counts {
                body.push_str(&format!(
                    "<div class=\"meta-row\"><span>{}</span><span>{}</span></div>",
                    esc(role),
                    n
                ));
            }
        }
        body.push_str(&format!(
            "<div class=\"meta-row\"><span>html_bytes</span><span>{}</span></div>",
            eng.html_bytes()
        ));
        body.push_str("</div>");
    } else {
        for n in &snap.nodes {
            match n.role.as_str() {
                "heading" => {
                    if !n.text.is_empty() {
                        body.push_str(&format!("<h2>{}</h2>", esc(&n.text)));
                    }
                }
                "link" | "button" => {
                    clickmap.push(n.node_id);
                    let label = if n.text.is_empty() {
                        "(no text)".to_string()
                    } else {
                        n.text.clone()
                    };
                    let n_display = clickmap.len();
                    body.push_str(&format!(
                        "<div class=\"link\" onclick=\"clickN({n})\"><span class=\"n\">[{n}]</span> {label}</div>",
                        n = n_display,
                        label = esc(&label)
                    ));
                }
                "field" => {
                    let label = if n.text.is_empty() { "input" } else { &n.text };
                    body.push_str(&format!("<div class=\"field\">[ {} ]</div>", esc(label)));
                }
                _ => {
                    if !n.text.is_empty() {
                        body.push_str(&format!("<p>{}</p>", esc(&n.text)));
                    }
                }
            }
        }

        if snap.nodes.is_empty() {
            body.push_str("<p class=\"empty\">No nodes in this view. Try Full, or open a URL.</p>");
        }
    }

    let html = format!(
        r##"<!DOCTYPE html>
<html><head><meta charset="utf-8">
<style>
  * {{ box-sizing: border-box; }}
  html, body {{ margin: 0; padding: 0; height: 100%; }}
  body {{
    background: #0d0d0f;
    color: #e8e8ed;
    font: 13px/1.45 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    padding: 16px 18px 48px;
    overflow-y: auto;
    border-left: 1px solid #2c2c2e;
  }}
  .view-tabs {{
    display: flex; flex-wrap: wrap; gap: 4px;
    margin: 0 0 8px;
  }}
  .view-tabs .tab {{
    height: 26px; padding: 0 9px; border: 1px solid #3a3a3c;
    border-radius: 0; background: #1c1c1e; color: #c7c7cc;
    font: 600 11px -apple-system, sans-serif; cursor: pointer;
  }}
  .view-tabs .tab.on {{
    background: #ff9f0a; color: #1c1c1e; border-color: #ff9f0a;
    border-radius: 0;
  }}
  .view-meta {{
    color: #636366; font-size: 11px; margin: 0 0 12px;
  }}
  .view-meta b {{ color: #ff9f0a; }}
  .meta-grid {{
    border: 1px solid #2c2c2e; border-radius: 8px; overflow: hidden;
    margin: 8px 0 16px;
  }}
  .meta-row {{
    display: flex; justify-content: space-between; gap: 12px;
    padding: 6px 10px; border-top: 1px solid #2c2c2e;
    font: 12px ui-monospace, Menlo, monospace; color: #c7c7cc;
  }}
  .meta-row:first-child {{ border-top: 0; }}
  .ai-banner {{
    background: rgba(255,159,10,.12);
    border: 1px solid #ff9f0a;
    color: #ff9f0a;
    border-radius: 8px;
    padding: 8px 10px;
    margin: 0 0 14px;
    font: 600 12px/1.35 -apple-system, BlinkMacSystemFont, sans-serif;
  }}
  .knox-panel {{
    background: #141416;
    border: 1px solid #3a3a3c;
    border-radius: 10px;
    padding: 12px;
    margin: 0 0 16px;
  }}
  .knox-title {{
    font: 700 13px/1.2 -apple-system, BlinkMacSystemFont, sans-serif;
    color: #64d2ff;
    margin-bottom: 6px;
  }}
  .knox-status {{
    color: #8e8e93;
    font-size: 11px;
    margin-bottom: 10px;
    line-height: 1.35;
  }}
  .knox-row {{ display: flex; gap: 6px; margin-bottom: 10px; }}
  .knox-row input {{
    flex: 1; min-width: 0; height: 28px; border-radius: 6px;
    border: 1px solid #3a3a3c; background: #2c2c2e; color: #f5f5f7;
    padding: 0 8px; font: 12px ui-monospace, Menlo, monospace;
  }}
  .knox-row button, .knox-actions button {{
    height: 28px; padding: 0 10px; border: 0; border-radius: 0;
    background: #64d2ff; color: #0d0d0f; font: 600 11px -apple-system, sans-serif;
    cursor: pointer;
  }}
  .knox-actions button.ghost {{ background: #3a3a3c; color: #f5f5f7; border-radius: 0; }}
  .knox-list {{ list-style: none; padding: 0; margin: 0; }}
  .knox-list li {{
    padding: 8px 0; border-top: 1px solid #2c2c2e;
  }}
  .knox-item-title {{ color: #f5f5f7; font: 600 12px -apple-system, sans-serif; }}
  .knox-item-meta {{ color: #8e8e93; font-size: 11px; margin: 2px 0 6px; word-break: break-all; }}
  .knox-actions {{ display: flex; gap: 6px; flex-wrap: wrap; }}
  .title {{
    font: 700 16px/1.3 -apple-system, BlinkMacSystemFont, sans-serif;
    color: #fff;
    margin: 0 0 4px;
  }}
  .url {{
    color: #8e8e93;
    font-size: 11px;
    margin-bottom: 4px;
    word-break: break-all;
  }}
  .meta {{
    color: #636366;
    font-size: 11px;
    margin-bottom: 16px;
    padding-bottom: 12px;
    border-bottom: 1px solid #2c2c2e;
  }}
  h2 {{
    font: 700 14px/1.35 -apple-system, BlinkMacSystemFont, sans-serif;
    color: #fff;
    margin: 18px 0 8px;
  }}
  p {{ margin: 6px 0; color: #c7c7cc; }}
  .link {{
    color: #ff9f0a;
    cursor: pointer;
    padding: 3px 0;
    margin: 1px 0;
  }}
  .link:hover {{ background: #1c1c1e; border-radius: 4px; }}
  .link .n {{ color: #ff9f0a; font-weight: 700; margin-right: 6px; }}
  .field {{ color: #636366; margin: 4px 0; }}
  .empty {{ color: #8e8e93; margin-top: 40px; text-align: center; }}
  .foot {{
    position: fixed; bottom: 0; left: 0; right: 0;
    padding: 8px 18px;
    background: #0d0d0f;
    border-top: 1px solid #2c2c2e;
    color: #636366;
    font-size: 11px;
  }}
</style></head>
<body>
{body}
<div class="foot">🎭 Chrime · Knox fills passwords into the left pane · secrets never shown here</div>
<script>
  function post(obj) {{ window.ipc.postMessage(JSON.stringify(obj)); }}
  function clickN(n) {{ post({{op:'click', n:n}}); }}
  function knoxSearch() {{
    var q = (document.getElementById('knox-q') || {{}}).value || '';
    post({{op:'knox_find', query: q}});
  }}
  function knoxFill(query, id, fields) {{
    post({{op:'knox_fill', query: query, id: id || null, fields: fields || 'both'}});
  }}
  function setView(kind) {{ post({{op:'set_view', kind: kind}}); }}
  document.getElementById('knox-q') && document.getElementById('knox-q').addEventListener('keydown', function(e) {{
    if (e.key === 'Enter') {{ e.preventDefault(); knoxSearch(); }}
  }});
</script>
</body></html>"##,
        body = body
    );
    (html, clickmap)
}

fn error_html(msg: &str) -> String {
    format!(
        r##"<!DOCTYPE html><html><head><meta charset="utf-8">
<style>body{{background:#0d0d0f;color:#ff453a;font:13px ui-monospace,Menlo,monospace;padding:24px}}</style>
</head><body><strong>Error</strong><p>{}</p></body></html>"##,
        esc(msg)
    )
}

fn read_html(text: &str) -> String {
    format!(
        r##"<!DOCTYPE html><html><head><meta charset="utf-8">
<style>
body{{background:#0d0d0f;color:#e8e8ed;font:13px/1.5 ui-monospace,Menlo,monospace;padding:20px;white-space:pre-wrap;word-break:break-word}}
.hint{{color:#8e8e93;margin-bottom:12px;font-size:11px}}
a{{color:#ff9f0a}}
</style></head>
<body><div class="hint">Full page text (press Go or click a link to return to snapshot)</div>{}</body></html>"##,
        esc(text)
    )
}
