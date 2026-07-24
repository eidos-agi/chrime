# ADR 0001 — Engine substrate: Servo, embedded in-process

**Status:** Accepted (2026-07-24). Owner delegated the call with the priority "top performance, mostly."

## Decision

Chrime's v1 engine is **Servo (`servo` crate, MPL-2.0), embedded in-process** in the Rust
binary, behind the existing `Engine` trait. Servo's WebView delegate trait maps directly onto
that seam. Feature-gated (`--features servo`) so the default build stays lean (StaticEngine).

## Why (tuned to "top performance")

The performance killer is **the boundary, not the engine**:
- **CDP** puts a JSON-over-socket protocol between agent and browser — every op serializes.
  It is also forbidden by the telos invariant `own-the-engine`.
- **WKWebView** puts an async `evaluateJavaScript` round-trip through an Obj-C framework on the
  main thread — again, every op serializes; also mac-only and not owned.
- **Servo in-process has no boundary.** `snapshot` / `click` / `settle` become direct calls
  into the engine's own data structures. No serialization, no IPC, no protocol. That is the
  top-performance answer structurally, not by tuning.

It is also the only choice that is **telos-legal and deep**: in-process ownership satisfies
`own-the-engine`, and gives the render tree, computed layout, event loop, and clock as direct
data — which is the `control-surfaces` requirement CDP/WKWebView cannot meet. Servo is
Rust-native and parallel-by-design.

## Alternatives rejected

- **Chrome + CDP** — protocol boundary (slow) and forbidden by `own-the-engine`. Bridge only.
- **WKWebView (WebKit)** — async framework boundary, not owned, mac-only, no deep control
  surfaces. Would turn `faithful-js`/`auth-session`/`gmail-scour` green fast, but not top-perf
  and not the endgame.
- **From-scratch engine** — ultimate control/perf, but slowest; the fleet can build it later
  behind the same trait if Servo's ceiling is ever hit.

## Consequences / honest costs

- **Heavy build.** Servo cold-compiles are large (SpiderMonkey + system deps). Getting a
  working Gmail scour is a multi-session / fleet effort, not one turn. Owner accepted
  performance over speed-to-Gmail.
- **Web-compat gaps.** Servo is not 100% web-compatible; Gmail may need work. Closed by the
  WPT-gated coding-agent fleet (telos "How It Grows").
- **License.** `servo` is MPL-2.0 (file-level copyleft) — fine to embed; keep Servo
  modifications in their own files.
- **Seam.** Implement `Engine` for a `ServoEngine` via Servo's WebView delegate; `snapshot`
  walks Servo's live DOM directly (no JS round-trip); `settle` uses Servo's own load/quiescence
  signals for the deterministic settle the telos requires.

## Build status (2026-07-24): UNBLOCKED

Servo builds here. The wall was an upstream packaging bug: **crates.io `servo 0.4.0`
exact-pins `p256/p384/p521 = "=0.14.0-rc.14"`**, a broken RustCrypto release-candidate whose
`WnafSize` impl doesn't match the `primefield`/`elliptic-curve` finals that resolve alongside
it (`error[E0277]: the trait bound Scalar: WnafSize is not satisfied`). The `=` pin makes it
un-bumpable under that release. **Fix: depend on git Servo** (`rev = aa297ce5`), which uses
`p* 0.14.0` final. Full build — including `mozjs`/SpiderMonkey — completes in ~7m on rust
**1.88** (468 crates, 0 errors). Pin the rev; git main moves and could re-break. Revisit when
a fixed crates.io release ships.

## Next

1. ~~Confirm Servo builds~~ ✓ (see above).
2. `ServoEngine` skeleton behind the trait, mapped to the WebView delegate.
3. DOM snapshot from Servo's live tree; deterministic settle; then `auth-session` (cookies)
   → the `gmail-scour` acceptance test.
