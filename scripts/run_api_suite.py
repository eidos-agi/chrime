#!/usr/bin/env python3
"""
Run cases/api-suite.jsonl against `chrime --api`.

Plain-English tests for subagents + machine asserts. Failures append to:
  logs/api-bugs.jsonl
  logs/api-suite-report.json

Usage:
  python3 scripts/run_api_suite.py
  python3 scripts/run_api_suite.py --max 20
  python3 scripts/run_api_suite.py --only 7,42,100
  python3 scripts/run_api_suite.py --complexity 1-4
  python3 scripts/run_api_suite.py --chrime ./target/release/chrime
  python3 scripts/run_api_suite.py --fail-fast

Exit code: 0 all pass, 1 failures, 2 suite/runner error.
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SUITE = ROOT / "cases" / "api-suite.jsonl"
DEFAULT_BIN = ROOT / "target" / "release" / "chrime"
LOG_DIR = ROOT / "logs"
BUGS_PATH = LOG_DIR / "api-bugs.jsonl"
REPORT_PATH = LOG_DIR / "api-suite-report.json"
TRACE_PATH = LOG_DIR / "trace.jsonl"  # same hierarchy as chrime (docs/BREADCRUMBS.md)
FIXTURES = ROOT / "cases" / "fixtures"
FIXTURE_PORT = 7431

# Extra argv for the chrime binary (e.g. --engine servo), set once in main().
ENGINE_ARGS: list[str] = []
# Per-run profile dir (Servo's cookie jar). A fresh one each run is what makes a
# cross-process persistence case honest — a jar left by an earlier run cannot pass it.
PROFILE_DIR = LOG_DIR / "suite-profile"
EMPTY_PROFILE_DIR = LOG_DIR / "suite-profile-empty"


def substitute(text: str) -> str:
    """{{FIXTURES}} → fixture dir as file://, {{HTTP}} → fixture server,
    {{EMPTY_PROFILE}} → a never-logged-in profile dir (control for persistence cases)."""
    return (
        text.replace("{{FIXTURES}}", FIXTURES.as_uri())
        .replace("{{HTTP}}", f"http://127.0.0.1:{FIXTURE_PORT}")
        .replace("{{EMPTY_PROFILE}}", str(EMPTY_PROFILE_DIR))
    )


def start_fixture_server() -> None:
    """Tiny cookie/JS fixture site for auth-session + faithful-js cases.

    ponytail: a thread in the runner, not a managed process — it lives exactly as long as
    the suite run and needs no cleanup.
    """
    import threading
    from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, format, *args):  # quiet
            pass

        def _send(self, body: str, extra=()):
            raw = body.encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(raw)))
            for k, v in extra:
                self.send_header(k, v)
            self.end_headers()
            self.wfile.write(raw)

        def do_GET(self):
            if self.path.startswith("/login"):
                self._send(
                    "<html><head><title>login</title></head><body><h1>LOGIN OK</h1>"
                    '<a href="/protected">go to protected</a></body></html>',
                    # Max-Age makes it a *persistent* cookie — a session cookie is allowed to
                    # die with the process, so it could never prove an on-disk jar works.
                    [("Set-Cookie", "chrime_sess=agent-ok; Path=/; Max-Age=3600")],
                )
            elif self.path.startswith("/protected"):
                cookie = self.headers.get("Cookie") or ""
                if "chrime_sess=agent-ok" in cookie:
                    body = ("<html><head><title>protected</title></head><body>"
                            "<h1>LOGGED IN AS AGENT</h1></body></html>")
                else:
                    body = ("<html><head><title>wall</title></head><body>"
                            "<h1>LOGIN WALL</h1></body></html>")
                self._send(body)
            elif self.path.startswith("/js"):
                self._send((FIXTURES / "js-render.html").read_text())
            else:
                self._send("<html><head><title>fixture</title></head><body><h1>FIXTURE ROOT</h1>"
                           '<a href="/login">login</a></body></html>')

    srv = ThreadingHTTPServer(("127.0.0.1", FIXTURE_PORT), Handler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()


def _base36(n: int, width: int = 10) -> str:
    chars = "0123456789abcdefghijklmnopqrstuvwxyz"
    if n <= 0:
        return "0" * width
    out = []
    while n:
        out.append(chars[n % 36])
        n //= 36
    s = "".join(reversed(out))
    return s[-width:].rjust(width, "0")


class Breadcrumbs:
    """CHRIME.SUITE.<id>.CASE.<n>.STEP/ASSERT/BUG — see docs/BREADCRUMBS.md."""

    def __init__(self) -> None:
        ms = int(time.time() * 1000)
        self.suite_id = f"u{_base36(ms ^ os.getpid())}"
        self.root = f"CHRIME.SUITE.{self.suite_id}"
        LOG_DIR.mkdir(parents=True, exist_ok=True)
        self._emit(self.root, None, "suite_start",
                   f"API suite started ({self.root}). Hierarchy: docs/BREADCRUMBS.md.",
                   {"suite": self.suite_id, "doc": "docs/BREADCRUMBS.md"})

    def _emit(self, id_: str, parent: str | None, kind: str, english: str, data: dict) -> None:
        line = {
            "id": id_,
            "parent": parent,
            "kind": kind,
            "ts": datetime.now(timezone.utc).isoformat(),
            "ts_ms": int(time.time() * 1000),
            "english": english,
            "data": data,
        }
        with TRACE_PATH.open("a") as f:
            f.write(json.dumps(line, ensure_ascii=False, default=str) + "\n")

    def case_start(self, case_id: int, english: str) -> str:
        cid = f"{self.root}.CASE.{case_id}"
        self._emit(cid, self.root, "case_start",
                   f"Case {case_id} started: {english}",
                   {"case_id": case_id, "english": english})
        return cid

    def step(self, case_id: str, step_n: int, op: str) -> str:
        sid = f"{case_id}.STEP.{step_n:04d}"
        self._emit(sid, case_id, "request",
                   f"Step {step_n}: op `{op}`",
                   {"step": step_n, "op": op})
        return sid

    def assert_(self, case_id: str, n: int, ok: bool, detail: str) -> str:
        aid = f"{case_id}.ASSERT.{n:03d}"
        self._emit(aid, case_id, "assert",
                   f"Assert {n}: {'PASS' if ok else 'FAIL'} — {detail}",
                   {"assert": n, "ok": ok, "detail": detail})
        return aid

    def case_end(self, case_id: str, passed: bool) -> None:
        self._emit(case_id, self.root, "case_end",
                   f"Case finished: {'PASS' if passed else 'FAIL'} ({case_id})",
                   {"ok": passed})

    def bug(self, case_id: str, payload: dict) -> str:
        bid = f"{case_id}.BUG.b{_base36(int(time.time() * 1000) ^ os.getpid())}"
        self._emit(bid, case_id, "bug",
                   f"Bug logged for failing case {case_id}",
                   payload)
        return bid


def load_suite(path: Path) -> list[dict]:
    cases = []
    with path.open() as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            cases.append(json.loads(line))
    return cases


def get_path(obj: Any, path: str) -> Any:
    if path == "" or path is None:
        return obj
    cur = obj
    for part in path.split("."):
        if cur is None:
            return None
        if isinstance(cur, list):
            try:
                cur = cur[int(part)]
            except (ValueError, IndexError):
                return None
        elif isinstance(cur, dict):
            cur = cur.get(part)
        else:
            return None
    return cur


def resolve_node_id(token: Any, responses: list[Any]) -> Any:
    """Resolve $find0 / $link0 placeholders from prior responses.

    $findN prefers the Nth *clickable* match (agents mean the link/button, not a wrapping text node).
    """
    if not isinstance(token, str) or not token.startswith("$"):
        return token
    if token.startswith("$find"):
        idx = int(token.replace("$find", "") or "0")
        for r in reversed(responses):
            if not isinstance(r, list):
                continue
            clickable = [n for n in r if isinstance(n, dict) and n.get("clickable")]
            pool = clickable if clickable else [n for n in r if isinstance(n, dict)]
            if len(pool) > idx:
                return pool[idx].get("node_id")
        return 0
    if token.startswith("$link"):
        idx = int(token.replace("$link", "") or "0")
        for r in reversed(responses):
            if isinstance(r, list) and len(r) > idx and isinstance(r[idx], dict):
                return r[idx].get("node_id")
        return 0
    return token


def materialize_ops(ops: list[dict], responses: list[Any]) -> list[dict]:
    out = []
    for op in ops:
        o = json.loads(json.dumps(op))
        if "node_id" in o:
            o["node_id"] = resolve_node_id(o["node_id"], responses)
        out.append(o)
    return out


def case_env(case: dict) -> dict:
    """Process env for this case's chrime — inherited, plus any per-case overrides.

    A case that needs a *different* engine profile (e.g. proving a fresh profile starts
    logged out) sets {"env": {"CHRIME_PROFILE_DIR": "{{EMPTY_PROFILE}}"}}.
    """
    env = dict(os.environ)
    env.update(case.get("env") or {})
    return env


def run_chrime(
    bin_path: Path, lines: list[str], timeout: float = 120.0, env: dict | None = None
) -> list[str]:
    proc = subprocess.Popen(
        [str(bin_path), "--api", *ENGINE_ARGS],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=env,
    )
    payload = "\n".join(lines) + "\n"
    try:
        stdout, stderr = proc.communicate(payload, timeout=timeout)
    except subprocess.TimeoutExpired:
        proc.kill()
        raise RuntimeError("chrime --api timed out")
    if proc.returncode not in (0, None) and not stdout.strip():
        raise RuntimeError(f"chrime exited {proc.returncode}: {stderr[:500]}")
    return [ln for ln in stdout.splitlines() if ln.strip()]


def parse_responses(lines: list[str]) -> list[Any]:
    out = []
    for ln in lines:
        try:
            out.append(json.loads(ln))
        except json.JSONDecodeError:
            out.append({"_raw": ln, "ok": False, "code": "non_json_response"})
    return out


def check_assert(a: dict, responses: list[Any]) -> tuple[bool, str]:
    on = a.get("on", -1)
    if on < 0:
        on = len(responses) + on
    if on < 0 or on >= len(responses):
        return False, f"response index {a.get('on')} out of range (have {len(responses)})"
    resp = responses[on]
    path = a.get("path")
    val = get_path(resp, path) if path is not None else resp

    if "exists" in a:
        # For simple top-level keys, null still "exists" if the key is present.
        if (
            a["exists"]
            and path
            and "." not in path
            and isinstance(resp, dict)
        ):
            ok = path in resp
            return ok, "key exists" if ok else f"path {path} missing"
        ok = (val is not None) if a["exists"] else (val is None)
        return (ok, "exists" if ok else f"path {path} missing")

    if "type" in a:
        t = a["type"]
        ok = (
            (t == "list" and isinstance(val, list))
            or (t == "object" and isinstance(val, dict))
            or (t == "string" and isinstance(val, str))
            or (t == "number" and isinstance(val, (int, float)) and not isinstance(val, bool))
            or (t == "boolean" and isinstance(val, bool))
        )
        return ok, f"type {t}" if ok else f"expected type {t}, got {type(val).__name__}"

    if "eq" in a:
        ok = val == a["eq"]
        return ok, f"{val!r} == {a['eq']!r}" if ok else f"expected {a['eq']!r}, got {val!r}"

    if "ne" in a:
        ok = val != a["ne"]
        return ok, "ne ok" if ok else f"value unexpectedly {val!r}"

    if "contains" in a:
        needle = a["contains"]
        if isinstance(val, str):
            ok = needle in val
        elif isinstance(val, list):
            ok = needle in val
        else:
            ok = False
        return ok, "contains" if ok else f"{needle!r} not in {val!r}"

    if "not_contains" in a:
        needle = a["not_contains"]
        if isinstance(val, str):
            ok = needle not in val
        else:
            ok = True
        return ok, "not_contains" if ok else f"unexpectedly contains {needle!r}"

    if "gte" in a:
        try:
            ok = val is not None and val >= a["gte"]
        except TypeError:
            ok = False
        return ok, f"{val} >= {a['gte']}" if ok else f"expected >= {a['gte']}, got {val!r}"

    if "lte" in a:
        try:
            ok = val is not None and val <= a["lte"]
        except TypeError:
            ok = False
        return ok, f"{val} <= {a['lte']}" if ok else f"expected <= {a['lte']}, got {val!r}"

    if "min_len" in a:
        try:
            ok = len(val) >= a["min_len"]
        except TypeError:
            ok = False
        return ok, "min_len" if ok else f"len {type(val).__name__} < {a['min_len']}"

    if "len_eq" in a:
        try:
            ok = len(val) == a["len_eq"]
        except TypeError:
            ok = False
        return ok, "len_eq" if ok else f"len != {a['len_eq']}"

    if "eq_path" in a:
        ref = a["eq_path"]
        on2 = ref.get("on", -1)
        if on2 < 0:
            on2 = len(responses) + on2
        other = get_path(responses[on2], ref.get("path"))
        ok = val == other
        return ok, "eq_path" if ok else f"{val!r} != {other!r}"

    if "gte_path" in a:
        # optional special: compare two path values — used loosely
        ref = a["gte_path"]
        on2 = ref.get("on", -1)
        if on2 < 0:
            on2 = len(responses) + on2
        other = get_path(responses[on2], ref.get("path"))
        try:
            ok = val is not None and other is not None and val >= other
        except TypeError:
            ok = False
        return ok, "gte_path" if ok else f"{val!r} not >= {other!r}"

    if "all_role_in" in a:
        allowed = set(a["all_role_in"])
        if not isinstance(val, list):
            return False, "not a list"
        for n in val:
            if isinstance(n, dict) and n.get("role") not in allowed:
                return False, f"role {n.get('role')} not in {allowed}"
        return True, "all_role_in"

    if "any_text_contains" in a:
        needle = a["any_text_contains"].lower()
        if not isinstance(val, list):
            return False, "not a list"
        for n in val:
            if isinstance(n, dict) and needle in (n.get("text") or "").lower():
                return True, "any_text_contains"
        return False, f"no node text contains {needle!r}"

    if "forbid_keys" in a:
        if not isinstance(resp, dict):
            return True, "skip forbid on non-object"
        for k in a["forbid_keys"]:
            if k == "_trace":
                continue
            if k in resp and resp[k] not in (None, "", [], {}):
                if k in ("password", "secret", "value") and isinstance(resp.get(k), str) and len(resp[k]) > 0:
                    return False, f"forbidden key {k} present with value"
        return True, "forbid_keys"

    return False, f"unknown assert keys: {list(a.keys())}"


def run_case(bin_path: Path, case: dict, crumbs: Breadcrumbs) -> dict:
    started = time.time()
    case = json.loads(substitute(json.dumps(case)))  # {{FIXTURES}} / {{HTTP}} in ops and asserts
    english = case.get("english", "")
    cid = case.get("id")
    case_trace = crumbs.case_start(int(cid), english)
    try:
        if "raw_ops" in case:
            crumbs.step(case_trace, 1, "raw")
            lines = list(case["raw_ops"])
            out_lines = run_chrime(bin_path, lines, env=case_env(case))
            responses = parse_responses(out_lines)
        else:
            ops = case.get("ops") or []
            blob = json.dumps(ops)
            if "$find" in blob or "$link" in blob:
                responses = []
                proc = subprocess.Popen(
                    [str(bin_path), "--api", *ENGINE_ARGS],
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                    bufsize=1,
                    env=case_env(case),
                )
                assert proc.stdin and proc.stdout
                try:
                    for i, op in enumerate(ops, 1):
                        crumbs.step(case_trace, i, op.get("op", "?"))
                        mop = materialize_ops([op], responses)[0]
                        proc.stdin.write(json.dumps(mop) + "\n")
                        proc.stdin.flush()
                        line = proc.stdout.readline()
                        if not line:
                            break
                        responses.append(json.loads(line))
                finally:
                    proc.stdin.close()
                    try:
                        proc.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        proc.kill()
            else:
                for i, op in enumerate(ops, 1):
                    crumbs.step(case_trace, i, op.get("op", "?"))
                lines = [json.dumps(op) for op in ops]
                out_lines = run_chrime(bin_path, lines, env=case_env(case))
                responses = parse_responses(out_lines)

        failures = []
        for i, a in enumerate(case.get("assert") or []):
            ok, msg = check_assert(a, responses)
            crumbs.assert_(case_trace, i, ok, msg)
            if not ok:
                failures.append({
                    "assert_index": i,
                    "assert_id": f"{case_trace}.ASSERT.{i:03d}",
                    "assert": a,
                    "detail": msg,
                })

        passed = len(failures) == 0
        crumbs.case_end(case_trace, passed)
        return {
            "id": cid,
            "trace_id": case_trace,
            "english": english,
            "complexity": case.get("complexity"),
            "tags": case.get("tags") or [],
            "passed": passed,
            "failures": failures,
            "responses": responses,
            "duration_ms": int((time.time() - started) * 1000),
        }
    except Exception as e:
        crumbs.case_end(case_trace, False)
        return {
            "id": cid,
            "trace_id": case_trace,
            "english": english,
            "complexity": case.get("complexity"),
            "tags": case.get("tags") or [],
            "passed": False,
            "failures": [{"assert_index": -1, "detail": str(e)}],
            "responses": [],
            "duration_ms": int((time.time() - started) * 1000),
            "error": str(e),
        }


def append_bug(result: dict, case: dict, crumbs: Breadcrumbs) -> None:
    LOG_DIR.mkdir(parents=True, exist_ok=True)
    case_trace = result.get("trace_id") or f"{crumbs.root}.CASE.{result.get('id')}"
    bug_id = crumbs.bug(case_trace, {
        "case_id": result.get("id"),
        "english": result.get("english"),
        "failures": result.get("failures"),
    })
    bug = {
        "id": bug_id,
        "parent": case_trace,
        "ts": datetime.now(timezone.utc).isoformat(),
        "source": "api-suite",
        "case_id": result.get("id"),
        "trace_id": case_trace,
        "hierarchy_doc": "docs/BREADCRUMBS.md",
        "complexity": result.get("complexity"),
        "english": result.get("english"),
        "tags": result.get("tags"),
        "failures": result.get("failures"),
        "error": result.get("error"),
        "responses_tail": (result.get("responses") or [])[-3:],
        "status": "open",
        "fix": {
            "symptoms": result.get("english"),
            "acceptance": "Re-run case until pass: python3 scripts/run_api_suite.py --only "
            + str(result.get("id")),
            "ops": case.get("ops") or case.get("raw_ops"),
            "breadcrumb": bug_id,
        },
    }
    with BUGS_PATH.open("a") as f:
        f.write(json.dumps(bug, ensure_ascii=False, default=str) + "\n")


def main() -> int:
    ap = argparse.ArgumentParser(description="Run Chrime plain-English API suite")
    ap.add_argument("--suite", type=Path, default=DEFAULT_SUITE)
    ap.add_argument("--chrime", type=Path, default=DEFAULT_BIN)
    ap.add_argument("--max", type=int, default=0, help="max cases to run (0=all)")
    ap.add_argument("--only", type=str, default="", help="comma-separated case ids")
    ap.add_argument("--complexity", type=str, default="", help="e.g. 1-4 or 8")
    ap.add_argument("--tag", type=str, default="", help="only cases with this tag")
    ap.add_argument("--skip-tag", type=str, default="", help="skip cases with this tag")
    ap.add_argument("--fail-fast", action="store_true")
    ap.add_argument("--quiet", action="store_true")
    ap.add_argument("--no-bugs", action="store_true", help="do not write logs/api-bugs.jsonl")
    ap.add_argument("--engine", type=str, default="static",
                    help="engine to drive: static (default) or servo (needs --features servo)")
    args = ap.parse_args()

    global ENGINE_ARGS
    if args.engine and args.engine != "static":
        ENGINE_ARGS = ["--engine", args.engine]

    if not args.chrime.exists():
        print(f"chrime binary not found: {args.chrime}", file=sys.stderr)
        print("Build: cargo build --release", file=sys.stderr)
        return 2

    if not args.suite.exists():
        print(f"suite missing: {args.suite} — run scripts/generate_api_suite.py", file=sys.stderr)
        return 2

    cases = load_suite(args.suite)

    if args.only:
        want = {int(x) for x in args.only.split(",") if x.strip()}
        cases = [c for c in cases if c["id"] in want]
    if args.complexity:
        if "-" in args.complexity:
            lo, hi = args.complexity.split("-", 1)
            lo, hi = int(lo), int(hi)
            cases = [c for c in cases if lo <= int(c.get("complexity", 0)) <= hi]
        else:
            c = int(args.complexity)
            cases = [c_ for c_ in cases if int(c_.get("complexity", 0)) == c]
    if args.tag:
        cases = [c for c in cases if args.tag in (c.get("tags") or [])]
    if args.skip_tag:
        cases = [c for c in cases if args.skip_tag not in (c.get("tags") or [])]
    if args.max:
        cases = cases[: args.max]

    # Cases tagged `servo` need a JS engine; on the static binary they are skipped, never
    # silently counted as passing. Say how many, so a green run can't hide them.
    drop = "servo" if args.engine != "servo" else "static-only"
    skipped = [c for c in cases if drop in (c.get("tags") or [])]
    if skipped:
        cases = [c for c in cases if drop not in (c.get("tags") or [])]
        print(f"Skipping {len(skipped)} {drop} case(s) "
              f"({','.join(str(c['id']) for c in skipped)}) — engine is {args.engine}")

    if not cases:
        print("no cases selected", file=sys.stderr)
        return 2

    if any("http" in (c.get("tags") or []) for c in cases):
        start_fixture_server()
        print(f"Fixture server on http://127.0.0.1:{FIXTURE_PORT} (cookie + JS fixtures)")

    crumbs = Breadcrumbs()

    # Fresh engine profile per run. Cases that prove cookies survive a restart must not be
    # able to pass on a jar left behind by an earlier run.
    global PROFILE_DIR, EMPTY_PROFILE_DIR
    PROFILE_DIR = LOG_DIR / f"suite-profile-{crumbs.suite_id}"
    EMPTY_PROFILE_DIR = LOG_DIR / f"suite-profile-{crumbs.suite_id}-empty"
    PROFILE_DIR.mkdir(parents=True, exist_ok=True)
    EMPTY_PROFILE_DIR.mkdir(parents=True, exist_ok=True)
    os.environ["CHRIME_PROFILE_DIR"] = str(PROFILE_DIR)

    print(f"Running {len(cases)} cases against {args.chrime}")
    print(f"Engine profile (cookie jar): {PROFILE_DIR}")
    print(f"Suite breadcrumb root: {crumbs.root}")
    print(f"Hierarchy: docs/BREADCRUMBS.md  |  trace → {TRACE_PATH}")
    results = []
    failed = 0
    for case in cases:
        r = run_case(args.chrime, case, crumbs)
        results.append(r)
        mark = "PASS" if r["passed"] else "FAIL"
        if not args.quiet or not r["passed"]:
            print(f"  [{mark}] #{r['id']:03d} c{r.get('complexity')}  {r.get('trace_id','')}")
            print(f"         {r['english'][:100]}")
            if not r["passed"]:
                for f in r.get("failures") or []:
                    print(f"         → {f.get('assert_id', '')} {f.get('detail')}")
        if not r["passed"]:
            failed += 1
            if not args.no_bugs:
                append_bug(r, case, crumbs)
            if args.fail_fast:
                break

    LOG_DIR.mkdir(parents=True, exist_ok=True)
    report = {
        "ts": datetime.now(timezone.utc).isoformat(),
        "suite_trace_id": crumbs.root,
        "hierarchy_doc": "docs/BREADCRUMBS.md",
        "binary": str(args.chrime),
        "suite": str(args.suite),
        "total": len(results),
        "passed": sum(1 for r in results if r["passed"]),
        "failed": failed,
        "bugs_log": str(BUGS_PATH) if not args.no_bugs else None,
        "trace_log": str(TRACE_PATH),
        "results": [
            {
                "id": r["id"],
                "trace_id": r.get("trace_id"),
                "passed": r["passed"],
                "english": r["english"],
                "complexity": r.get("complexity"),
                "duration_ms": r.get("duration_ms"),
                "failures": r.get("failures"),
            }
            for r in results
        ],
    }
    REPORT_PATH.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    print(
        f"\n{report['passed']}/{report['total']} passed  |  "
        f"{failed} failed  |  report {REPORT_PATH}"
    )
    if failed and not args.no_bugs:
        print(f"Failures logged → {BUGS_PATH}")
        print("Subagent next: read open bugs, fix API, re-run --only <id>")
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
