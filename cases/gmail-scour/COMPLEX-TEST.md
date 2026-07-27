# Complex Gmail scour acceptance test (EID-1059 / gmail-scour)

## Intent

Prove Chrime can drive a **real authenticated Gmail session** through the API alone
(after a human completes login), and extract structured evidence for **six emails on
deliberately unrelated themes** — not six messages from the same cluster.

This is harder than example.com suite cases: SPA DOM, auth wall, human-in-the-loop login,
and theme diversity.

## Roles

| Role | Does |
|------|------|
| **Human** | Logs into Gmail in the headed Chrime window (password, 2FA, CAPTCHA). Never automated. |
| **Agent / runner** | Navigates, waits for inbox signal, searches/scours, scores six themes, writes report. |
| **Chrime** | Headed WebKit (live) + JSONL API on `:7420`/`:7421`. Uses `live_read` / `live_sync` / `live_eval`. |

## Zero-tolerance rules

- **No pixel/coordinate clicks.** No “click at (x,y)”.
- **No agent-owned 2FA.** If login stalls on challenge, status = `blocked_human_auth`.
- **Secrets never in report.** No passwords, no cookies, no Knox values.
- **Unrelated themes.** Filling all six slots with commerce receipts fails the diversity gate.

## Six required themes (must be different axes)

| id | Theme | Search cues (Gmail query language / keywords) |
|----|--------|-----------------------------------------------|
| T1 | **Commerce** | `subject:(order OR shipped OR delivery OR tracking OR receipt)` |
| T2 | **Security** | `subject:(security OR "sign-in" OR "signed in" OR 2FA OR password OR unusual OR verify)` |
| T3 | **Calendar** | `subject:(invitation OR invited OR meeting OR calendar OR RSVP OR "Zoom" OR "Google Meet")` |
| T4 | **Finance** | `subject:(invoice OR statement OR payment OR receipt OR bank OR tax OR wire OR payroll)` — prefer non-shipping |
| T5 | **Travel** | `subject:(flight OR boarding OR hotel OR itinerary OR booking OR Airbnb OR airline)` |
| T6 | **Social/product** | `subject:(newsletter OR digest OR "new feature" OR announcement OR "just shipped" OR unfollow)` — not pure commerce |

If a hit could score two themes, assign the **more specific** one and do not reuse that message id for a second theme.

## Protocol (machine steps)

### Phase 0 — Start

```text
chrime --listen 127.0.0.1:7421 https://mail.google.com/
# optional: collapse agent chrome for human login comfort
{"op":"sidebar","visible":false}
```

### Phase 1 — Human login (gate)

Poll until inbox OR timeout:

```json
{"op":"live_read"}
```

**Pass gate** when live text matches any of:

- `Inbox`
- `Primary`
- `Compose`
- `Search mail`

**Fail gate** (`blocked_human_auth`) if after `LOGIN_TIMEOUT_SECS` (default 300) still sees:

- `Sign in`
- `Forgot email`
- `Enter your password`
- `Verify it’s you`
- `2-Step Verification`

Human is prompted once: *“Log into Gmail in the Chrime window, then press Enter in the runner.”*

### Phase 2 — Live sync

```json
{"op":"live_sync"}
{"op":"status"}
```

Expect `node_count >= 1` and url containing `mail.google.com`.

### Phase 3 — Theme scour (six independent searches)

For each theme T1…T6:

1. Open Gmail search (prefer URL navigation — deterministic):

   `https://mail.google.com/mail/u/0/#search/<urlencoded query>`

2. `{"op":"wait","ms":2500}` then `{"op":"live_read"}` and `{"op":"live_sync"}`.

3. Extract **one** hit from live text / synced DOM:

   - `subject` (required)
   - `from` or `snippet` (at least one)
   - `evidence` = short raw line(s) supporting the theme
   - `message_key` = hash of subject+from to prevent double-count

4. If no hit: mark theme `miss` with last query + excerpt of live text (≤400 chars).

### Phase 4 — Score

| Gate | Pass condition |
|------|----------------|
| **auth** | Phase 1 gate passed |
| **coverage** | ≥6 themes with `hit` (telos said ≥5 threads; this complex bar is 6 themes) |
| **diversity** | No two hits share the same `message_key`; themes are six distinct ids |
| **api_only** | Ops used ⊆ allowed set below |
| **no_secrets** | Report JSON must not contain password-like fields |

Allowed ops: `ping`, `status`, `navigate`, `wait`, `live_read`, `live_eval`, `live_sync`,
`snapshot`, `find_text`, `query`, `read`, `view`, `sidebar`, `layout`, `settle`, `back`,
`forward`, `help`, `quit`.

Disallowed: any coordinate/pixel tool; OS GUI automation of Gmail chrome.

## Report shape

Written to `logs/gmail-scour-report.json` (and stdout summary):

```json
{
  "test": "gmail-scour-complex",
  "eid": "EID-1059",
  "started_at": "...",
  "finished_at": "...",
  "auth": "passed|blocked_human_auth|failed",
  "themes": {
    "commerce": {"status": "hit|miss", "subject": "...", "from": "...", "snippet": "...", "query": "..."},
    "security": { "...": "..." },
    "calendar": { "...": "..." },
    "finance": { "...": "..." },
    "travel": { "...": "..." },
    "social_product": { "...": "..." }
  },
  "hits": 0,
  "misses": 0,
  "passed": false,
  "ops_used": [],
  "notes": []
}
```

## Runner

```bash
# Terminal A — headed Chrime (human logs in when the window opens)
./target/release/chrime --listen 127.0.0.1:7421 https://mail.google.com/

# Terminal B — after you are in the inbox (or let the runner wait)
python3 scripts/run_gmail_scour.py --host 127.0.0.1 --port 7421
```

Flags:

- `--skip-login-prompt` — assume already authenticated; only poll for inbox markers
- `--login-timeout 300`
- `--report logs/gmail-scour-report.json`

## Why this is “complex”

1. Real third-party SPA (not example.com).
2. Human auth boundary (Hancock/Knox may assist password fill, but 2FA stays human).
3. Six **orthogonal** theme proofs, not N messages of one type.
4. Forces `live_*` path — StaticEngine alone cannot pass.
5. Produces durable evidence for telos `gmail-scour`.
