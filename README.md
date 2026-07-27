# chrime

> 🎭 **A member of the [Fraude family](https://github.com/eidos-agi/fraude-code)** — alongside
> `fraude-code` and the Fraude OS apps (Chrime · Gfail · Schemes · Extort). The family joke is
> that the brand is a costume but the work is real. Chrime is the most real of the bunch.

**A browser built for AI agents, not humans.** No GUI, no pixel pipeline. The interface is an
API; what it exposes is the DOM as a compact semantic tree with stable node-ids — the thing an
agent's decision loop actually needs.

**Telos / requirements:** [`TELOS.md`](TELOS.md) · [`REQUIREMENTS.md`](REQUIREMENTS.md) ·
north star `ns_46cf50bfa273`.

**API test suite (100+ plain English):** [`cases/`](cases/) — run with
`python3 scripts/run_api_suite.py` (failures → `logs/api-bugs.jsonl` for subagent fix loops).

**Breadcrumbs (every event, unique hierarchical ids):** [`docs/BREADCRUMBS.md`](docs/BREADCRUMBS.md) —
every API response has `_trace.id`; full trail in `logs/trace.jsonl`.

**Session save / shim:**
```sh
# save
{"op":"session_save","name":"my-work"}
# later (even a new process)
{"op":"session_load","name":"my-work"}   # shims HTML+history into current SESS
{"op":"session_list"}
```
Files under `logs/sessions/` (override with `CHRIME_SESSIONS_DIR`).

**Hancock (ask me to sign):**
```sh
{"op":"hancock_request","action":"knox_fill","why":"Log into bank","risk":"high","wait":true}
{"op":"hancock_wait","id":"req_…"}      # STILL_PENDING is NOT approval
{"op":"hancock_pending"}
```
Only `APPROVED_AND_RAN` / `AUTO_APPROVED_AND_RAN` means go. GUI chrome has a **Hancock** button (queues without freezing the window).

Existing automation tools are bad at agent control because they puppeteer a *human* browser
(pixels, coordinates, flaky selectors). Chrime inverts that: the machine-native surface is
primary. And it's a **settle-and-snapshot** engine, not a real-time one — an agent never
watches a stream of frames, so Chrime drops the compositor/vsync/animation loop that eats a
human browser's memory. See [DESIGN.md](DESIGN.md).

Today (v0): fetch + parse real HTML (Servo's html5ever) — **1.9 MB binary, ~7.6 MB RSS**.
No JS yet (that's v1, embedded v8).

## Run

```sh
# DEFAULT — dual-pane GUI (you help the AI surf) + API on 127.0.0.1:7420
cargo build --release
./target/release/chrime https://www.cnn.com

# Drive the open window with zero mouse clicks (API)
printf '%s\n' '{"op":"ping"}' '{"op":"status"}' | nc -w 2 127.0.0.1 7420

# Headless API only (no window) — same binary
printf '%s\n' \
  '{"op":"navigate","url":"https://example.com"}' \
  '{"op":"snapshot"}' | ./target/release/chrime --api

# Terminal DOM view (no WebView window)
./target/release/chrime --tui https://example.com

# Lean binary (no wry/winit) for CI / pure agent
cargo build --release --no-default-features
```

### Dependency policy

| Build | What |
|-------|------|
| **default** | GUI co-surf + core (wry/winit) — **normal product** |
| `--no-default-features` | Lean core only (ureq, scraper, serde, url) |
| `--features servo` | Full JS engine (heavy) — run it with `--engine servo` |

With `--engine servo` the page's JavaScript actually runs, and the login sticks: cookies are
kept in a profile dir (`CHRIME_PROFILE_DIR`, default `logs/profile`) and reloaded next launch,
so a fresh process starts already authenticated. That dir holds live session cookies — treat it
like a credential store. The jar is flushed on clean shutdown, so end a session with
`{"op":"quit","force":true}` (or EOF) rather than killing the process. See
`docs/servo-integration.md`.

## Fully API-driven (no clicks)

Every capability is a JSON op. Humans may use the chrome buttons; agents never need to.

| op | purpose |
|----|---------|
| `navigate` / `back` / `forward` / `current` | go places (forward stack survives `back`) |
| `snapshot` / `view` / `views` / `read` / `links` / `find_text` / `query` | see the page (CSS `query` keeps stable node-ids) |
| `click` | follow a node_id (not a mouse) |
| `settle` | drive the engine to quiescence; returns a receipt (`spins`, `ms`, `quiescent`), never a sleep |
| `fill` / `type` / `press` / `eval` | live form control (GUI / live surface) |
| `knox_find` / `knox_fill` / `knox_use` | credentials (secrets never returned) |
| `set_ai_vis` / `ai_marks` | AI visibility overlay |
| `status` / `ping` / `help` | introspection |
| `wait` | timed pause |

### Page views (one buffer, many lenses)

The engine stores **one** HTML buffer per page. Views are ephemeral filters — no second copy:

| kind | what you get |
|------|----------------|
| `full` | all semantic nodes |
| `outline` | headings only |
| `links` | links with href |
| `fields` | form fields / labels |
| `clickables` | links + buttons |
| `text` | paragraph / list text |
| `compact` | headings + acts + fields, truncated text |
| `meta` | role counts + `html_bytes` only (empty nodes) |

```sh
printf '%s\n' \
  '{"op":"navigate","url":"https://example.com"}' \
  '{"op":"view","kind":"outline"}' \
  '{"op":"view","kind":"meta"}' | ./target/release/chrime --api
```

GUI: tab bar on the right pane switches the same projection. Node-ids stay stable across views.

Drive the default GUI without touching it:

```sh
# terminal A
./target/release/chrime https://example.com

# terminal B — zero clicks
printf '%s\n' \
  '{"op":"ping"}' \
  '{"op":"status"}' \
  '{"op":"set_ai_vis","on":true}' \
  '{"op":"navigate","url":"https://news.ycombinator.com"}' \
  '{"op":"snapshot"}' | nc -q1 127.0.0.1 7420
```

The desktop app is a real window (WebKit left, semantic DOM right). The API is complete
without it for navigate/snapshot/click/knox_find; live fill/knox_fill need the GUI
listener (default).

**AI visibility mode** (on by default — toggle with the **AI vis** button in the chrome bar)
paints orange Set-of-Mark boxes + numbers on every live clickable in the left pane
(links, buttons, inputs, roles, etc.). Screenshot-using agents can see/draw on things
worth clicking; marks update on scroll, resize, and DOM mutation.

## Knox (passwords)

Chrime talks to [Knox](https://github.com/eidos-agi) directly for credentials. Secrets are
**never** printed in the API, the side panel, or logs.

| Surface | How |
|--------|-----|
| **GUI** | **Knox** button (or right-pane Find) → match list (title/login/url only) → **Fill both** injects login+password into the live left pane via Touch ID unlock |
| **API** | `knox_find` / `knox_use` (see below) |

```sh
# metadata only
printf '%s\n' '{"op":"knox_find","query":"github.com"}' | ./target/release/chrime --api

# type/paste via Knox CLI (no secret on stdout) — focus the field first
printf '%s\n' '{"op":"knox_use","query":"github.com","field":"password","via":"type-frontmost","target_app":"chrime"}' \
  | ./target/release/chrime --api
```

Browser-fill (inject into the DOM, not OS keystroke) is the GUI path — that is the safe one
for agent browsers. `knox_use` wraps Knox's approved frontmost type/paste fallback.

## No pop-ups for features

Chrime features are never hidden behind dialogs a human must navigate:

- **AI vis / Knox / Read** — always-visible chrome buttons (one click).
- **API** — `set_ai_vis`, `knox_find`, `knox_use`, … (no UI required).
- **Web** — `alert`/`confirm`/`prompt` suppressed; `window.open` and `target=_blank` load
  in the main pane; downloads denied (no save picker).

The only OS dialog you may still see is **Knox Touch ID** when unlocking secrets — that is
Knox's boundary, not a Chrime settings popup.

## API

| op | args | returns |
|----|------|---------|
| `navigate` | `url` | nav result (ok, url, status, title) |
| `back` / `forward` | — | history stack; `status.forward_len` shows what's available |
| `snapshot` | — | the semantic DOM: nodes with `node_id`, `role`, `text`, `href`, `clickable` |
| `read` | — | full page text |
| `links` | — | every link on the page (`node_id`, `text`, `href`) |
| `find_text` | `text` | nodes whose text contains the substring — how an agent finds "the login button" |
| `query` | `selector` | CSS select → `{ok, count, nodes}`; semantic matches keep stable `node_id`s |
| `click` | `node_id` | follows the node's link (v0: href only; v1 will run JS handlers) |
| `current` | — | current URL |

Same commands back both interfaces, through one `Engine` trait — so a future v8 engine drops
in behind the identical API. That seam is the whole point.

MIT.
