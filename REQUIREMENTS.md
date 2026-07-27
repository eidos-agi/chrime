# Chrime — Requirements

Source of truth for product requirements. Maps to `TELOS.md` / north star `ns_46cf50bfa273`.

**Product in one line:** a real browser AIs can steer — including ~100 sessions at once.

Status: **green** | **open** | **stub-only** (allowed for CI, not product green).

---

## Invariants (never violate)

| ID | Must | Case | Status |
|----|------|------|--------|
| **own-the-engine** | No CDP / coordinate-only / screenshot-click as permanent sole agent path | Grep agent path | green (path clean); depth open |
| **one-engine-of-record** | Agent snapshot/click = that session’s real post-JS document | SPA mutation visible to agent ops | open |
| **agent-native-interface** | Semantic DOM + node-ids; no pixels required | API-only script | green (stub); re-prove on real engine |
| **fleet-sessions** | Session is the unit; design for ~100 concurrent steers | Multi-session isolation + budget | open |
| **determinism** | Settle = quiescence; same inputs → same tree when we own clock | Double navigate+settle+snapshot | open (suite #144 partial on stub) |
| **api-complete-control** | Every capability via agent API; GUI optional attach | Multi-op without human clicks | green (single-session) |
| **no-feature-popups** | No feature wizards; suppress web modals | API/chrome toggles only | green |
| **human-attach-optional** | Human view attachable; not product identity | Agent works with no window | open (product still window-centric) |
| **single-buffer-views** | Views project one buffer | Same html_bytes; stable ids | green |
| **secrets-never-surface** | Knox secrets never in API/logs/UI | knox_* responses | green |
| **hierarchical-breadcrumbs** | `CHRIME.*` ids + english + trace log | `_trace.id` | green |
| **session-save-shim** | Save/restore session + lineage | session_save/load | green |
| **hancock-permissions** | Hancock; STILL_PENDING ≠ go | hancock_* | green |

---

## Core product

| ID | Must | Case | Status |
|----|------|------|--------|
| **real-browser-engine** | Production engine runs JS + real document/network | Post-JS snapshot on fixture | open (Servo partial; Static = stub-only) |
| **dom-snapshot-api** | navigate/settle/snapshot → node-ids | Multi-op script | green on stub |
| **full-jsonl-api** | Full op surface; per-session as fleet matures | help + multi-op | green single-session |
| **fleet-100** | ≥100 concurrent steered sessions (design + smoke) | N isolated sessions | open |
| **page-views** | full/outline/links/fields/clickables/text/compact/meta | view ops | green |
| **faithful-js** | Agent DOM is post-JS on engine of record | JS fixture | open product |
| **auth-session** | Cookies across nav + process (profile) | login → protected; jar reload | open product |
| **gmail-scour** | Real Gmail via API on engine of record; ≥5 threads (complex: 6 themes) | cases/gmail-scour | open |
| **control-surfaces** | settle + intercept + layout-as-data | real receipts/data | open (settle partial) |
| **memory-efficiency** | Per-session budget toward fleet-100 | documented + measured | open |
| **api-suite-100** | ≥100 plain-English tests + bug log | run_api_suite.py | green (stub/CI) |
| **knox-credentials** | Knox without secret echo | knox_* | green |
| **ai-visibility-marks** | Optional SoM on attach surface | set_ai_vis | green (GUI) |
| **no-web-modals** | Suppress blocking web UI | alert/open denied | green (GUI) |

---

## Anti-requirements (not product green)

| Anti | Why |
|------|-----|
| StaticEngine + WKWebView dual truth for one agent session | one-engine-of-record |
| Dual-pane co-surf as product identity | fleet-sessions + human-attach-optional |
| Headless static scrape claiming Gmail/SPA green | real-browser-engine / faithful-js |
| Blocking UI thread for WebView eval to answer agent API | fleet-sessions / api design |
| CDP as long-term control path | own-the-engine |

---

## API ops (current single-session surface)

```
ping, help, status,
navigate, back, forward, current, snapshot, view, views, read, links, find_text, query, click, settle,
fill, type, press, eval, live_eval, live_read, live_sync,
knox_find, knox_fill, knox_use,
session_save, session_load, session_list, session_delete,
hancock_request, hancock_wait, hancock_pending,
set_ai_vis, toggle_ai_vis, ai_marks,
layout, sidebar,
wait, quit
```

`live_*` ops are transitional bridges; long-term agent control uses the **session engine of
record**, not a second document. See TELOS anti-requirements.

### Hancock

| op | What |
|----|------|
| `hancock_request` | Ask human; only APPROVED_AND_RAN / AUTO_APPROVED_AND_RAN = go |
| `hancock_wait` / `hancock_pending` | Poll; STILL_PENDING is not approval |

### Session save / shim

| op | What |
|----|------|
| `session_save` / `session_load` / `session_list` / `session_delete` | Disk session + lineage |

Env: `CHRIME_SESSIONS_DIR` (default `logs/sessions`), `CHRIME_PROFILE_DIR` (cookie jar),
`CHRIME_TIMEOUT_SECS` (static stub HTTP timeout).

### Suite

```sh
# CI stub engine (not product headed):
cargo build --release --features headless
python3 scripts/run_api_suite.py --chrime ./target/release/chrime
```

Product builds are **headed-only** (no windowless `--api` unless `--features headless`).

### Gmail complex acceptance

See `cases/gmail-scour/COMPLEX-TEST.md` and `scripts/run_gmail_scour.py` (EID-1059). Requires
engine of record + real session — not StaticEngine alone.

---

## Preferences

See TELOS.md Preferences. Summary: real engine, fleet sessions, API-first, honest stubs,
Knox/Hancock, settle-and-snapshot, WPT growth.
