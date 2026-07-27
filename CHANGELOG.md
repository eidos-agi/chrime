# Changelog

## 0.1.0 — 2026-07-27

### Added
- Collapsible agent **sidebar** (chrome `Sidebar · on/off` + `{"op":"sidebar","visible":false}`) — full-width live page when collapsed (EID-1058)
- `forward` history stack after `back`; `status.forward_len`
- `query` CSS selector op (`selector` / `css` / `q`) with stable semantic `node_id`s
- `layout` dual-pane geometry: `auto` | `side` | `stack` + `page_ratio` (GUI); chrome Layout button
- POSIX CLI: `--help` / `-h` / `help` exits without opening the GUI
- Graceful non-HTML: Content-Type aware navigate (`content_kind`: `html` | `non_html`)
- Request timeout via `CHRIME_TIMEOUT_SECS` (default 30s, clamp 1–600)
- Session save/load, Hancock, Knox, settle receipt, Servo engine (feature), cookie jar profile
- ≥140 plain-English API suite cases under `cases/`

### Changed
- Default dual-pane is no longer permanent 50/50 phone column: page majority ~68%, auto side/stack
- Default window size 1600×1000 landscape

### Docs
- README API table updated for forward / query / layout
- `docs/servo-integration.md`, ADR 0001, TELOS / REQUIREMENTS
