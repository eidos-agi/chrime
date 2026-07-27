# Chrime — Telos

North star id: `ns_46cf50bfa273`

## Philosophy

**Chrime is a real browser built so AIs can steer it — including ~100 sessions at once.**

Not a human browser with a debug port bolted on. Not a static HTML fetcher wearing a
browser costume. Not a dual-pane co-surf app that sometimes answers JSONL.

One product: a **real browser runtime** (JS, cookies, layout, clock) whose **primary
driver is an agent API** — semantic DOM, stable node-ids, deterministic settle, structured
actions. Humans may **attach a view** to a session when they need to watch, help, or log
in. They are not the control path. Pixels and coordinates are never the primary way an
agent acts.

What stays true when everything else changes:

1. **Real browser** — the document the agent reads and acts on is the same post-JS page a
   human would see in that session.
2. **AI-first interface** — every capability is a programmatic op; settle-and-snapshot, not
   “watch 60fps and guess.”
3. **Fleet scale** — the unit of work is a **session**, not “the window.” ~100 concurrent
   steered sessions is a design constraint, not a stretch hope.
4. **Own the engine** — control comes from owning the substrate, not puppeteering Chrome
   through CDP as the permanent architecture.
5. **One engine of record per session** — no parallel fake DOM that agents drive while a
   separate WebView holds the truth.
6. **Secrets and permissions stay real** — Knox never leaks passwords; Hancock gates
   consequential actions.

Static fetch+parse and dual-pane GUI are **stubs or attach surfaces** when they exist —
not the definition of Chrime.

## The Friction

We tried to put agents on the real web (inbox scour, multi-step app work). Every tool was a
puppeteer on a human browser (CDP): no owned clock, heuristic settle, coordinate clicks,
hidden internals, flaky timing. Then we built the wrong escape hatch: a lean static engine
that agents could drive cleanly — but it was **not a browser**, so Gmail and every real SPA
stayed fake-green or impossible. We glued a human WebView next to it and called that
“co-surf,” which gave **two truths on one UI thread** and a sync API that deadlocks when it
touches the live page. The pain is structural: agents need totality on the **real** web,
at **fleet** scale, without being guests of Chrome’s protocol or guests of a demo dual-pane.

## The Cost of Not

If this is never built as a real AI-steerable browser, the agent economy stays trapped:

- Behind CDP flakiness and pixel inference, or
- Behind toys that only work on server-rendered HTML.

Either way, **you cannot run a hundred reliable steers** on real apps. Teams burn tokens on
retries and screenshots, ship flaky automations, and mistake “we opened a window” for
“we control the web.” The people who pay are everyone building autonomous work on the open
web. Correctness at scale is the game; not-building this costs correctness **and** throughput.

## Why Not The Alternatives

- **Chrome + CDP forever** — guest on a human browser; no owned clock; no deep control
  surfaces; not our engine; not our scale model.
- **Static HTML engine as the product** — great for CI stubs; **not a browser**. Fails
  Gmail, SPAs, and any JS-auth wall. Calling it Chrime is a lie.
- **One dual-pane App as the product** — optimizes for 1 human + 1 agent demo. Cannot host
  ~100 concurrent sessions; couples agent I/O to the GUI event loop.
- **Vision-only agents** — throw away ground-truth structure; slow, brittle, expensive.
  Marks may assist vision; vision is never the sole control path.
- **Fork Blink as step one** — unmaintainable monster; still a human-browser architecture.
- **Feature control via modals** — agents cannot navigate settings wizards; every feature is
  an API op (and optional always-visible chrome when a human is attached).

## The Unique Offer

Chrime is a **real browser runtime for agents**, with:

- **Engine ownership path** (embed/own substrate — Servo as starting legal substrate) so
  settle, interception, and layout can become first-class data, not protocol leftovers.
- **Agent-native I/O** — navigate / settle / snapshot / act-by-node-id; same API for 1
  session or 100.
- **Session as the unit** — isolated profile, cookies, page, history; kill or snapshot one
  without taking down the fleet.
- **Optional human attach** — open a view on session *k* for login or supervision; detach
  without changing what the agent steers.
- **Determinism where the clock is ours** — settle is quiescence, not sleep; same inputs →
  same settled DOM when the engine is under our control.
- **Knox + Hancock** — credentials and human sign-off without secrets in logs or fake
  approvals.

That combination is not Playwright. It is not a headless scraper. It is not a co-surf toy.
It is a browser AIs can drive **and** scale.

## How It Grows

1. **Faithfulness** — real engine + WPT-gated fleet; “renders the real web” is measured.
2. **Control depth** — each agent wall becomes a falsifiable control surface (settle,
   intercept, layout-as-data) on the **same** engine agents already use.
3. **Fleet scale** — session pool, isolation, memory budget per session, attach/detach
   viewers; target **≥100 concurrent steered sessions** on a single strong machine as a
   hard design number (not “tabs in one WebView process forever”).
4. **Regression corpus** — deterministic runs become fixtures; yesterday’s scour guards
   tomorrow’s engine change.
5. **Stub honesty** — StaticEngine remains allowed **only** as a labeled non-browser stub
   for pure unit/CI paths that do not claim SPA/Gmail green.

## Metric
name: pct_requirements_green
kind: percent

## Serves
parent: root
how: Root mission of the Eidos body — the browser an agent can fully control, at fleet
scale. Serves autonomous agents doing real work on the open web, the largest interface surface.

## Invariants

### own-the-engine
must: Agent control originates from owning (or embedding as-owned) the browser engine, never
from CDP / coordinate-click / screenshot-click as the permanent sole action model.
case: Agent control path does not require CDP transport or mouse-at-(x,y) as the only way to act.
irreversible: true

### one-engine-of-record
must: For any session an agent is steering, there is exactly one engine of record. The DOM
the agent snapshots and clicks is that engine’s post-JS document for that session — not a
parallel static fetch of the same URL while a WebView holds the real session.
case: After JS has mutated the page in-session, snapshot/find/click operate on that
mutation; a second silent document is not the agent control path.
irreversible: true

### agent-native-interface
must: Primary agent I/O is semantic DOM + stable node-ids + structured ops; pixels and
coordinates are never required to complete the control path.
case: Scripted task completes via API with pixel/coordinate calls disabled.

### fleet-sessions
must: The unit of steerability is a session (isolated state). The system is designed to run
on the order of **100 concurrent steered sessions**, not one global dual-pane window.
case: Architecture and API address sessions by id; resource model documents per-session
budget; a fleet smoke can open N>1 isolated sessions without sharing cookie jars.
irreversible: true

### determinism
must: Where we own the clock, same inputs produce the same settled result; settle is
quiescence, not a sleep heuristic.
case: Double navigate+settle+snapshot on a fixed fixture yields equal DOM trees (engine of
record under test).

### api-complete-control
must: Every product capability is reachable through the agent API alone; a human GUI is
optional attach/observe/login, never a required control path for automated work.
case: Multi-op agent script completes without mouse/keyboard on a window (session may be
headless or headed-attach; agent still uses only the API).

### no-feature-popups
must: Product features are not behind multi-step dialogs; web modal APIs do not block the
agent loop.
case: Feature changes are API ops (and optional always-visible chrome when attached); alert/
confirm/prompt/window.open do not stall the agent.

### human-attach-optional
must: A human may attach a visual surface to a session for supervision or auth; attach is
not the identity of the product and must not be the only way the agent “has” a browser.
case: Agent can steer sessions with no window; with a window attached, the engine of record
for agent ops remains the session engine, not a disconnected spectator document.
irreversible: false

### single-buffer-views
must: Named views of a page are projections of one stored page buffer for that session;
switching views does not allocate a second full page copy.
case: Views share one html_bytes; node-ids stable across views.

### secrets-never-surface
must: Knox credential values never appear in API JSON, logs, UI panels, or stdout.
case: knox_find/fill/use responses carry secret_output suppressed; no password values in body.

### hierarchical-breadcrumbs
must: Every run/session/request has a unique hierarchical id under `CHRIME.*`; API responses
include `_trace`; events append-logged with plain-english.
case: Response `_trace.id` matches hierarchy; see `docs/BREADCRUMBS.md`.

### session-save-shim
must: A session can be saved and restored (page buffer + history + flags + lineage) via API.
case: session_save → later session_load restores url and content with shim_from lineage.

### hancock-permissions
must: Consequential surf actions can require Hancock; STILL_PENDING/QUEUED/DENIED is not go;
only APPROVED_AND_RAN / AUTO_APPROVED_AND_RAN is go.
case: hancock_request/wait/pending; missing CLI is HANCOCK_MISSING, not fake approval.

## Requirements

### real-browser-engine
must: The production engine of record executes JavaScript and maintains a real document and
network stack for the session (not static HTTP+parse alone).
case: navigate a JS-rendered fixture; snapshot contains nodes only present after script ran.
status: open (Servo path partial; StaticEngine is stub-only and must not claim this green)

### dom-snapshot-api
must: navigate + settle + snapshot return semantic DOM with stable node-ids over the agent API.
case: multi-op script; every node has node_id.
status: green (stub engine); must re-prove on real-browser-engine

### full-jsonl-api
must: Complete JSONL op surface for navigation, inspection, action, views, Knox, sessions,
Hancock, status — addressable per session as fleet matures.
case: help lists ops; multi-op script completes without a GUI for stub/CI engine.
status: green (single-session); multi-session addressing open

### fleet-100
must: System design and implementation target **≥100 concurrent steered sessions** on a
reference workstation (document budget; prove with a load smoke).
case: open 100 isolated sessions (or document interim N with a dated plan to 100); no shared
mutable cookie jar; agent can address session ids.
status: open

### page-views
must: Named views (full/outline/links/fields/clickables/text/compact/meta) as pure projections.
case: outline/meta/links; stable node_ids.
status: green

### ai-visibility-marks
must: Optional Set-of-Mark overlay for attached live views; API-reachable; never sole control path.
case: marks on clickables when attach surface + set_ai_vis; no dialog to toggle.
status: green (attach/GUI path)

### knox-credentials
must: Knox find/fill/use without secrets in responses; prefer in-browser fill into the
session’s real document.
case: knox_find metadata only; fill does not echo password.
status: green (path exists; must target engine of record)

### no-web-modals
must: Suppress alert/confirm/prompt; deny popups/downloads that block agents.
case: page cannot stall agent on alert or second window.
status: green (attach/GUI path)

### memory-efficiency
must: Per-session memory dominated by one page buffer + engine state; views ephemeral; dormant
sessions can serialize.
case: document per-session budget toward fleet-100; views do not clone full DOMs.
status: open (stub lean binary exists; fleet budget not proven)

### api-suite-100
must: ≥100 plain-English machine-checkable API tests; failures go to bug log.
case: run_api_suite.py on CI engine stub; ≥100 cases.
status: green

### faithful-js
must: Agent-visible DOM is post-JS for the engine of record.
case: JS fixture post-marker present after settle on real engine.
status: open as product green (partial on Servo; stub correctly fails)

### auth-session
must: Cookies/session carry across navigations and clean process restarts for a session profile.
case: login then protected page; jar reload in new process.
status: open as product green (partial on Servo)

### gmail-scour
must: Agent scours a real authenticated Gmail session end-to-end via API only — same engine of
record the session used to authenticate — extract structured data from ≥5 threads (complex
bar: 6 unrelated themes per cases/gmail-scour).
case: live report with hits; zero pixel/coordinate ops; not a static-engine fake.
status: open

### control-surfaces
must: Deterministic settle, synchronous interception, render-tree/layout as data — on the
owned/embedded engine.
case: settle receipt + intercept + layout ops return real data.
status: open (settle partial)

## Anti-requirements (explicit)

These are **not** the product and must not be confused with green telos:

- Dual StaticEngine + WKWebView as two simultaneous truths for one agent session.
- Default identity = dual-pane co-surf window.
- Headless static scrape as “we support Gmail.”
- Blocking the UI thread to wait for WebView eval callbacks for agent API replies.
- Treating CDP as the long-term control path.

## Preferences

- Prefer Rust; embed a real engine (Servo start) behind one Engine trait.
- Prefer multi-session fleet over single-window architecture.
- Prefer API-first; if it is not an op, it is incomplete.
- Prefer human **attach** over human-as-required-pilot.
- Prefer Knox for secrets; Hancock for consequential actions.
- Prefer settle-and-snapshot over real-time compositor cost for agent loops.
- Prefer WPT-gated faithfulness growth.
- Prefer honest stubs: label StaticEngine as non-browser; never mark SPA acceptance green on it.
- Prefer square chrome when a human attach UI exists; suppress web modals.
- Prefer one buffer + view projections per session.
