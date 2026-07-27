#!/usr/bin/env python3
"""Complex Gmail scour acceptance runner (EID-1059).

Drives headed Chrime over JSONL TCP after a human logs into Gmail.
Fills six unrelated theme slots (commerce, security, calendar, finance, travel, social).

  ./target/release/chrime --listen 127.0.0.1:7421 https://mail.google.com/
  python3 scripts/run_gmail_scour.py --port 7421

See cases/gmail-scour/COMPLEX-TEST.md.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import socket
import sys
import time
import urllib.parse
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]

THEMES: list[dict[str, str]] = [
    {
        "id": "commerce",
        "label": "Commerce",
        "query": "subject:(order OR shipped OR delivery OR tracking OR receipt OR package)",
        "positive": r"\b(order|shipped|shipping|delivery|tracking|package|parcel|bought|purchase)\b",
    },
    {
        "id": "security",
        "label": "Security",
        "query": 'subject:(security OR "sign-in" OR "signed in" OR password OR unusual OR verify OR "2-step" OR 2FA)',
        "positive": r"\b(security|sign[- ]?in|password|unusual|verify|verification|2-?step|2fa|suspicious|alert)\b",
    },
    {
        "id": "calendar",
        "label": "Calendar",
        "query": "subject:(invitation OR invited OR meeting OR calendar OR RSVP OR Zoom OR Meet)",
        "positive": r"\b(invitation|invited|meeting|calendar|rsvp|zoom|google meet|teams|webinar)\b",
    },
    {
        "id": "finance",
        "label": "Finance",
        "query": "subject:(invoice OR statement OR payment OR bank OR tax OR payroll OR wire OR refund)",
        "positive": r"\b(invoice|statement|payment|bank|tax|payroll|wire|refund|balance|card ending)\b",
    },
    {
        "id": "travel",
        "label": "Travel",
        "query": "subject:(flight OR boarding OR hotel OR itinerary OR booking OR airline OR Airbnb)",
        "positive": r"\b(flight|boarding|hotel|itinerary|booking|airline|airbnb|check-?in|departure|arrival)\b",
    },
    {
        "id": "social_product",
        "label": "Social/product",
        "query": 'subject:(newsletter OR digest OR "new feature" OR announcement OR update OR unfollow)',
        "positive": r"\b(newsletter|digest|feature|announcement|unsubscribe|product update|what's new|whats new)\b",
    },
]

INBOX_MARKERS = ("inbox", "primary", "compose", "search mail", "mail.google.com")
LOGIN_MARKERS = (
    "sign in",
    "forgot email",
    "enter your password",
    "verify it’s you",
    "verify it's you",
    "2-step verification",
    "account recovery",
)


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


class ChrimeClient:
    def __init__(self, host: str, port: int, timeout: float = 30.0) -> None:
        self.host = host
        self.port = port
        self.timeout = timeout
        self.ops_used: list[str] = []

    def call(self, op: dict[str, Any]) -> dict[str, Any]:
        name = str(op.get("op", "?"))
        self.ops_used.append(name)
        line = json.dumps(op, separators=(",", ":")) + "\n"
        with socket.create_connection((self.host, self.port), timeout=self.timeout) as sock:
            sock.settimeout(self.timeout)
            sock.sendall(line.encode("utf-8"))
            # one JSONL response (trace-wrapped body may be flat)
            buf = b""
            while b"\n" not in buf:
                chunk = sock.recv(65536)
                if not chunk:
                    break
                buf += chunk
        raw = buf.decode("utf-8", errors="replace").strip().splitlines()
        if not raw:
            return {"ok": False, "code": "empty_response"}
        try:
            return json.loads(raw[0])
        except json.JSONDecodeError as e:
            return {"ok": False, "code": "bad_json", "error": str(e), "raw": raw[0][:200]}


def text_looks_inbox(text: str) -> bool:
    t = text.lower()
    return any(m in t for m in INBOX_MARKERS)


def text_looks_login(text: str) -> bool:
    t = text.lower()
    return any(m in t for m in LOGIN_MARKERS)


def extract_hit(theme: dict[str, str], text: str) -> dict[str, Any] | None:
    """Pick a plausible subject/from/snippet line cluster for the theme from live_read text."""
    if not text or len(text.strip()) < 20:
        return None
    pos = re.compile(theme["positive"], re.I)
    lines = [ln.strip() for ln in text.splitlines() if ln.strip()]
    # Prefer lines that match theme keywords and look like list rows (not chrome chrome)
    candidates: list[tuple[int, str]] = []
    for i, ln in enumerate(lines):
        if len(ln) < 8 or len(ln) > 220:
            continue
        if ln.lower() in {"inbox", "primary", "social", "promotions", "compose", "search mail"}:
            continue
        if pos.search(ln):
            candidates.append((i, ln))
    if not candidates:
        # fallback: any dense line near a positive keyword elsewhere
        if not pos.search(text):
            return None
        for i, ln in enumerate(lines):
            if 20 <= len(ln) <= 180 and not ln.startswith("http"):
                candidates.append((i, ln))
                break
    if not candidates:
        return None
    idx, subject = candidates[0]
    # neighbors as from/snippet
    from_ = ""
    snippet = ""
    if idx > 0 and len(lines[idx - 1]) < 80:
        from_ = lines[idx - 1]
    if idx + 1 < len(lines) and len(lines[idx + 1]) < 200:
        snippet = lines[idx + 1]
    key = hashlib.sha256(f"{subject}|{from_}".encode()).hexdigest()[:16]
    return {
        "status": "hit",
        "subject": subject,
        "from": from_,
        "snippet": snippet,
        "query": theme["query"],
        "message_key": key,
        "evidence": " · ".join(x for x in (from_, subject, snippet) if x)[:400],
    }


def search_url(query: str) -> str:
    q = urllib.parse.quote(query, safe="")
    return f"https://mail.google.com/mail/u/0/#search/{q}"


def wait_for_inbox(client: ChrimeClient, timeout: int, prompt: bool) -> tuple[str, str]:
    """Returns (auth_status, last_text)."""
    if prompt:
        print(
            "\n=== HUMAN STEP ===\n"
            "Log into Gmail in the headed Chrime window (password / 2FA / CAPTCHA).\n"
            "When you see your Inbox (or Primary), return here and press Enter.\n",
            flush=True,
        )
        try:
            input("Press Enter after you are logged in (or wait for auto-detect)… ")
        except EOFError:
            pass

    deadline = time.time() + timeout
    last = ""
    while time.time() < deadline:
        client.call({"op": "navigate", "url": "https://mail.google.com/mail/u/0/#inbox"})
        client.call({"op": "wait", "ms": 1500})
        resp = client.call({"op": "live_read"})
        last = str(resp.get("text") or "")
        if resp.get("ok") and text_looks_inbox(last) and not text_looks_login(last):
            return "passed", last
        if text_looks_login(last):
            # still on auth wall
            time.sleep(2)
            continue
        if text_looks_inbox(last):
            return "passed", last
        time.sleep(2)
    if text_looks_login(last):
        return "blocked_human_auth", last
    return "failed", last


def run(args: argparse.Namespace) -> int:
    client = ChrimeClient(args.host, args.port, timeout=args.socket_timeout)
    report: dict[str, Any] = {
        "test": "gmail-scour-complex",
        "eid": "EID-1059",
        "started_at": utc_now(),
        "finished_at": None,
        "auth": "unknown",
        "themes": {},
        "hits": 0,
        "misses": 0,
        "passed": False,
        "ops_used": [],
        "notes": [],
        "host": f"{args.host}:{args.port}",
    }

    # Connectivity
    ping = client.call({"op": "ping"})
    if not ping.get("ok"):
        report["notes"].append(f"ping failed: {ping}")
        report["finished_at"] = utc_now()
        write_report(args.report, report, client)
        print("FAIL: cannot reach Chrime API — start headed chrime with --listen", file=sys.stderr)
        return 2
    if not ping.get("live"):
        report["notes"].append("ping.live is false — gmail scour requires headed GUI live surface")
        report["finished_at"] = utc_now()
        write_report(args.report, report, client)
        print("FAIL: not a live GUI session (live_read will not work)", file=sys.stderr)
        return 2

    # Comfort: collapse sidebar for human login
    client.call({"op": "sidebar", "visible": False})
    client.call({"op": "navigate", "url": "https://mail.google.com/"})

    auth, last = wait_for_inbox(
        client,
        timeout=args.login_timeout,
        prompt=not args.skip_login_prompt,
    )
    report["auth"] = auth
    if auth != "passed":
        report["notes"].append(
            f"auth gate failed ({auth}); last live_read excerpt: {last[:400]!r}"
        )
        report["finished_at"] = utc_now()
        write_report(args.report, report, client)
        print(f"FAIL: auth={auth}", file=sys.stderr)
        return 1

    sync = client.call({"op": "live_sync"})
    report["notes"].append(f"live_sync: ok={sync.get('ok')} nodes={sync.get('node_count')}")

    used_keys: set[str] = set()
    for theme in THEMES:
        tid = theme["id"]
        url = search_url(theme["query"])
        client.call({"op": "navigate", "url": url})
        client.call({"op": "wait", "ms": args.search_wait_ms})
        live = client.call({"op": "live_read"})
        text = str(live.get("text") or "")
        client.call({"op": "live_sync"})
        hit = extract_hit(theme, text) if live.get("ok") else None
        if hit and hit["message_key"] in used_keys:
            # force miss rather than double-count
            hit = None
            report["notes"].append(f"{tid}: discarded duplicate message_key")
        if hit:
            used_keys.add(hit["message_key"])
            report["themes"][tid] = hit
            report["hits"] += 1
            print(f"  [HIT]  {theme['label']}: {hit['subject'][:80]}")
        else:
            report["themes"][tid] = {
                "status": "miss",
                "query": theme["query"],
                "excerpt": text[:400],
            }
            report["misses"] += 1
            print(f"  [MISS] {theme['label']}")

    report["ops_used"] = list(dict.fromkeys(client.ops_used))
    report["passed"] = report["auth"] == "passed" and report["hits"] >= 6
    report["finished_at"] = utc_now()
    write_report(args.report, report, client)

    print(
        f"\nResult: hits={report['hits']}/6 misses={report['misses']} "
        f"passed={report['passed']} report={args.report}"
    )
    return 0 if report["passed"] else 1


def write_report(path: str, report: dict[str, Any], client: ChrimeClient) -> None:
    report["ops_used"] = list(dict.fromkeys(client.ops_used))
    p = Path(path)
    if not p.is_absolute():
        p = ROOT / p
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(json.dumps(report, indent=2) + "\n")


def main() -> int:
    ap = argparse.ArgumentParser(description="Complex Gmail scour acceptance (6 unrelated themes)")
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=7421)
    ap.add_argument("--login-timeout", type=int, default=300)
    ap.add_argument("--search-wait-ms", type=int, default=3000)
    ap.add_argument("--socket-timeout", type=float, default=45.0)
    ap.add_argument("--skip-login-prompt", action="store_true")
    ap.add_argument("--report", default="logs/gmail-scour-report.json")
    args = ap.parse_args()
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
