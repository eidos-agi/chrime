# Chrime API suite — plain-English tests for subagents

**100+ plain-English tests**, ordered by rising complexity, that exercise `chrime --api`.

| File | Role |
|------|------|
| `api-suite.jsonl` | The tests (english + ops + asserts) |
| `../scripts/generate_api_suite.py` | Regenerates the suite |
| `../scripts/run_api_suite.py` | Runs tests; writes bug log on failure |
| `../logs/api-bugs.jsonl` | Append-only failure log (gitignored) |
| `../logs/api-suite-report.json` | Latest full report |

## Subagent workflow

1. **Run the suite**
   ```sh
   cargo build --release
   python3 scripts/run_api_suite.py
   # or a slice:
   python3 scripts/run_api_suite.py --complexity 1-3
   python3 scripts/run_api_suite.py --only 12,44,91
   ```
2. **Read failures** (every event has a hierarchical id — see `docs/BREADCRUMBS.md`)
   - `logs/api-suite-report.json` — summary with `suite_trace_id` + per-case `trace_id`
   - `logs/api-bugs.jsonl` — failure rows with `id` = `CHRIME.SUITE….CASE….BUG….`
   - `logs/trace.jsonl` — every breadcrumb (`case_start`, `STEP`, `ASSERT`, `bug`, …)
3. **Fix** the API/engine using the breadcrumb id (no ambiguity).
4. **Re-run** only the failed ids until green:
   ```sh
   python3 scripts/run_api_suite.py --only 12,44
   ```
5. Optionally file to Linear via the repo `bugs` skill using the bug log body.

### Breadcrumb hierarchy (closed enum)

```
CHRIME.SUITE.<u…>.CASE.<n>
CHRIME.SUITE.<u…>.CASE.<n>.STEP.<ssss>
CHRIME.SUITE.<u…>.CASE.<n>.ASSERT.<aaa>
CHRIME.SUITE.<u…>.CASE.<n>.BUG.<b…>
```

API process uses:

```
CHRIME.RUN.<r…>.SESS.<sNNNN>.REQ.<rrrrrrrr>
```

Every API response includes `_trace.id` and `_trace.parent`. Full rules: **`docs/BREADCRUMBS.md`**.

## Case shape

```json
{
  "id": 42,
  "complexity": 5,
  "english": "Open example.com and request the outline view of the same page.",
  "ops": [{"op":"navigate","url":"https://example.com"},{"op":"view","kind":"outline"}],
  "assert": [{"on": -1, "path": "view", "eq": "outline"}],
  "tags": []
}
```

- **english** — what a human/subagent should understand and verify.
- **ops** — JSONL ops sent to `chrime --api` (executable truth).
- **assert** — machine checks (`eq`, `contains`, `gte`, `type`, `all_role_in`, …).
- **$find0 / $link0** — resolve `node_id` from prior `find_text` / `links` responses.

## Complexity ladder

| Level | Focus |
|------:|--------|
| 1 | ping, help, errors |
| 2 | navigate |
| 3 | snapshot, read |
| 4–5 | views |
| 6 | find_text, links |
| 7 | click, back |
| 8 | multi-step agent scripts |
| 9 | no_live / knox soft / safety |
| 10 | stability & end-to-end loops |

## Regenerating

```sh
python3 scripts/generate_api_suite.py
```
