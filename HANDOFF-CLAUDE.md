# Handoff for Claude CLI — Chrime engine depth

**From:** Grok Build session (product surface shipped)  
**Repo:** `/Users/dshanklinbv/repos-eidos-agi/chrime`  
**North star:** `ns_46cf50bfa273` — acceptance still **gmail-scour via API alone**  
**Read first:** `TELOS.md`, `REQUIREMENTS.md`, `docs/decisions/0001-engine-servo.md`, `docs/BREADCRUMBS.md`

---

## What is already done (do not re-litigate)

- Full JSONL API (`--api` / GUI listen `:7420`)
- Dual-pane GUI is **default** (`cargo build --release` includes `gui`)
- Page views, AI vis marks, Knox, Hancock, session save/shim
- Hierarchical breadcrumbs + 118 plain-English API suite
- Lean build: `--no-default-features`

**Verify green before you start:**
```sh
cd /Users/dshanklinbv/repos-eidos-agi/chrime
cargo test --release
python3 scripts/run_api_suite.py --quiet   # expect 118/118 (or current count)
```

---

## Your mission (highest impact)

Flip these TELOS requirements from **open → green**:

| ID | Must |
|----|------|
| **faithful-js** | Post-JS DOM in snapshot (not empty pre-JS shell) |
| **auth-session** | Cookies/session across navigations |
| **control-surfaces** | Real settle (not just HTTP fetch) — at least a settle receipt |

`gmail-scour` is later — do not start Gmail until the three above are honest.

---

## Concrete work packages

### WP1 — Servo engine builds and selects
- Feature already: `--features servo` (git pin in `Cargo.toml`)
- Make `cargo build --release --features servo` reliable on this machine
- `--engine servo` path works in `main` (already partially wired; finish + test)
- Document: `docs/servo-integration.md` (Engine ↔ libservo mapping)

### WP2 — `ServoEngine` implements full `Engine` trait
- File: `src/servo_engine.rs` (exists, incomplete)
- Same ops as StaticEngine: navigate, snapshot, click, links, find_text, view, export/import page
- Snapshot must be **post-JS** DOM (walker via evaluate_javascript)
- Settle: spin until load quiet / timeout with explicit settle metadata

### WP3 — API + suite for servo
- Add suite cases tagged `servo` (skip if feature not built)
- Case: navigate a known client-rendered page; assert content that only exists after JS
- Do not break StaticEngine suite (default binary)

### WP4 — Cookies / auth-session (minimum viable)
- Persist cookie jar with Servo (or document exact gap if blocked)
- Case: set cookie / login page fixture → second navigate still authenticated

### WP5 — Telos bookkeeping
- When a requirement is honestly green, update `TELOS.md` + `REQUIREMENTS.md` status
- Sync charter: copy TELOS → `.telos/north_stars/ns_46cf50bfa273/charter.md` and refresh `charter_hash`
- Append GOALS.md items as you complete them
- Failures → `logs/api-bugs.jsonl` via suite runner; fix by case id

---

## Constraints (hard)

1. **API-complete** — no new feature that only works via GUI clicks
2. **Breadcrumbs** — every new path logs under `CHRIME.*` (`docs/BREADCRUMBS.md`)
3. **Secrets** — never print Knox/Hancock secrets
4. **Lean default still builds** — GUI default ok; `--no-default-features` must stay green
5. **No fake green** — if servo only half-works, leave status open and write the gap

---

## Definition of done for this handoff

- [ ] `cargo build --release --features servo` succeeds
- [ ] `chrime --features… --engine servo --api` can navigate + snapshot post-JS content
- [ ] At least 5 new suite cases for servo (or skip_if cleanly)
- [ ] StaticEngine suite still 100% green
- [ ] TELOS statuses updated honestly
- [ ] Short note in `logs/` or GOALS.md: what blocked auth-session if incomplete

---

## Out of scope (leave for later)

- Gmail live scour
- Perfect pixel compositor
- Rewriting the GUI
- Changing Hancock/Knox product design

---

## How to work

```sh
cd /Users/dshanklinbv/repos-eidos-agi/chrime
# Prefer small commits. Run suite after each meaningful change.
python3 scripts/run_api_suite.py --only <ids>
```

When stuck twice on the same Servo crypto/build wall: document the exact error in `docs/servo-integration.md` and switch to the smallest path that still advances faithful-js (even a fixture-based settle proof), rather than thrashing.
