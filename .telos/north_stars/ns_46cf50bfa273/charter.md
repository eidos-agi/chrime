# Chrime — Telos

## Philosophy

We believe control is an ownership property, not a protocol feature. An agent that must
reason and act on the web needs to control every layer of the browser — the DOM, the event
pipeline, layout, and the clock — and that degree of control exists only if you own the
engine. The browser was built for a human watching pixels sixty times a second; an agent
never watches, it acts in a request/response loop. So the machine-native surface must be
primary and the human surface must be absent. What stays true when everything else changes:
the interface an agent drives is the semantic DOM with stable identifiers; the engine is a
settle-and-snapshot machine, not a real-time one; and any capability that cannot be reached
deterministically and programmatically does not exist for our purposes. Own the engine,
expose its internals, and make the agent the first-class actor rather than a guest poking at
a debug port from outside the loop.

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
  intelligence.
- **A pixel-perfect from-scratch renderer as step one** — insufficient because painting at
  frame rates is the most expensive and least necessary subsystem for an agent that never
  watches; leading with it burns the budget on the wrong thing and delays the DOM-first
  control that actually matters.
- **Embed a real engine but keep a human-style API on top** — insufficient because it leaves
  the agent puppeteering again; owning the engine pays off only if the interface is
  agent-native (semantic DOM, deterministic settle, exposed internals). Engine and interface
  are separable, and the interface is the point.

## The Unique Offer

Chrime offers what nothing else can: total, deterministic, fine-grained control of a browser,
exposed as an agent-native interface. The clock is ours — settle is a guaranteed primitive,
animations fast-forward to their end state, timers are capped, and the same inputs produce the
same result. Interception is synchronous at any layer — every request, mutation, style
recalc, and layout pass is a first-class hook, not a polled event. The render tree and
computed layout are queryable data, not reconstructed guesses. The action model is first-class:
"operate on node N" carries an engine-guaranteed post-condition instead of a simulated mouse
event. And because we own the engine, there is no detection surface and no capability the
protocol refuses to open — the agent is the browser, not a guest inside it. That combination
is unavailable from any human-browser-plus-protocol, by construction, which is why owning the
engine is the mission rather than an optimization.

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
must: Control originates from owning the engine, never from puppeteering a human browser through a debug protocol; the agent control path must not depend on CDP, simulated coordinate input, or pixel-based clicking.
case: grep the control-path source for a CDP transport, coordinate/mouse-at-(x,y) click, or screenshot-click dependency; the check fails if any appear in the agent action path.
irreversible: true

### agent-native-interface
must: The agent interface is the semantic DOM with stable node-ids and structured actions; reading or acting never requires pixels or screen coordinates.
case: run a scripted task end-to-end via the API with pixel/coordinate calls disabled; it must complete using only node-id and DOM operations.

### determinism
must: The same inputs produce the same result — settle is a guaranteed quiescence, not a timing heuristic.
case: run the same navigate+settle+snapshot twice on a fixed page; the two DOM snapshots must be identical.

## Requirements

### dom-snapshot-api
must: navigate and snapshot return the page as a semantic DOM with stable node-ids over a programmatic API.
case: pipe a navigate then snapshot command into `chrime --api`; every returned node carries a node_id.

### faithful-js
must: Chrime renders JS-rendered pages — the post-JS DOM, not the empty pre-JS shell.
case: navigate a known client-rendered page; the snapshot contains content that exists only after the page's JavaScript runs.

### auth-session
must: Chrime carries an authenticated session (cookies/credentials) across navigations.
case: authenticate to a test app, then fetch a protected page; the snapshot shows logged-in content, not the login wall.

### gmail-scour
must: An agent scours a real authenticated Gmail account end-to-end — navigate, list, open, and extract from threads — driven entirely through the API.
case: run the agent scour against a live Gmail session; it returns structured data from at least five threads with zero pixel or coordinate calls.

### control-surfaces
must: Chrime exposes the control surfaces CDP cannot — deterministic settle, synchronous interception, and render-tree/computed-layout as data.
case: call the settle, intercept, and render-tree API ops; each returns real data (a settle receipt, an intercepted event, a layout-box tree), not an unsupported error.

## Preferences
- Prefer Rust for the engine and core.
- Prefer forking/embedding a real Rust engine (Servo) as the starting substrate over from-scratch, closing web-compat gaps with a WPT-gated fleet.
- Prefer building with a coding-agent fleet, measured against web-platform-tests.
- Prefer a single Engine trait so the substrate can swap without changing the agent API.
- Embed a JS engine (v8/SpiderMonkey); never reimplement JavaScript.
