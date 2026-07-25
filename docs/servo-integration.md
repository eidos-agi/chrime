# Servo integration — Engine ↔ libservo

How Chrime's `Engine` trait (`src/main.rs`) maps onto embedded Servo (`src/servo_engine.rs`),
what is real today, and what is still a gap. ADR: `docs/decisions/0001-engine-servo.md`.

## Build

```sh
cargo build --release --features servo     # GUI default + servo engine
cargo check  --features servo              # fast loop (~90s warm)
./target/release/chrime --api --engine servo
```

- Pin: `servo = { git = "https://github.com/servo/servo", rev = "aa297ce5" }`.
  crates.io `servo 0.4.0` exact-pins broken RustCrypto RCs (`p256/p384/p521 = "=0.14.0-rc.14"`)
  and does not compile — see ADR. Do not un-pin the rev casually; git main moves.
- Toolchain verified here: rustc **1.96.0** (Homebrew) and 1.88 (per ADR).
- Both builds write `target/release/chrime`, so a later `cargo build --release` **overwrites**
  the servo binary with a static-only one and `--engine servo` starts refusing. Rebuild with the
  feature, or keep them apart with `--target-dir target/servo`.
- Engine choice is runtime, not compile-time-only: `--engine servo` vs default `static`.
  Without the feature, `--engine servo` exits with rebuild instructions rather than
  silently falling back — a wrong-engine run must never look like a right one.

## Mapping

| `Engine` method | libservo |
|---|---|
| `ServoEngine::new` | `ServoBuilder` + `SoftwareRenderingContext` (headless, no window, no GPU) + `WebViewBuilder` |
| `navigate` | `WebView::load`, then spin until `WebView::url()` is the target (commit), then load-complete |
| `settle` | `Servo::spin_event_loop` until `LoadStatus::Complete`; returns `SettleReceipt { spins, ms, quiescent, reason }` |
| `snapshot` | `WebView::evaluate_javascript(WALKER)` — walks the **live post-JS DOM** and emits StaticEngine's exact node schema |
| `read_text` | `document.body.innerText` |
| `click` | `el.click()` on the node_id-th interesting element, then settle — runs the page's real handlers |
| `links` / `find_text` | filters over `snapshot()` (same projection rules as StaticEngine) |
| `html_bytes` / `export_page` | `document.documentElement.outerHTML` — the serialized post-JS document is the single buffer |
| `import_page` | writes the saved buffer to a temp file and `WebView::load`s it as `file://` (no network); `current_url` reports the *saved* url, not the scratch path |
| cookies | Servo's own resource/cookie store, per `Servo` instance — nothing in Chrime touches it |

### Why JS-evaluated walk, not a direct DOM walk

`evaluate_javascript` runs **in-process, inside the engine's own script thread** — there is no
protocol, no socket, no external debugger. It is not CDP-shaped: nothing here depends on a
debug port, and the agent never touches pixels or coordinates (telos `own-the-engine`,
`agent-native-interface`). A direct Rust-side walk of Servo's DOM would avoid the JS round trip
entirely; that is a performance refinement, not a control-model change, and it needs Servo's
`script` internals exposed across the crate boundary. Filed as a gap below, not a blocker.

### Settle is the point

`spin()` pumps the engine's event loop and **counts the turns**. That count is what makes the
settle receipt evidence: a sleep cannot report spins, and a poller outside the loop cannot
either. Cap is `SPIN_CAP = 30_000` turns; hitting it reports `quiescent: false, reason: "cap"`
rather than claiming success.

## Verified

Run the servo cases (they are skipped on a static binary, never silently passed):

```sh
python3 scripts/run_api_suite.py --engine servo --tag servo
python3 scripts/run_api_suite.py                        # static: 119 cases, servo ones skipped
```

Fixtures: `cases/fixtures/js-render.html` (content that exists only after JS) and the cookie
fixture server the runner starts on `127.0.0.1:7431` (`/login` sets a cookie, `/protected`
shows logged-in vs. wall).

See "Status" at the end of this file for what actually passes today.

## The two traps that cost the most (do not undo these)

1. **A webview must be shown, focused, and pumped before it can navigate.** Right after
   `WebViewBuilder::build()` the constellation has no browsing context yet. `load()` is
   accepted and silently dropped — Servo logs `LoadUrl for unknown browsing context` (visible
   only with `RUST_LOG=warn`). `ServoEngine::new` therefore calls `show()` + `focus()` and then
   spins until the initial `about:blank` load *completes*.
2. **`load_status()` lies across a navigation.** Immediately after `load()` it still reports
   `Complete` from the previous document, so a settle that trusts it returns instantly and every
   snapshot reads the old page (`url: "about:blank"`, `node_count: 0`) while claiming success.
   The delegate counts *load completions* instead; `navigate` records the count before the load
   and waits for it to advance. A navigation that never completes returns `ok: false`, not a
   silently stale page.

Both failures look like success from the outside — that is exactly why the servo suite cases
assert post-JS content rather than `ok: true`.

## Known gaps

- **Cookie persistence to disk.** Cookies live in the `Servo` instance; a fresh process starts
  cold. `auth-session` is only honestly green for the in-process case until a jar is persisted.
- **DOM walk goes through JS.** See above — correctness is fine, the in-process boundary is
  intact, but a native walk would be faster.
- **Interception and render-tree/computed-layout** are not exposed yet — `control-surfaces`
  needs those two beyond the settle receipt.
- **Web-compat.** Servo is not fully web compatible; heavy apps (Gmail) are unproven here.

## Status (2026-07-25)

`cargo build --release --features servo` — **green** (7m13s warm, rustc 1.96, 69 MB binary).
Servo cases **10/10 green** (`--engine servo --tag servo`), static suite **120/120 green**
(119 + 2 new engine cases, servo cases skipped by tag).

| Telos requirement | Status after this pass |
|---|---|
| `faithful-js` | **green** — post-JS title, post-JS DOM nodes, JS-appended links, and JS-created click handlers all observed through the API (cases 122–127) |
| `auth-session` | **green (in-process)** — cookie set on `/login` is carried to `/protected` across a separate navigate (case 128). Cross-process persistence is still a gap. |
| `control-surfaces` | **open** — settle is real and receipted (cases 119/125); synchronous interception and render-tree/computed-layout are not exposed yet |
| `determinism` | **open** — settle exists now, but no double-run snapshot-equality case yet |
