# Goals — Chrime + the Fraude family

Autonomous push. `[x]` met · `[ ]` open · `[~]` in progress/blocked. Goals serve
the Chrime telos (`ns_46cf50bfa273`) and the Fraude family it belongs to.

**Telos / requirements:** `TELOS.md`, `REQUIREMENTS.md` (source of truth for musts).

## Chrime — v0 engine & API (pure Rust, no Servo)
1. [x] `navigate` + `snapshot` return semantic DOM with stable node-ids (dom-snapshot-api)
2. [x] `forward` nav op (engine + API)
3. [x] `query(selector)` op — CSS select, return matching node-ids
4. [x] `find_text(text)` op — node-ids whose text contains a substring
5. [x] `links` op — list every link (node_id, text, href)
6. [x] Consistent JSON error shape `{ok:false, code, error}` across all ops
7. [x] `--version` flag
8. [x] Unit test: `normalize()` url resolution (bare / relative / full / search)
9. [x] Unit test: `walk()`/snapshot on a fixed HTML fixture (roles + ids)
10. [x] Integration test: pipe an API script, assert JSON results (`api_pipe_script_asserts_json_results`)
11. [x] Graceful non-HTML handling (content-type aware) — `content_kind` + wrap for agents
12. [x] Expose `back` + `forward` in the API
13. [ ] Headed: show node-id beside each line (headed + API share addressing)
14. [x] Request timeout config + document (`CHRIME_TIMEOUT_SECS`, default 30)
15. [x] README: API table + views + Knox + dependency policy

## Chrime — product surface (2026-07-25) → TELOS
51. [x] Full JSONL API (`--api` / `--listen`) — zero human clicks (api-complete-control)
52. [x] Page views: full/outline/links/fields/clickables/text/compact/meta (page-views, single-buffer-views)
53. [x] AI visibility Set-of-Marks on live clickables (ai-visibility-marks)
54. [x] Knox find/fill/use; secrets never surface (knox-credentials, secrets-never-surface)
55. [x] Dual-pane GUI is the **default** build (co-surf; dual-pane-gui-default)
55b. [x] Adaptive pane layout (auto side/stack, page majority ~68%) — not permanent 50/50 phone column (EID-1057)
56. [x] No feature pop-ups; suppress web modals (no-feature-popups, no-web-modals)
57. [x] Square buttons only (square-buttons)
58. [x] Lean headless still available via `--no-default-features` (lean-optional-core)
59. [x] Memory: one HTML buffer + ephemeral views; ~2 MB lean binary (memory-efficiency)
60. [x] TELOS + REQUIREMENTS.md updated with all of the above
61. [x] ≥100 plain-English API tests + runner + bug log (`cases/`, `scripts/run_api_suite.py`)
62. [x] Hierarchical breadcrumbs on every event (`docs/BREADCRUMBS.md`, `logs/trace.jsonl`)
63. [x] Session save + shim restore (`session_save` / `session_load`)
64. [x] Native Hancock permission requests (`hancock_request` / wait / pending)

## Chrime — v1 (Servo) prep
16. [x] Servo builds (incl. mozjs/SpiderMonkey) — fix: git servo rev aa297ce5 drops the broken RC crypto pin (crates.io 0.4.0 exact-pinned p*=0.14.0-rc.14). 468 crates, 7m11s.
       broken RC crypto `p256/p384/p521 v0.14.0-rc.14` (`Scalar: WnafSize`). Fix = version pin.
       Not toolchain, not size — a bounded dep fix.
17. [ ] `rust-toolchain.toml` pinning 1.88 for the servo feature
18. [x] `ServoEngine` module (feature-gated) implementing `Engine` via WebView delegate
19. [x] `docs/servo-integration.md`: Engine ↔ libservo delegate mapping
20. [x] `--engine static|servo` flag
21. [ ] cases/ — a runnable case script per telos requirement
22. [ ] Telos tick: report an iteration once a requirement flips green

## Chrime — quality
23. [x] GitHub Actions CI: build + test on push
24. [x] `cargo clippy` clean
25. [x] `cargo fmt` clean
26. [x] LICENSE (MIT for our code; Servo stays MPL in its own files)
27. [x] CHANGELOG.md

## Fraude OS — complete the suite
28. [ ] Gfail GUI (inbox + compose) + tools `gfail_compose`, `gfail_list_inbox`
29. [ ] Schemes GUI (channels + messages) + tools `schemes_send`, `schemes_list`
30. [ ] Extort GUI (grid) + tools `extort_set_cell`, `extort_get_cell`, `extort_sum`
31. [ ] OS skins actually re-theme (Lienux / LackOS / Winblows)
32. [ ] vitest: a WebMCP round-trip (register → executeTool → assert)
33. [ ] README screenshot/gif of the desktop
34. [ ] Tools panel lists the full suite across all apps
35. [x] fraude-os repo set up, builds green, pushed

## Fraude Code — deferred + requested
36. [x] Replace README ASCII banner with the current octopus render
37. [x] Replace splash screenshot with the latest welcome
38. [ ] The "10 VHS examples" model — 10 tapes → 10 gifs
39. [x] Welcome hero gif
40. [ ] Version gag (report a Claude-like build string)
41. [ ] Narrow-terminal collapse
42. [x] flickercheck detector + Ink `<Static>` fix (no more flicker)
43. [x] Section-aligned welcome + parody accomplices (Chrime/Gfail/Schemes/Extort)

## Cross-cutting — the family
44. [x] "Fraude family" cross-links in all three READMEs
45. [ ] A family meta note (what each repo is)
46. [x] chrime repo set up, v0 shipped, pushed
47. [x] Fraude family branding on chrime (headed footer + README)

## Capture / governance
48. [x] Chrime telos registered (`ns_46cf50bfa273`) + charter committed
49. [x] ADR 0001: engine substrate = Servo
50. [x] Devlog/brief in cockpit summarizing the ecosystem build

## v1 engine — Servo depth (2026-07-25)
51. [x] `cargo build --release --features servo` green on this machine (7m13s, rustc 1.96)
52. [x] `ServoEngine` completes the `Engine` trait (settle, html_bytes, export/import page)
53. [x] `settle` API op + `SettleReceipt` (spins/ms/quiescent) — deterministic settle, not a sleep
54. [x] faithful-js green: post-JS DOM, JS-created nodes, JS click handlers (cases 122-127)
55. [x] auth-session green in-process: cookies carried across navigations (case 128)
56. [x] Suite runner `--engine` flag, servo/static-only tag skipping, fixture web server
57. [x] `docs/servo-integration.md` — Engine ↔ libservo mapping + the two engine traps
58. [x] Cookie jar persisted to disk — `$CHRIME_PROFILE_DIR` (default `logs/profile`); cases 131/132 + control 133
59. [ ] Interception + render-tree/computed-layout ops (rest of control-surfaces)
60. [x] Determinism case: double navigate+settle+snapshot equality (suite #144)
