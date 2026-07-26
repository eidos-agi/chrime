# Chrime — Telos

## Philosophy

We believe control is an ownership property, not a protocol feature. An agent that must
reason and act on the web needs to control every layer of the browser — the DOM, the event
pipeline, layout, and the clock — and that degree of control exists only if you own the
engine. The browser was built for a human watching pixels sixty times a second; an agent
never watches, it acts in a request/response loop. So the machine-native surface must be
primary: every capability exists as a programmatic API, and the human surface (if any) is
optional observation, never a required control path. What stays true when everything else
changes: the interface an agent drives is the semantic DOM with stable identifiers; the
engine is a settle-and-snapshot machine, not a real-time one; features never hide behind
pop-ups a human must navigate; the **default binary is the dual-pane GUI** so a human can
help the agent surf in real time while the API remains fully driveable; a lean headless
build stays available via `--no-default-features`; views of a page are projections of one
buffer, not copies; and any capability that cannot be reached deterministically and
programmatically does not exist for our purposes. Own the engine, expose its internals, and
make the agent the first-class actor rather than a guest poking at a debug port from outside
the loop.

## The Friction

The incident: we tried to have an agent scour a real inbox and drive real web tasks, and
every available tool was a puppeteer strapped to a human browser through a debug port. Chrome
DevTools Protocol lets you poke the browser from outside its event loop, but you cannot own
the clock, cannot make "the page has settled" a guarantee instead of a heuristic, cannot hook
every mutation and layout pass synchronously, and cannot read the render tree and computed
layout as first-class data. Clicks are simulated mouse events at coordinates; timing is a
race; the whole surface is Chrome's, on Chrome's terms, versioned and detectable. The
concrete pain was watching an agent flake on timing it could not control and hit walls the
protocol would not open — and realizing the ceiling was structural, not a missing feature.
A debug protocol on a human browser is the wrong substrate for an agent that needs totality.

## The Cost of Not

If this is never built, the agent economy stays trapped behind a human browser it can only
puppeteer. Every autonomous web task inherits CDP's non-determinism: flaky settles,
coordinate clicks that miss, interception that races, engine internals that stay hidden. Teams
paper over it with retries, screenshots, and vision models that infer structure from pixels —
expensive, slow, and brittle — because they never had the ground-truth DOM or a deterministic
clock. The people who pay are anyone trying to build reliable web agents: they ship
flakiness, burn tokens on pixel inference, and cap their ambition at what a debug protocol
tolerates. Worse, the field mistakes "we drove Chrome" for "we control the web," and never
builds the thing that would make agents dependable on the open web. A shortcut is acceptable
only where determinism and depth do not matter; the moment they do, not-building this costs
correctness, and correctness is the whole game for an autonomous agent.

## Why Not The Alternatives

We considered the obvious paths and killed each for a concrete reason:

- **Chrome + CDP (Playwright/Puppeteer/CDP-as-usual)** — insufficient because it is
  control-from-outside a human browser: no owned clock, heuristic settle, no synchronous deep
  interception, render internals hidden, coordinate-simulated input, detectable, and
  un-ownable without forking Blink. It is a bridge to a working demo, never the destination.
- **Fork Blink/Chromium to expose internals** — insufficient because Blink is a
  multi-million-line monster; a fork is unmaintainable, drifts from upstream constantly, and
  still carries the human-browser architecture we are trying to leave.
- **Vision-only agents (screenshot + click by pixel)** — insufficient because it throws away
  the ground-truth DOM to re-infer structure from pixels, which is slow, brittle, expensive,
  and non-deterministic; it is the very human-oriented control model we reject, dressed as
  intelligence. (AI-visibility marks exist so that *when* vision is used as a supplement, the
  page already shows numbered click targets — vision is never the sole control path.)
- **A pixel-perfect from-scratch renderer as step one** — insufficient because painting at
  frame rates is the most expensive and least necessary subsystem for an agent that never
  watches; leading with it burns the budget on the wrong thing and delays the DOM-first
  control that actually matters.
- **Embed a real engine but keep a human-style API on top** — insufficient because it leaves
  the agent puppeteering again; owning the engine pays off only if the interface is
  agent-native (semantic DOM, deterministic settle, exposed internals). Engine and interface
  are separable, and the interface is the point.
- **Feature control via settings dialogs / pop-ups** — insufficient because an agent cannot
  "navigate a UI to flip a boolean"; every feature is an API op and/or an always-visible
  control. Web-originated modals (`alert`/`confirm`/`prompt`/`window.open`) are suppressed
  so they never block the agent loop.
- **GUI only as an afterthought** — insufficient for real surfing: most agent web work needs
  a human co-pilot watching the rendered page. Default is dual-pane GUI + API; lean headless
  is opt-in (`--no-default-features`) for pure agent/CI paths. Full JS engine stays behind
  `--features servo`.

## The Unique Offer

Chrime offers what nothing else can: total, deterministic, fine-grained control of a browser,
exposed as an agent-native interface. The clock is ours — settle is a guaranteed primitive,
animations fast-forward to their end state, timers are capped, and the same inputs produce the
same result. Interception is synchronous at any layer — every request, mutation, style
recalc, and layout pass is a first-class hook, not a polled event. The render tree and
computed layout are queryable data, not reconstructed guesses. The action model is first-class:
"operate on node N" carries an engine-guaranteed post-condition instead of a simulated mouse
event. Credentials flow through Knox without secrets appearing in logs or API responses.
Many named views of one page cost a re-walk, not a second HTML copy. And because we own the
engine, there is no detection surface and no capability the protocol refuses to open — the
agent is the browser, not a guest inside it. That combination is unavailable from any
human-browser-plus-protocol, by construction, which is why owning the engine is the mission
rather than an optimization.

## How It Grows

Chrime improves itself along three axes, each with a built-in oracle. Faithfulness grows
against web-platform-tests: a coding-agent fleet closes conformance gaps in parallel, gated on
WPT pass rate, so "renders the real web" is measured, not asserted. Control depth grows by
demand: each new agent task that hits a wall becomes a new first-class control surface with a
falsifiable case, so the API expands only where reality proved it thin. Leanness grows through
the settle-and-snapshot lifecycle: dormant tabs serialize to their DOM and rehydrate on
demand, and the split-duty design keeps the semantic core pure while pixels stay a separate
on-demand component — so scale (thousands of tabs) is a target with a number, not a hope. The
engine substrate swaps behind one Engine trait, so the project starts on a real engine and
migrates to an owned one without ever rewriting the agent interface. Finally, determinism
turns every past run into a regression fixture: because the same inputs reproduce the same
DOM, yesterday's successful scour becomes today's automated test, so faithfulness and control
never silently regress — the corpus of what worked becomes the corpus that guards the next
change, and the fleet grows the browser without eroding the guarantees the charter names.

## Metric
name: pct_requirements_green
kind: percent

## Serves
parent: root
how: This is a root mission of the Eidos body — the browser an agent can fully control. It serves the top goal of an autonomous agent that does real work on any interface, of which the web is the largest surface.

## Invariants

### own-the-engine
must: Control originates from owning the engine, never from puppeteering a human browser through a debug protocol; the agent control path must not depend on CDP, simulated coordinate input, or pixel-based clicking as the sole action model.
case: grep the control-path source for a CDP transport, coordinate/mouse-at-(x,y) click, or screenshot-click dependency as the only action path; the check fails if any appear as required for agent control.
irreversible: true

### agent-native-interface
must: The agent interface is the semantic DOM with stable node-ids and structured actions; reading or acting never requires pixels or screen coordinates.
case: run a scripted task end-to-end via the API with pixel/coordinate calls disabled; it must complete using only node-id and DOM operations.

### determinism
must: The same inputs produce the same result — settle is a guaranteed quiescence, not a timing heuristic.
case: run the same navigate+settle+snapshot twice on a fixed page; the two DOM snapshots must be identical.

### api-complete-control
must: Every product capability is reachable through the JSON API alone with zero human clicks; a human GUI is optional observation, never a required control path.
case: drive navigate, snapshot/view, click, knox_find, set_ai_vis (when live), and status solely via `chrime --api` or the listen socket; no mouse or keyboard interaction with a window is required for the sequence to complete.

### no-feature-popups
must: Chrime features never live behind dialogs, wizards, or multi-step pop-ups a human must navigate to change a setting; web-originated modal APIs are suppressed so they cannot block the agent loop.
case: feature toggles are one-shot chrome controls or JSON ops; page webviews override alert/confirm/prompt and deny window.open/download dialogs; grep GUI chrome for settings modals — none may gate a feature.

### gui-default-cosurf
must: The default release build includes the dual-pane GUI so a human can help the agent surf; API listen remains on by default in the GUI; lean headless remains available with `--no-default-features`.
case: `cargo build --release` enables feature `gui`; bare `chrime <url>` opens the window; `cargo tree --depth 1 --no-default-features` has no wry/winit.
irreversible: false

### lean-optional-core
must: A headless lean binary (ureq/scraper/serde/url only) can still be built without GUI for CI and pure-agent pipes.
case: `cargo build --release --no-default-features` succeeds; `chrime --api` works; `chrime --gui` exits with rebuild instructions.

### single-buffer-views
must: Multiple named views of a page are pure projections of one stored HTML buffer; switching views must not allocate a second full copy of the page.
case: after navigate, `view` ops for outline/links/meta report the same `html_bytes`; Meta returns empty nodes with counts; node-ids for the same element remain stable across views.

### secrets-never-surface
must: Credential values unlocked via Knox never appear in API JSON, logs, the side panel, or stdout; responses carry `secret_output: "suppressed"`.
case: run knox_find and knox_fill (or knox_use dry-run); responses include metadata and status only — no password field values in the JSON body or process logs.

### hierarchical-breadcrumbs
must: Every run, session, request, suite case, assert, and bug has a unique hierarchical id under `CHRIME.*` with no ambiguous naming; every API response includes `_trace.id` / `_trace.parent`; every event is append-logged with plain-English `english`.
case: After any `--api` call, response has `_trace.id` matching `CHRIME.RUN.*.SESS.*.REQ.*`; `logs/trace.jsonl` contains a line with that id; docs/BREADCRUMBS.md is the sole naming scheme.

### session-save-shim
must: Any existing session can be saved to disk (single HTML buffer + history + flags) and shimmed back into a later session via the API alone; lineage is explicit (`source_sess` / `shim_from` / SAVE+SHIM breadcrumbs).
case: navigate → session_save → (new process) session_load → current url and snapshot node_count match the saved page; response includes shim_from and into_sess.

### hancock-permissions
must: Chrime natively requests human permissions through Hancock (CLI) for consequential surf actions; the agent must not proceed on STILL_PENDING/QUEUED/DENIED; only APPROVED_AND_RAN (or AUTO_APPROVED_AND_RAN) is go.
case: `hancock_request` with action+why+risk returns a hancock_id and outcome; breadcrumbs under `…HANCOCK.req_*`; missing CLI returns HANCOCK_MISSING without fake approval.

## Requirements

### dom-snapshot-api
must: navigate and snapshot return the page as a semantic DOM with stable node-ids over a programmatic API.
case: pipe a navigate then snapshot command into `chrime --api`; every returned node carries a node_id.
status: green

### full-jsonl-api
must: A complete JSONL op surface exists on stdin/stdout (`--api`) and optionally TCP (`--listen`), covering navigation, inspection, action, views, Knox, AI visibility, and status.
case: `{"op":"help"}` lists the op set; a multi-op script completes navigate → view → find_text → click → current without a GUI.
status: green

### page-views
must: The current page supports named views — full, outline, links, fields, clickables, text, compact, meta — as API ops and (when GUI is built) as square tab controls.
case: after navigate to a fixed page, `view` kind=outline returns only headings; kind=meta returns empty nodes with counts and html_bytes; kind=links preserves stable node_ids usable by click.
status: green

### ai-visibility-marks
must: AI visibility mode paints Set-of-Mark boxes and numbers on live clickable targets so screenshot-using agents can see what is worth clicking; toggle and state are API-reachable.
case: with live surface + set_ai_vis on, marks appear on links/buttons/fields; toggle does not open a dialog; ai_marks reports a count.
status: green (gui feature)

### knox-credentials
must: Chrime integrates Knox for credential find/fill/use without printing secrets; preferred path is browser-fill into the live surface; fallback is knox CLI type/paste/dry-run.
case: knox_find returns title/login/url/id only; knox_fill injects into live fields without secret in the response; knox_use dry-run reports record match without typing.
status: green

### dual-pane-gui-default
must: Dual-pane desktop app (left = live render for human help, right = semantic/agent views) is the default product surface, fully driveable over the local JSONL listen port (and still usable with human clicks when the AI needs help).
case: default `cargo build --release` includes gui; bare `chrime <url>` opens dual-pane; listen on 127.0.0.1:7420; ping returns live:true.
status: green

### no-web-modals
must: Live page webviews suppress alert/confirm/prompt/print/showModalDialog, force same-tab navigation for window.open and target=_blank, and deny download save dialogs.
case: load a page that calls alert and window.open; neither blocks the agent nor opens a second window; download starts are denied.
status: green (gui feature)

### square-buttons
must: All Chrime chrome buttons, view tabs, and AI-vis mark badges are square (border-radius: 0); never rounded pills for controls.
case: grep gui chrome CSS for button/tab/c-num border-radius; all must be 0.
status: green (gui feature)

### memory-efficiency
must: Peak memory stays dominated by a single HTML buffer per page; views and snapshots are ephemeral; the binary optimizes for size (opt-level z, LTO, strip).
case: Meta view exposes html_bytes; default release binary stays on the order of ~2 MB without gui; views do not retain parallel full node caches across switches.
status: green

### api-suite-100
must: At least 100 plain-English tests of increasing complexity exist and can be executed by a subagent (or CI) against the JSON API; each test states intent in English and has machine-checkable asserts.
case: `python3 scripts/run_api_suite.py` runs ≥100 cases from `cases/api-suite.jsonl` and exits 0 on a healthy build.
status: green

### api-suite-bug-log
must: Suite failures are written to a durable bug log (`logs/api-bugs.jsonl`) including the plain-English statement, ops, and failure detail so a subagent can fix and re-run by case id.
case: A forced failing assert produces a JSONL bug entry; `run_api_suite.py --only <id>` re-checks that case after a fix.
status: green

### faithful-js
must: Chrime renders JS-rendered pages — the post-JS DOM, not the empty pre-JS shell.
case: navigate a known client-rendered page; the snapshot contains content that exists only after the page's JavaScript runs.
status: green (servo engine; suite cases 122-127, `--engine servo`; static engine sees only the shell — case 120 proves the fixture discriminates)

### auth-session
must: Chrime carries an authenticated session (cookies/credentials) across navigations.
case: authenticate to a test app, then fetch a protected page; the snapshot shows logged-in content, not the login wall.
status: green (servo; case 128 — cookie carried across navigations in-process; cases 131/132 — cookie jar persisted to `$CHRIME_PROFILE_DIR` and reloaded by a *different* process that never logs in; case 133 control — empty profile hits the login wall. Gap: flush needs a clean shutdown, a SIGKILL loses the jar.)

### gmail-scour
must: An agent scours a real authenticated Gmail account end-to-end — navigate, list, open, and extract from threads — driven entirely through the API.
case: run the agent scour against a live Gmail session; it returns structured data from at least five threads with zero pixel or coordinate calls.
status: open

### control-surfaces
must: Chrime exposes the control surfaces CDP cannot — deterministic settle, synchronous interception, and render-tree/computed-layout as data.
case: call the settle, intercept, and render-tree API ops; each returns real data (a settle receipt, an intercepted event, a layout-box tree), not an unsupported error.
status: open (1 of 3 — `settle` returns a real receipt with engine-loop spin counts, cases 119/125; interception and render-tree/computed-layout ops do not exist yet)

## Preferences
- Prefer Rust for the engine and core.
- Prefer forking/embedding a real Rust engine (Servo) as the starting substrate over from-scratch, closing web-compat gaps with a WPT-gated fleet.
- Prefer building with a coding-agent fleet, measured against web-platform-tests.
- Prefer a single Engine trait so the substrate can swap without changing the agent API.
- Embed a JS engine (v8/SpiderMonkey); never reimplement JavaScript.
- Prefer API-first: if a feature cannot be expressed as a JSON op, it is incomplete.
- Prefer browser-fill for credentials over OS type-into-frontmost (focus races).
- Prefer GUI-default for co-surf; lean headless via `--no-default-features`; `servo` stays feature-gated.
- Prefer square chrome controls; never rounded buttons.
- Prefer one HTML buffer + view projections over caching multiple full DOMs.
- Prefer suppressing web modals over asking a human to dismiss them.
- Prefer Knox for secrets; never log or return raw password values.
