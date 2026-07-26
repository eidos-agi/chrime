# Chrime — Requirements

Source of truth for product requirements. Each item maps to a TELOS requirement or
invariant (`TELOS.md` / north star `ns_46cf50bfa273`). Status: **green** | **open** | **n/a**.

---

## Invariants (never violate)

| ID | Must | Case (falsify) | Status |
|----|------|----------------|--------|
| **own-the-engine** | No CDP / coordinate-only / screenshot-click as the agent control path | Grep agent path for CDP transport or mouse-at-(x,y) as required | green (static path) |
| **agent-native-interface** | Semantic DOM + stable node-ids; no pixels required to act | API-only script completes without pixel/coordinate ops | green |
| **determinism** | Same inputs → same settled snapshot | Double navigate+snapshot on fixed fixture; equal trees | open (settle primitive exists — `settle` op; no double-run equality case yet) |
| **api-complete-control** | Every capability via JSON API; zero human clicks required | Multi-op script: navigate → view → click → knox_find → status | green |
| **no-feature-popups** | No dialogs/wizards to change features; suppress web modals | Features are API ops or always-visible chrome; no settings modal | green |
| **gui-default-cosurf** | Default build includes dual-pane GUI (human helps AI surf) | `cargo build --release` has `gui`; bare `chrime url` opens window | green |
| **lean-optional-core** | Headless lean build still available | `--no-default-features` has no wry; `--api` works | green |
| **single-buffer-views** | Views are projections of one HTML buffer | Same `html_bytes` across views; Meta empties nodes | green |
| **secrets-never-surface** | Knox secrets never in API/logs/UI | knox_find/fill responses have no password values | green |
| **hierarchical-breadcrumbs** | Every event has unique `CHRIME.*` id + english; API `_trace`; append `logs/trace.jsonl` | Response `_trace.id` matches hierarchy; see `docs/BREADCRUMBS.md` | green |
| **session-save-shim** | Save session to disk; load/shim into a later SESS with full lineage | `session_save` → `session_load` restores url + HTML + history | green |
| **hancock-permissions** | Native Hancock ask-human; STILL_PENDING ≠ approval | `hancock_request` / `hancock_wait` / `hancock_pending` | green |

---

## Core engine & API

| ID | Must | Case | Status |
|----|------|------|--------|
| **dom-snapshot-api** | `navigate` + `snapshot` → semantic DOM with node_ids | `chrime --api` pipe | green |
| **full-jsonl-api** | Complete JSONL surface: stdio `--api` and optional `--listen` | `help` lists ops; multi-op script works | green |
| **page-views** | Views: full, outline, links, fields, clickables, text, compact, meta | `view` kind=outline/meta/links; stable node_ids | green |
| **find-text-links** | `find_text`, `links` ops | Substring match + link list | green |
| **click-by-node-id** | `click` follows node_id (href on static engine) | click after find_text | green |
| **back-nav** | `back` in API with history | navigate A→B→back → A | green |
| **memory-efficiency** | One HTML buffer; ephemeral views; size-optimized release | Meta `html_bytes`; ~2 MB lean binary | green |

### API ops (complete set)

```
ping, help, status,
navigate, back, current, snapshot, view, views, read, links, find_text, click, settle,
fill, type, press, eval,
knox_find, knox_fill, knox_use,
session_save, session_load, session_list, session_delete,
hancock_request, hancock_wait, hancock_pending,
set_ai_vis, toggle_ai_vis, ai_marks,
wait, quit
```

### Hancock (ask Daniel to sign)

| op | What |
|----|------|
| `hancock_request` | `{ "action":"knox_fill", "why":"…", "risk":"high", "wait":true }` → blocks until human signs (or STILL_PENDING) |
| `hancock_wait` | `{ "id":"req_…" }` — re-block; **not** approval until APPROVED_AND_RAN |
| `hancock_pending` | Text tray of what is waiting on the human |

Only outcomes `APPROVED_AND_RAN` / `AUTO_APPROVED_AND_RAN` mean proceed. Env: `HANCOCK_BIN`.

### Session save / shim

| op | What |
|----|------|
| `session_save` | `{ "name": "my-work" }` → writes `logs/sessions/<stem>.json` (HTML + history + ai_vis + source_sess) |
| `session_load` | `{ "id": "…" }` or `{ "name": "my-work" }` → shims into **current** SESS; returns `shim_from` / `into_sess` |
| `session_list` | Metadata only (no full HTML dump) |
| `session_delete` | Remove a saved file by id/name |

Env: `CHRIME_SESSIONS_DIR` (default `logs/sessions`).

### Plain-English API suite (≥100 tests)

| ID | Must | Case | Status |
|----|------|------|--------|
| **api-suite-100** | ≥100 plain-English tests of increasing complexity, machine-runnable against `--api` | `python3 scripts/run_api_suite.py` exits 0 | green |
| **api-suite-bug-log** | Failures append to `logs/api-bugs.jsonl` with english + ops + failures for subagent fix loops | Fail a case → bug line written; re-run `--only <id>` | green |

```sh
python3 scripts/generate_api_suite.py   # regenerate cases/api-suite.jsonl
python3 scripts/run_api_suite.py        # full suite → logs/api-suite-report.json
python3 scripts/run_api_suite.py --only 12,44
python3 scripts/run_api_suite.py --complexity 1-4
```

Subagent loop: run suite → read `logs/api-bugs.jsonl` → fix → re-run failed ids.

---

## Credentials (Knox)

| ID | Must | Case | Status |
|----|------|------|--------|
| **knox-credentials** | Find/fill/use via Knox; secrets never printed | knox_find metadata only; fill injects live; dry-run safe | green |
| **knox-browser-fill** | Prefer inject into live WebView over OS type-into-frontmost | knox_fill with live:true | green (gui) |
| **knox-cli-fallback** | knox_use type/paste/dry-run without secret on stdout | knox_use dry-run | green |

---

## GUI (default) — human + AI co-surf

| ID | Must | Case | Status |
|----|------|------|--------|
| **dual-pane-gui-default** | Left live render (for you), right agent views; default product | `cargo build --release`; `chrime url` opens dual-pane | green |
| **ai-visibility-marks** | Set-of-Mark boxes on live clickables; API toggle | set_ai_vis on → marks; no dialog | green |
| **no-web-modals** | Suppress alert/confirm/prompt; deny popups/downloads | Init script + new_window Deny | green |
| **square-buttons** | border-radius: 0 on buttons, tabs, AI badges | Grep CSS | green |
| **gui-api-listen** | Full API on 127.0.0.1:7420 while GUI runs | nc ping live:true | green |
| **view-tabs** | Right pane tabs switch projections without second page store | Click Outline/Meta tabs (or API set_view) | green |

---

## v1 engine (Servo path — `--features servo`, `--engine servo`)

| ID | Must | Case | Status |
|----|------|------|--------|
| **servo-build** | `cargo build --release --features servo` succeeds and `--engine servo` selects it | Build green (7m13s, rustc 1.96); `ping` reports `engine: servo` (case 121) | green |
| **faithful-js** | Post-JS DOM, not empty shell | Cases 122–127: JS-set title, JS-created nodes, JS-appended link, JS click handler | green |
| **auth-session** | Cookies/session across navigations **and across processes** | Case 128 (in-process); cases 131/132 (jar written on clean shutdown, reloaded by a fresh process); case 133 control (empty profile → login wall) | green (SIGKILL still loses the jar) |
| **gmail-scour** | End-to-end Gmail via API only | ≥5 threads structured extract | open |
| **control-surfaces** | Settle, intercept, render-tree as data | `settle` returns a real receipt (spins/ms/quiescent) — cases 119/125 | open (1 of 3: no intercept, no render-tree) |

Details, the mapping to libservo, and the two engine traps: `docs/servo-integration.md`.

```sh
cargo build --release --features servo
python3 scripts/run_api_suite.py --engine servo --tag servo   # servo cases
python3 scripts/run_api_suite.py                              # static; servo cases skipped by tag
```

---

## Dependency policy

| Build | What you get |
|-------|----------------|
| **`cargo build --release`** (default) | GUI dual-pane + API (wry/winit + core) — co-surf with a human |
| **`--no-default-features`** | Lean core only (ureq, scraper, url, serde) — CI / pure agent pipe |
| **`--features servo`** | Full JS engine (heavy, optional) |

```sh
cargo build --release                      # DEFAULT: GUI co-surf
cargo build --release --no-default-features  # lean headless
./target/release/chrime https://cnn.com    # opens dual-pane + :7420 API
./target/release/chrime --api              # headless JSONL (even in GUI build)
```

---

## Acceptance north star

**Gmail scour through the API alone** (requirement `gmail-scour`) remains the long-horizon
acceptance case. Everything above either enables that path or protects leanness,
determinism, and agent-first control until the Servo substrate lands.

---

## Traceability

| Doc | Role |
|-----|------|
| `TELOS.md` | Full charter + invariants + requirements (this project's telos) |
| `.telos/north_stars/ns_46cf50bfa273/charter.md` | Registered north-star charter (synced) |
| `REQUIREMENTS.md` | This checklist |
| `DESIGN.md` | Design rationale |
| `GOALS.md` | Session/goal board |
| `README.md` | Operator surface |
