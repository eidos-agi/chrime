# Chrime — design

**A browser built for AI agents — with a human co-surf surface by default.** The primary
control path is still the JSON API (semantic DOM, stable node-ids). The **default binary**
opens a dual-pane GUI so you can help the agent when surfing gets hard (auth walls, captchas,
judgment calls). Existing automation tools puppeteer a human browser through CDP; Chrime
inverts that: agent-native API first, optional human eyes on a real render.

**Normative requirements and telos:** see `TELOS.md` and `REQUIREMENTS.md` (north star
`ns_46cf50bfa273`). This file is design rationale only.

## Default = GUI co-surf; lean is opt-in

| Build | Role |
|-------|------|
| **Default** (`cargo build --release`) | Dual-pane GUI + API on `:7420` — human helps AI surf |
| **Lean** (`--no-default-features`) | ureq + scraper + serde only — CI / pure agent pipe |
| **Servo** (`--features servo`) | Full JS engine — heavy, optional |

```sh
cargo build --release                        # DEFAULT: GUI
cargo build --release --no-default-features  # lean headless
```

## Square buttons only

Chrime chrome controls are **square** (`border-radius: 0`). Never use rounded pills
or soft corners on buttons, view tabs, or AI-vis mark badges. Inputs may differ;
buttons do not.

## No pop-ups for features

A feature that only changes behind a dialog a human must navigate is a broken feature for
agents (and for this product). Rules:

1. **Chrime features** (AI visibility, Knox find/fill, read mode, engine choice) are
   one-shot: always-visible chrome controls and/or JSON ops. Never a settings modal, never
   a multi-step wizard to flip a boolean.
2. **Web-originated modals** that block the loop are suppressed: `alert`/`confirm`/`prompt`,
   `window.open` (same-tab or deny), `target=_blank` (main pane), download save dialogs
   (denied).
3. **Exception:** OS-level Knox Touch ID unlocks *secrets*. That is Knox's security
   boundary, not a Chrime feature switch. Chrime must not invent its own modal to change
   product behavior.

## The load-bearing decision: snapshot, not live

A human browser is a **real-time** system — it lays out, paints, and GPU-composites ~60×/s
forever, runs `requestAnimationFrame` loops, decodes video, animates. That continuous render
pipeline is where nearly all the memory/CPU/GPU goes. **An agent never watches a stream of
frames; it acts in a request/response loop.** So Chrime is a *settle-and-snapshot* engine:

```
navigate → build DOM → run JS until quiescent → FREEZE → answer queries → discard on next navigate
```

Dropping "live" deletes the single most expensive subsystem: no compositor, no vsync, no rAF
loop, no animation timers left running. Everything is **pull-based** (compute on request), not
push-based (a loop that never stops).

Consequences:

- **Pixels become cheap.** A screenshot is a *one-shot, offscreen, software raster* of the
  settled layout — downscaled, or just one subtree — then freed. No GPU, no persistent
  surface. "Pixels on demand" is nearly free because nothing stays alive between shots.
- **Time is controllable.** No live clock ⇒ fast-forward animations to their end state, cap
  runaway timers ⇒ a deterministic settled DOM. Kills timing flakiness.
- **Memory is peak, not sustained.** Between snapshots nothing runs; a tab serializes to its
  DOM and goes dormant, rehydrating on demand. Thousands of tabs where Chromium holds tens.

## Views (many lenses, one buffer)

A page is stored **once** (raw HTML + url/title). "Views" are named projections of that
single buffer — outline, links, fields, clickables, text, compact, meta — produced on
demand and discarded. Switching views costs a re-walk, not a second copy of the page.
Node-ids are stable across views (same walk order), so `click` still works after an agent
moves from `outline` to `links`. The Meta view returns only role counts + `html_bytes`
(empty `nodes`), so introspection is near-zero RAM beyond the HTML itself.

## Can an agent "see" without pixels?

"Seeing" is two layers:

1. **Semantic** — text, links, roles, fields, tables, headings, and (with a real engine)
   computed styles + geometry + visibility. Covers ~90% of read-and-act tasks, and beats
   vision: the DOM is ground truth; vision infers structure from pixels and clicks by fragile
   coordinates. (Why accessibility-tree agents outscore pure-vision agents on most benchmarks.)
2. **Pixel** — the painted image. Genuinely required for visual-only content: images, charts,
   `<canvas>`/WebGL, video frames, PDF-as-image, CAPTCHAs, and spatial/design questions.

So: **DOM-first, pixels on demand.** Not either/or. And note — running JS (v8) and *painting*
are different costs; you can have a faithful post-JS DOM without ever painting.

## The bet: split the duty

Don't grow one god-browser. Split "seeing" into two components with **one API contract**:

- **Duty A — semantic (Chrime core):** DOM / settle / snapshot. Tiny, always-on, the entry
  point, handles ~90%. Stays pure — never learns to paint.
- **Duty B — pixel (separate component):** heavier, invoked *only* when A hits a visual
  question. Swappable; may reuse a real headless renderer for the one-shot raster.

Why it's a good bet: A and B have **opposite cost profiles** (A tiny/always-on, B heavy/rare);
coupling them makes both worse. Splitting lets A stay memory-free forever and B be replaceable.

The risk is **the handoff**: A hands B a precise reference (URL + settled state + node-id or
region); B must reproduce the *same* page to shoot it. The settle-not-live decision is what
makes this winnable — state is deterministic, so B can re-navigate + settle independently
instead of sharing a live surface. (In the Eidos ecosystem: Chrime = A, a Helios-style
renderer = B.)

## Roadmap

- **v0 (done):** `StaticEngine` — fetch + html5ever parse, no JS. Proves the interface.
  1.9 MB binary, ~7.6 MB RSS. Misses JS-rendered content.
- **v1:** embed v8; settle-and-freeze ⇒ faithful post-JS DOM + computed styles/geometry/
  visibility. **No paint.** The big "seeing" unlock; stays lean. This is the version that can
  scour a real JS app (Gmail, etc.) — it needs JS + a session/cookies, which v0 has not.
- **Duty B (separate, parallel):** an on-demand one-shot offscreen rasterizer for the
  visual-only ~10%. Its own component behind the shared contract — not a Chrime-core feature.

## Engine seam

Everything above lives behind one trait so the engine is swappable:

```rust
trait Engine {
    fn navigate(&mut self, url: &str) -> NavResult;
    fn snapshot(&self) -> DomSnapshot;   // the semantic DOM
    fn read_text(&self) -> String;
    fn click(&mut self, node_id: u32) -> NavResult;
    fn current_url(&self) -> Option<String>;
    // v1 adds: fill/submit, query(selector), wait_until(quiescent), computed style/geometry
    // v2 adds: screenshot(region?) -> bytes
}
```

`StaticEngine` implements it today; a `V8Engine` implements the same API next. The API never
changes when the engine gets more faithful — that seam is the whole point.

## Interfaces

- **Headed (default):** `chrime [url]` — a terminal render of the semantic DOM (numbered
  links, headings, wrapped text) with an address prompt. For humans; records cleanly in VHS.
- **API:** `chrime --api` — one JSON command per line on stdin, one JSON result per line on
  stdout (`navigate`, `snapshot`, `read`, `click`, `current`). For agents. Same `Engine`.
