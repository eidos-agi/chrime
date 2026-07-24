# 50 Goals — Chrime + the Fraude family

Autonomous push, 2026-07-24. `[x]` met · `[ ]` open · `[~]` in progress/blocked. Goals serve
the Chrime telos (`ns_46cf50bfa273`) and the Fraude family it belongs to.

## Chrime — v0 engine & API (pure Rust, no Servo)
1. [x] `navigate` + `snapshot` return semantic DOM with stable node-ids (dom-snapshot-api)
2. [ ] `forward` nav op (engine + API)
3. [ ] `query(selector)` op — CSS select, return matching node-ids
4. [x] `find_text(text)` op — node-ids whose text contains a substring
5. [x] `links` op — list every link (node_id, text, href)
6. [x] Consistent JSON error shape `{ok:false, code, error}` across all ops
7. [x] `--version` flag
8. [x] Unit test: `normalize()` url resolution (bare / relative / full / search)
9. [x] Unit test: `walk()`/snapshot on a fixed HTML fixture (roles + ids)
10. [ ] Integration test: pipe an API script, assert JSON results
11. [ ] Graceful non-HTML handling (content-type aware)
12. [ ] Expose `back`/`forward` in the API `handle`
13. [ ] Headed: show node-id beside each line (headed + API share addressing)
14. [ ] Request timeout config + document
15. [ ] README: full API table + examples

## Chrime — v1 (Servo) prep
16. [~] Servo build: 619 crates compile (webrender/html5ever/cssparser); BLOCKED only on
       broken RC crypto `p256/p384/p521 v0.14.0-rc.14` (`Scalar: WnafSize`). Fix = version pin.
       Not toolchain, not size — a bounded dep fix.
17. [ ] `rust-toolchain.toml` pinning 1.88 for the servo feature
18. [ ] `ServoEngine` module (feature-gated) implementing `Engine` via WebView delegate
19. [ ] `docs/servo-integration.md`: Engine ↔ libservo delegate mapping
20. [ ] `--engine static|servo` flag
21. [ ] cases/ — a runnable case script per telos requirement
22. [ ] Telos tick: report an iteration once a requirement flips green

## Chrime — quality
23. [x] GitHub Actions CI: build + test on push
24. [x] `cargo clippy` clean
25. [x] `cargo fmt` clean
26. [x] LICENSE (MIT for our code; Servo stays MPL in its own files)
27. [ ] CHANGELOG.md

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
36. [ ] Replace README ASCII banner with the current octopus render
37. [ ] Replace splash screenshot with the latest welcome
38. [ ] The "10 VHS examples" model — 10 tapes → 10 gifs
39. [ ] Welcome hero gif
40. [ ] Version gag (report a Claude-like build string)
41. [ ] Narrow-terminal collapse
42. [x] flickercheck detector + Ink `<Static>` fix (no more flicker)
43. [x] Section-aligned welcome + parody accomplices (Chrime/Gfail/Schemes/Extort)

## Cross-cutting — the family
44. [ ] "Fraude family" cross-links in all three READMEs
45. [ ] A family meta note (what each repo is)
46. [x] chrime repo set up, v0 shipped, pushed
47. [x] Fraude family branding on chrime (headed footer + README)

## Capture / governance
48. [x] Chrime telos registered (`ns_46cf50bfa273`) + charter committed
49. [x] ADR 0001: engine substrate = Servo
50. [ ] Devlog/brief in cockpit summarizing the ecosystem build
