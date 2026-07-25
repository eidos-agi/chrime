# Chrime breadcrumb IDs — hierarchy for AIs (no ambiguity)

Every event in Chrime gets a **unique, hierarchical id**. There is exactly one way to read
it. Left = root. Right = leaf. Dots separate segments. No spaces. No synonyms.

## Shape (always)

```
CHRIME . RUN.<run> . SESS.<sess> . REQ.<req> [ . STEP.<step> ] [ . ASSERT.<n> ] [ . BUG.<bug> ]
```

| Segment | Meaning | Value format | Example |
|---------|---------|--------------|---------|
| `CHRIME` | Product root (constant) | literal | `CHRIME` |
| `RUN` | One process lifetime | `r` + 10 base36 chars | `RUN.rK7m2p9q1a` |
| `SESS` | One API client/session | `s` + 4 zero-pad decimal | `SESS.s0001` |
| `REQ` | One JSONL request → one response | 8 zero-pad decimal | `REQ.00000042` |
| `STEP` | One op inside a multi-op suite case | 4 zero-pad decimal | `STEP.0003` |
| `ASSERT` | One machine assert on a response | 3 zero-pad decimal | `ASSERT.002` |
| `BUG` | One failure log entry | `b` + 10 base36 | `BUG.bH4n8w2x0c` |
| `SUITE` | One suite invocation (runner) | `u` + 10 base36 | `SUITE.uM3k…` |
| `CASE` | One plain-English test case | decimal case id | `CASE.64` |
| `SAVE` | One session snapshot written to disk | stem / timestamp | `SAVE.my_sess_1720…` |
| `SHIM` | One session restore into the current SESS | ms timestamp | `SHIM.1721944800123` |
| `HANCOCK` | Human permission request via Hancock | `req_…` or `pending` | `HANCOCK.req_1785…` |

## Full examples

```
CHRIME.RUN.rK7m2p9q1a
CHRIME.RUN.rK7m2p9q1a.SESS.s0001
CHRIME.RUN.rK7m2p9q1a.SESS.s0001.REQ.00000007
CHRIME.SUITE.uM3k9p2q1a.CASE.64
CHRIME.SUITE.uM3k9p2q1a.CASE.64.STEP.0002
CHRIME.SUITE.uM3k9p2q1a.CASE.64.ASSERT.001
CHRIME.SUITE.uM3k9p2q1a.CASE.64.BUG.bH4n8w2x0c
```

## Rules (AI must follow)

1. **Parent** of an id = the same string with the **last `.SEGMENT.value` removed**.
2. **Never reuse** a `REQ` number inside the same `SESS`.
3. **Never invent** segment names. Only: `CHRIME RUN SESS REQ STEP ASSERT BUG SUITE CASE`.
4. **Correlation**: every API JSON response includes `_trace.id` and `_trace.parent` matching the request that produced it.
5. **Secrets**: breadcrumb `data` never contains password/secret values (keys redacted).
6. **English**: every log line has an `english` field — one plain sentence of what happened.
7. **Logs**: append-only JSONL at `logs/trace.jsonl` (and suite bugs at `logs/api-bugs.jsonl` with the same ids).

## Log line schema (every breadcrumb)

```json
{
  "id": "CHRIME.RUN.r….SESS.s0001.REQ.00000007",
  "parent": "CHRIME.RUN.r….SESS.s0001",
  "kind": "request|response|error|session_start|run_start|suite_start|case_start|case_end|assert|bug",
  "ts": "2026-07-25T22:00:00.000Z",
  "ts_ms": 1721944800000,
  "english": "Request #7: navigate to https://example.com",
  "op": "navigate",
  "ok": true,
  "data": { }
}
```

| Field | Required | Meaning |
|-------|----------|---------|
| `id` | yes | Full hierarchical id |
| `parent` | yes | Immediate parent id (`null` only for `CHRIME.RUN.*` run_start) |
| `kind` | yes | Event type (closed enum above) |
| `ts` | yes | ISO-8601 UTC |
| `ts_ms` | yes | Unix ms |
| `english` | yes | Plain English, no jargon overload |
| `op` | when request/response | API op name |
| `ok` | when response/error/case_end | success flag |
| `data` | optional | Small structured payload (redacted) |

## Kind enum (closed — do not invent)

| kind | When |
|------|------|
| `run_start` | Process starts |
| `session_start` | New API session (stdio or TCP client) |
| `request` | JSONL line received |
| `response` | JSONL line sent |
| `error` | Parse/dispatch failure |
| `suite_start` | API suite runner begins |
| `case_start` | One plain-English case begins |
| `case_end` | Case finished (pass or fail) |
| `assert` | One assert checked |
| `bug` | Failure written to bug log |
| `session_save` | Session blob written under `logs/sessions/` |
| `session_shim` | Saved blob loaded into current SESS |
| `hancock_request` | Queued a human permission request |
| `hancock_wait` | Polled/blocked on a Hancock request outcome |
| `hancock_pending` | Listed the human signing tray |

## How a subagent uses this

1. Read `logs/trace.jsonl` filtered by `id` prefix `CHRIME.RUN.<run>`.
2. Walk children by `parent == that id`.
3. On failure, open `logs/api-bugs.jsonl` and match `trace_id` / `id`.
4. Re-run `python3 scripts/run_api_suite.py --only <CASE.n>` after fix.

There is no second naming scheme. If it is not in this document, it is not a valid breadcrumb.

## Session save / shim lineage

```
# while working in a session
CHRIME.RUN.r….SESS.s0001.REQ.00000010     # navigate, click, …
CHRIME.RUN.r….SESS.s0001.SAVE.my_work_… # session_save

# later, new process/session
CHRIME.RUN.rNEWER.SESS.s0001
CHRIME.RUN.rNEWER.SESS.s0001.SHIM.1721…   # session_load — shim_from points at saved id
```

The **saved file** stores `source_sess` + `source_run` + the single HTML buffer + history.
The **live session** after load is a new SESS (new breadcrumbs) with `shim_from` in the
response so AIs never confuse “where I am” with “where this came from.”

## Hancock permission lineage

```
CHRIME.RUN.r….SESS.s0001.HANCOCK.req_1785…
CHRIME.RUN.r….SESS.s0001.HANCOCK.req_1785….WAIT
```

Outcomes (closed — do not invent):

| outcome | Meaning for the agent |
|---------|------------------------|
| `APPROVED_AND_RAN` | Human signed; proceed with the Chrime action |
| `AUTO_APPROVED_AND_RAN` | Local Hancock license auto-ran; proceed |
| `QUEUED` | Waiting for human; call `hancock_wait` — **not** approval |
| `STILL_PENDING` | Human has not decided; wait again — **not** approval |
| `DENIED` / `EXPIRED` | Do **not** proceed |
| `HANCOCK_MISSING` | CLI not installed; cannot ask the human this way |

Never treat absence of a field or a non-zero wait as approval.
