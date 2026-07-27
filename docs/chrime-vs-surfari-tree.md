# Can Chrime replace Surfari, and on what schedule?

A decomposition, not an answer. Produced under `THE-DEPTH.md` (eidos-philosophy) for
EID-1038. Every node states a claim, the evidence that settles it, and a confidence
target from THE-CONTRACT's ladder (`proven` / `tested` / `researched` / `inferred` /
`unknown` / `blocked`). Parents roll up at the **floor**, not the average.

Nothing below is an answer to the root question. Where a node is already settled by
evidence gathered on 2026-07-26, it is marked and cited. Everything else is open.

---

## R — Root

**Claim.** For each workload Surfari serves today, there exists either (a) a Chrime
cutover date backed by a named green gate, or (b) an explicit "not on this horizon" with
the blocking reason.

**Contract (written before execution).** Passes when every workload in A1 is assigned to
(a) or (b), and every date in (a) cites a gate that is green with a red history. Fails on
any workload unassigned, or any date backed by an estimate rather than a gate.

**Why this is not a leaf.** The pass condition above is writable, but the pass condition
of "can Chrime replace Surfari" is not — "replace" has no meaning until the workload set
exists. That is the split trigger. R decomposes into four independent branches plus one
roll-up.

**Confidence today: `unknown`.** Floor comes from B0.

```
R  can Chrime replace Surfari, on what schedule
├── A  DEMAND      — what must a replacement actually do
├── B  SUPPLY      — what Chrime actually does today
├── C  FLOOR       — does either survive the targets' defenses
├── D  OWNERSHIP   — what each engine costs to keep
└── E  SCHEDULE    — roll-up of A..D  (not a research node)
```

---

## A — DEMAND: what must a replacement actually do?

**Claim.** The replacement surface is the set of Surfari commands actually invoked by
live consumers against live targets — not Surfari's published surface.

**Confidence target: `tested`.** Current: `inferred` (A1 partial).

### A1 — Enumerate live consumers and what each calls · LEAF

- **Claim.** The set of systems that break if Surfari is removed is exactly {…}.
- **Evidence needed.** `grep -rl "agent-browser"` across `~/repos-*` excluding the
  surfari repo itself; cross-check against `boss.db` mission records; confirm each hit is
  live rather than vendored/docs by checking last-modified and whether the call path is
  reachable.
- **Afternoon?** Yes.
- **Partial result already in hand (2026-07-26):** hits at
  `cockpit-eidos/tools/agent-browser/`, `cockpit-eidos/tools/browser-control/`,
  `manyhats/docker-hat/agent-browser-mcp/server.py`, `felix/felix/core.py`,
  `eidosomni/tools/learning-browser/`, `ai-cockpit/tools/learning-browser/`.
  Plus **Dally**, which is not a repo hit: `boss.db` learning 100 records a live reverse
  tunnel, MacBook Surfari Chrome `:9223` → mac-mini `:19222` via `ssh -R`, verified by
  matching browser UUID on `/json/version`.
- **Correction to the brief.** `~/repos-personal/tally-downloader` does **not** use
  Surfari. `pyproject.toml` depends on `playwright>=1.40.0` directly. Tally is not a
  consumer; removing Surfari does not touch it.
- **Confidence: `inferred`** — the grep is done, the liveness check is not.

### A2 — Reduce the published surface to the invoked surface · LEAF

- **Claim.** Of Surfari's command surface, only N commands are ever invoked in
  production, and the replacement must cover those N.
- **Evidence needed.** Static: extract the literal `agent-browser <cmd>` invocations from
  each A1 call site. Dynamic: Surfari's session/daemon logs for one week.
- **Afternoon?** Yes for static; the dynamic half needs a week of wall-clock but no
  attention.
- **Why this node exists.** `README.md` contains 308 `agent-browser …` invocations. That
  number is documentation surface, not demand. Sizing the replacement against 308 would
  make the answer "no" by arithmetic rather than by evidence.
- **Confidence: `unknown`.**

### A3 — Which workloads need a human-authenticated session · LEAF

- **Claim.** Workloads split into {anonymous fetch} and {requires a logged-in human
  session}, and only the second set is exposed to session/fingerprint risk.
- **Evidence needed.** Consumer configs plus the on-disk profile:
  `~/.local/share/surfari-chrome/Default/` exists and carries real profile state
  (`first_party_sets.db`, `heavy_ad_intervention_opt_out.db` observed) — enumerate which
  origins have live cookies.
- **Afternoon?** Yes.
- **Confidence: `unknown`.**

### A4 — Which workloads are irreversible · LEAF

- **Claim.** The subset that moves money or sends is gated by Hancock regardless of
  engine, so engine choice does not change its risk posture.
- **Evidence needed.** Trace each A1 consumer's action set for writes/sends; check
  whether the gate is engine-side or caller-side.
- **Afternoon?** Yes.
- **Confidence: `unknown`.**

---

## B — SUPPLY: what does Chrime actually do today?

**Claim.** Chrime's real capability is what its suite proves on a fresh run, not what
`GOALS.md` asserts.

**Confidence target: `tested`.** Current: **`unknown`** — and this floor propagates all
the way to R.

### B0 — Resolve the contradiction in Chrime's own records · LEAF · **BLOCKING**

- **Claim.** Exactly one of the three records is true about the Servo engine's state.
- **The contradiction, as of 2026-07-26.** Three artifacts in one repo, two answers:

  | Record | Says |
  |---|---|
  | `GOALS.md:94` | `[x] faithful-js green: post-JS DOM, JS-created nodes, JS click handlers (cases 122-127)` |
  | `GOALS.md:95` | `[x] auth-session green in-process: cookies carried across navigations (case 128)` |
  | `GOALS.md:98` | `[x] Cookie jar persisted to disk … cases 131/132 + control 133` |
  | `logs/api-suite-report.json` (2026-07-26T03:19:42Z) | ran **1** case — 132 — `"passed": false`. Asserted `text contains "LOGGED IN AS AGENT"`, got `'LOGIN WALL'` |
  | `BOSS-STATUS.md` | "faithful-js fixture: navigate OK, **snapshot still empty** (not green)"; scoreboard `faithful-js post-JS snapshot: red / in progress`; `auth-session: not started` |
  | `EID-966` (updated 2026-07-25) | headless load returns `title: None, node_count: 0` for `https://example.com` — "NOT file:// specific" |

- **Evidence needed.** One full run of all 133 cases (`cases/api-suite.jsonl`) against
  both engines via `scripts/run_api_suite.py`, report committed. Not a re-read of
  `GOALS.md`.
- **Afternoon?** Yes. This is the cheapest node in the tree.
- **Why it is blocking.** Under the MLTRL floor rule, every downstream Chrime claim
  inherits this node's confidence. While B0 is `unknown`, B is `unknown`, and therefore R
  is `unknown`. **No schedule can be honestly stated until this one afternoon is spent.**
- **This is the finding the tree bought.** A flat answer would have taken `GOALS.md` at
  its word — it is the file that looks authoritative — and produced a phased roadmap on
  top of a green that the repo's own last test run contradicts.
- **Confidence: `unknown`.**

### B1 — EID-966: does headless Servo load a page at all · LEAF

- **Claim.** `ServoEngine` returns a non-empty DOM for `https://example.com` headless.
- **Evidence needed.** The repro already written into EID-966:
  `printf '{"op":"navigate","url":"https://example.com/"}\n{"op":"snapshot"}\n{"op":"quit"}' | ./target/debug/chrime --engine servo --api`
  returning `node_count > 0`. EID-966 already names the suspect list (show/resize/focus
  before navigate; `notify_new_frame_ready` → `paint()` cycle; `evaluate_javascript`
  resolving against an empty pipeline) and the reference (`components/servo/tests/common/mod.rs`).
- **Afternoon?** Yes — the hypothesis set is already enumerated; this is one focused pass.
- **Depends on:** nothing. **Blocks:** B2, B4, all of C2.
- **Confidence: `unknown`.**

### B2 — Servo coverage on the actual targets · LEAF (per target)

- **Claim.** For target T, Chrime-servo's DOM is equivalent to Surfari's for the
  selectors the consumer actually uses.
- **Evidence needed.** For each T in A1/A3: snapshot in both engines, diff node count and
  the specific selectors the consumer depends on. Not "does Servo pass web-platform-tests."
- **Afternoon?** Yes, per target, once B1 is green.
- **Why scoped this way.** "Is Servo's coverage sufficient" has no writable pass
  condition — sufficient for what? Scoped to a named target and a named selector set, it
  does. This is the split trigger doing real work.
- **Confidence: `blocked`** on B1.

### B3 — Command-surface delta · LEAF

- **Claim.** The set of A2 commands with no Chrime equivalent is exactly {…}.
- **Evidence needed.** Mechanical diff of the A2 list against Chrime's op list. Chrime
  self-declares **33 ops** in the `"help" | "ops"` arm of `src/api.rs:109-122` (e37bd53):
  ping, help, status, navigate, back, current, snapshot, view, views, read, links,
  find_text, click, settle, fill, type, press, knox_find/fill/use,
  session_save/load/list/delete, hancock_request/wait/pending,
  set_ai_vis/toggle_ai_vis/ai_marks, eval, wait, quit.
- **Afternoon?** Yes.
- **Caveat worth one line, not a node.** That list is *self-declared* and does not match
  the dispatch arms one-for-one (23 arms at the top match level; names like
  `hancock_request` and `session_save` do not appear as arms verbatim). Verifying that
  each declared op actually dispatches is part of this same afternoon, not a child node —
  same evidence artifact.
- **Known gaps already visible from Surfari's surface:** cookies, network route/intercept,
  download, drag, geo/device emulation, clipboard, dialog accept/dismiss, iframe
  switching, screenshot/annotate, CDP attach, batch. `GOALS.md:99` lists interception +
  render-tree/computed-layout as open.
- **Do not decompose per-command.** Thirty sibling nodes sharing one diff command and one
  contract is enumeration, not depth. (See THE-DEPTH.md, shared-evidence signature.)
- **Confidence: `inferred`** — the Chrime side is counted, the A2 side does not exist yet.

### B4 — Session survives a fresh process · LEAF

- **Claim.** A process that never logged in reads a protected page from the on-disk jar.
- **Evidence needed.** Case 132 green, with its red history intact (it is red right now —
  that red history is an asset, not a problem; it satisfies `done_is_earned_not_asserted`).
- **Afternoon?** Yes.
- **Confidence: `unknown`** — currently observed failing, which is a *result*, not a gap.

---

## C — FLOOR: does either engine survive the targets' defenses?

**Claim.** Bot defenses are a property of each target, and they set a floor that engine
choice may or may not clear.

**Confidence target: `tested`.** Current: `inferred`.

### C0 — Correct the premise before decomposing · SETTLED

- **The brief asserted** that `boss.db` records that headless trips Cloudflare.
- **It does not.** FTS over `~/.grok/plugin-data/grok-boss/boss.db` `learnings_fts` for
  `cloudflare OR captcha OR challenge OR bot OR fingerprint OR browser` returns nothing on
  bot detection. The Cloudflare rows that exist (148, 149) are about DNS for jetta-sso.
- **The record exists elsewhere, and says something stronger.**
  - `cerebro-builder-mcp/learnings.json` **L014** — Browserbase needed
    `browserSettings.fingerprint` because the UA read `HeadlessChrome` and Cloudflare
    verification failed. Headless does trip Cloudflare. `tested`.
  - `reeves-finance/tools/learning-browser/sites/empower-retirement.com/cloudflare-blocked.md`
    — **2026-04-05, agent-browser with `--headed --profile`** — real Chrome, real profile,
    a human clicking the CAPTCHA — looped infinitely on "Just a moment…" and never
    resolved. **Surfari is already blocked on this target.** `tested`.
  - omni memory, jetta-operating — plain `python-urllib` UA → Cloudflare error 1010.
    `tested`.
- **What this does to the tree.** Cloudflare is not a Chrime-vs-Surfari discriminator on
  every target. On at least one target it is a floor *both* engines are under, which
  means C cannot be modeled as "Surfari passes, Chrime must catch up." It must be modeled
  per target. Had this been assumed rather than checked, the whole C branch would have
  been shaped wrong — and the shape of a branch is not something a reader can audit from
  the leaves.
- **Confidence: `tested`.**

### C1 — Per-target challenge baseline · LEAF (per target)

- **Claim.** Target T challenges / does not challenge Surfari headed+profile today.
- **Evidence needed.** Load each A1/A3 target with `agent-browser --headed --profile`,
  record outcome and screenshot. This is the baseline every Chrime comparison is against.
- **Afternoon?** Yes for the full target set.
- **Confidence: `inferred`** — one datum (Empower, blocked) exists; the rest untested.

### C2 — Does Servo's signature trip challenges Chrome's does not · LEAF

- **Claim.** Chrime-servo's TLS fingerprint and JS environment are distinguishable from
  Chrome in ways the targets in C1 act on.
- **Evidence needed.** Both engines against a fingerprint reflector (echo the TLS
  ClientHello / JA3 plus `navigator` surface), then both against one C1 target known to
  challenge. Two artifacts: the reflector diff, and the pass/fail on the live target.
- **Afternoon?** Yes, once B1 is green.
- **Confidence: `blocked`** on B1.

### C3 — Is the gap fixable or structural · **STOP — not decomposable into leaves**

- **Claim.** A Servo embedding can be made indistinguishable from Chrome for the checks
  the targets actually run.
- **Why this node stops here.** The checks are adversarial and undisclosed, and they
  change. Any decomposition of this claim produces leaves that all return `inferred` while
  the tree *looks* rigorous — the exact failure THE-DEPTH.md warns about. Splitting it
  would manufacture confidence, not find it.
- **Honest state: `unknown`.**
- **The experiment that would change it, named rather than pretended:** run Chrime-servo
  against the C1 target set for a fixed window and measure the challenge rate versus
  Surfari's. That is weeks of wall-clock against live third parties, not an afternoon, and
  it can only be run after B1 and C2. It is a program, not a leaf. Marking it as such is
  the finding.
- **Consequence for R.** Any schedule that assumes C3 resolves favorably is an estimate,
  not a gate, and fails R's contract.

### C4 — Does session inheritance matter independently of fingerprint · LEAF

- **Claim.** Chrime can import a live Chrome cookie jar and reach an authenticated page,
  making the human-session advantage transferable rather than Surfari-exclusive.
- **Evidence needed.** Export cookies from `~/.local/share/surfari-chrome/Default`, import
  via `$CHRIME_PROFILE_DIR` / `shim_session`, hit one authenticated origin from A3.
- **Afternoon?** Yes.
- **Why separate from C2.** Fingerprint and session are commonly conflated. They fail
  independently: an engine can carry a perfect session and still be fingerprinted, or pass
  fingerprinting with no session. Separating them is what makes each checkable.
- **Confidence: `blocked`** on B1.

---

## D — OWNERSHIP: what does each engine cost to keep?

**Confidence target: `tested`.** Current: `inferred`.

### D1 — Surfari's carrying cost · LEAF

- **Claim.** Surfari costs approximately one `agent-browser upgrade` per release.
- **Evidence.** It is upstream OSS — `vercel-labs/agent-browser`, currently **0.27.1**
  (`package.json`). `AGENTS.md` documents CI that builds 7 platform binaries, publishes to
  npm, and cuts the GitHub release automatically on version bump. Maintenance is
  *consumed*, not *performed*.
- **Afternoon?** Already done. **Confidence: `tested`.**

### D2 — Chrime's carrying cost · LEAF

- **Claim.** Chrime's Servo embedding requires N hours per Servo rev bump, and rev bumps
  are forced at rate R.
- **Evidence needed.** One attempted bump off the pinned rev, timed. Chrime currently pins
  git servo `aa297ce5` specifically to dodge a broken RC crypto pin
  (`p256/p384/p521 v0.14.0-rc.14`, `Scalar: WnafSize`) — `GOALS.md:42`. Known figures:
  468 crates, 7m11s debug / 7m13s release build (rustc 1.96), ~70 MB release binary
  (`GOALS.md:42,91`; `BOSS-STATUS.md`).
- **Afternoon?** Yes — one bump attempt.
- **Why this matters more than it looks.** The pin is already a workaround for an upstream
  breakage. The question is not "can it build" (it does) but "what happens the next time
  it must move," and the pin is evidence that answer is not free.
- **Confidence: `inferred`.**

### D3 — What Chrime buys that Surfari cannot · **STOP — irreducible judgment**

- **Claim.** Engine ownership is worth its cost.
- **Why this node stops here.** This is a telos question, not a research question. Chrime's
  north star (`ns_46cf50bfa273`, ADR-0001) is *own the engine for total agent control* —
  stable node-ids, a deterministic `settle` receipt instead of sleeps, in-process Knox and
  Hancock. Whether that is worth carrying a Servo embedding is Daniel's call, and any
  decomposition I write would smuggle my answer in as criteria.
- **What is legitimately checkable, and belongs in the tree:** the *factual* half — which
  of those properties Surfari demonstrably cannot provide. That is a real leaf (compare
  `settle` receipts to Surfari's `wait --load networkidle`; compare node-id stability
  across re-snapshots). The *worth it* half is not.
- **Confidence: `researched`** on the factual half; the judgment half is surfaced, not
  answered.

---

## E — SCHEDULE: the roll-up

**Not a research node.** E is a function of A–D, and it has no evidence of its own.

```
schedule = ordered set of workloads w in A1 where
             B covers A2(w)  ∧  C1/C2 survive on w  ∧  D favors Chrime
           each with the gate that must be green before cutover
confidence(E) = floor( confidence(A), B, C, D )
```

**Today: `floor(inferred, unknown, inferred, inferred) = unknown`.**

**Therefore: no schedule can be honestly stated today.** Not "roughly Q4," not "three
phases." `unknown`.

That is the correct output, and it is what depth bought. The floor is set by **B0 — one
afternoon of running a test suite that already exists.** Until Chrime's own records agree
with Chrime's own test output, every downstream number would be built on a green the repo
itself contradicts.

**The first three things to do, in order:**

1. **B0** — run all 133 cases on both engines, commit the report. Reconcile `GOALS.md`
   against it. One afternoon. Unblocks the entire tree.
2. **B1** — EID-966, headless load. One afternoon. Unblocks B2, C2, C4.
3. **A1 + A2** — liveness-check the consumer list, extract the invoked command set. One
   afternoon. Turns "replace Surfari" into a bounded surface.

After those three, E's floor moves from `unknown` to whatever A–D honestly support, and
the schedule question becomes answerable for the first time.

---

## Where depth stopped paying

Four boundaries, all hit while building this tree. The first two are mechanical and
should generalize. The last two are judgment calls dressed as rules, and I say so.

**1. Below "name the evidence artifact," splitting renames rather than clarifies.**
B3 is the clean case. It has ~30 natural children — one per missing command. All thirty
would share one diff command, one contract, and one afternoon. Thirty boxes, zero new
information. **Signature: siblings that share an evidence artifact are one node.**

**2. A child whose contract is the parent's contract restated is ceremony.**
The sharpest mechanical test found. If the child passes exactly when the parent passes,
the child is a box drawn around already-scoped work and costs a level of indirection for
nothing. Every node above that survived was one where the child could pass while the
parent failed. That asymmetry *is* the value of the level.

**3. Depth cannot exceed the evidence available (C3).**
The most useful boundary and the least comfortable. C3 — "is Servo's fingerprint gap
fixable" — decomposes beautifully on paper: TLS layer, JS environment, timing, behavior.
Four crisp-looking children. All four would return `inferred`, because the detection
surface is adversarial and undisclosed. A tree of `inferred` leaves is *worse* than an
honest `unknown`, because the structure signals rigor the content does not have. That is
depth actively producing a lie — the failure mode the discipline exists to prevent,
reappearing inside the discipline itself. **Stop, mark `unknown`, name the experiment,
say it is not a leaf.**

**4. Depth stops at irreducible judgment (D3).**
"Is owning the engine worth it" is a telos question. Decomposing it produces criteria, and
criteria are where the answer gets smuggled in — whoever writes the criteria has written
the conclusion. The honest move is to split off the factual half (which properties Surfari
cannot provide — checkable) and surface the judgment half to the human unanswered. **This
is the boundary I am least sure about**, because "irreducible judgment" is exactly the
excuse a lazy agent would use to stop early. I do not have a test that separates the two.
Naming the gap rather than papering it.

**One more, unexpected: depth paid off fastest at the shallowest node.**
B0 sits one level below the root and is the cheapest leaf in the tree, and it is the one
that makes the entire question unanswerable today. The value did not come from going deep
on the interesting adversarial branch. It came from decomposing far enough to notice that
a "supply" branch existed at all, then asking the dullest possible question about it —
*do the records agree?* — before asking any interesting one. A flat solve reads `GOALS.md`
(the authoritative-looking file), sees green, and builds a roadmap on it. The tree's
contribution was not depth on C. It was **one level of structure that forced a boring
question to be asked before an interesting one.**
