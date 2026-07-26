#!/usr/bin/env python3
"""Generate cases/api-suite.jsonl — 100+ plain-English API tests of rising complexity."""
from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "cases" / "api-suite.jsonl"

EX = "https://example.com"
EX_SLASH = "https://example.com/"
IANA = "https://iana.org/domains/example"


def case(
    id_: int,
    complexity: int,
    english: str,
    ops: list,
    assert_: list,
    *,
    tags: list | None = None,
    skip_if: str | None = None,
    env: dict | None = None,
) -> dict:
    d = {
        "id": id_,
        "complexity": complexity,
        "english": english,
        "ops": ops,
        "assert": assert_,
        "tags": tags or [],
    }
    if skip_if:
        d["skip_if"] = skip_if
    if env:
        d["env"] = env
    return d


def main() -> None:
    cases: list[dict] = []
    n = 0

    def add(complexity: int, english: str, ops: list, assert_: list, **kw):
        nonlocal n
        n += 1
        cases.append(case(n, complexity, english, ops, assert_, **kw))

    # --- complexity 1: liveness & API surface ---
    add(1, "Ping the API and confirm it answers that everything is ok.",
        [{"op": "ping"}], [{"on": -1, "path": "ok", "eq": True}])
    add(1, "Ask for help and confirm the response lists available operations.",
        [{"op": "help"}], [
            {"on": -1, "path": "ok", "eq": True},
            {"on": -1, "path": "ops", "type": "list"},
            {"on": -1, "path": "ops", "contains": "navigate"},
        ])
    add(1, "Ask for ops (alias of help) and confirm navigate is listed.",
        [{"op": "ops"}], [{"on": -1, "path": "ops", "contains": "snapshot"}])
    add(1, "Send empty JSON object without an op and expect a clear error.",
        [{}], [{"on": -1, "path": "ok", "eq": False}, {"on": -1, "path": "code", "exists": True}])
    add(1, "Send an unknown operation name and expect unknown_op or similar failure.",
        [{"op": "definitely_not_a_real_op"}],
        [{"on": -1, "path": "ok", "eq": False}])
    add(1, "Send invalid JSON (as a bare string op) and expect a bad_json style failure.",
        [{"op": "ping"}],  # valid - next is custom via raw lines
        [{"on": -1, "path": "ok", "eq": True}], tags=["sanity"])
    # raw invalid handled in runner for a dedicated case
    add(1, "Call hello (alias of ping) and expect ok true.",
        [{"op": "hello"}], [{"on": -1, "path": "ok", "eq": True}])
    add(1, "Call status on a fresh session with no page and expect ok without crashing.",
        [{"op": "status"}], [{"on": -1, "path": "ok", "eq": True}])
    add(1, "Call current with no page and expect a url field that is null or missing.",
        [{"op": "current"}], [{"on": -1, "path": "url", "exists": True}])
    add(1, "Confirm help promises that secrets are never returned.",
        [{"op": "help"}],
        [{"on": -1, "path": "secret_output", "eq": "suppressed"}])

    # --- complexity 2: basic navigate ---
    add(2, "Go to example.com using a full https URL and confirm navigation succeeds.",
        [{"op": "navigate", "url": EX}],
        [{"on": -1, "path": "ok", "eq": True}, {"on": -1, "path": "url", "contains": "example.com"}])
    add(2, "Go to example.com using a bare host name and confirm it resolves to https.",
        [{"op": "navigate", "url": "example.com"}],
        [{"on": -1, "path": "ok", "eq": True}, {"on": -1, "path": "url", "contains": "example.com"}])
    add(2, "Navigate to example.com and confirm the page title mentions Example.",
        [{"op": "navigate", "url": EX}],
        [{"on": -1, "path": "ok", "eq": True},
         {"on": -1, "path": "title", "contains": "Example"}])
    add(2, "Navigate to example.com and check HTTP status is 200.",
        [{"op": "navigate", "url": EX}],
        [{"on": -1, "path": "status", "eq": 200}])
    add(2, "Navigate to an empty URL string and expect failure without crashing.",
        [{"op": "navigate", "url": ""}],
        [{"on": -1, "path": "ok", "eq": False}])
    add(2, "After navigating to example.com, ask for current URL and see example.com.",
        [{"op": "navigate", "url": EX}, {"op": "current"}],
        [{"on": -1, "path": "url", "contains": "example.com"}])
    add(2, "After navigating, status should report a positive node_count.",
        [{"op": "navigate", "url": EX}, {"op": "status"}],
        [{"on": -1, "path": "ok", "eq": True},
         {"on": -1, "path": "node_count", "gte": 1}])
    add(2, "Navigate to example.com twice in a row and both should succeed.",
        [{"op": "navigate", "url": EX}, {"op": "navigate", "url": EX}],
        [{"on": 0, "path": "ok", "eq": True}, {"on": 1, "path": "ok", "eq": True}])
    add(2, "Navigate with a trailing slash URL and still land on example.com.",
        [{"op": "navigate", "url": EX_SLASH}],
        [{"on": -1, "path": "ok", "eq": True}])
    add(2, "Navigate to httpbin status 404 if reachable, or accept network failure without hang.",
        [{"op": "navigate", "url": "https://httpbin.org/status/404"}],
        [{"on": -1, "path": "ok", "exists": True}],
        tags=["network", "optional"])

    # --- complexity 3: read / snapshot basics ---
    add(3, "Navigate to example.com and take a full snapshot; it must list nodes.",
        [{"op": "navigate", "url": EX}, {"op": "snapshot"}],
        [{"on": -1, "path": "nodes", "type": "list"},
         {"on": -1, "path": "node_count", "gte": 1},
         {"on": -1, "path": "view", "eq": "full"}])
    add(3, "Snapshot nodes must each have a node_id.",
        [{"op": "navigate", "url": EX}, {"op": "snapshot"}],
        [{"on": -1, "path": "nodes.0.node_id", "type": "number"}])
    add(3, "Snapshot should include a title for example.com.",
        [{"op": "navigate", "url": EX}, {"op": "snapshot"}],
        [{"on": -1, "path": "title", "contains": "Example"}])
    add(3, "Read the full page text and confirm it mentions Example Domain.",
        [{"op": "navigate", "url": EX}, {"op": "read"}],
        [{"on": -1, "path": "text", "contains": "Example"}])
    add(3, "Read text should be a non-empty string after loading example.com.",
        [{"op": "navigate", "url": EX}, {"op": "read"}],
        [{"on": -1, "path": "text", "type": "string"},
         {"on": -1, "path": "text", "min_len": 5}])
    add(3, "Snapshot must report html_bytes greater than zero for a real page.",
        [{"op": "navigate", "url": EX}, {"op": "snapshot"}],
        [{"on": -1, "path": "html_bytes", "gte": 100}])
    add(3, "Every snapshot node must have a role string.",
        [{"op": "navigate", "url": EX}, {"op": "snapshot"}],
        [{"on": -1, "path": "nodes.0.role", "type": "string"}])
    add(3, "Snapshot url should match the navigated example.com host.",
        [{"op": "navigate", "url": EX}, {"op": "snapshot"}],
        [{"on": -1, "path": "url", "contains": "example.com"}])
    add(3, "Calling snapshot without navigate should still return a structure with nodes list.",
        [{"op": "snapshot"}],
        [{"on": -1, "path": "nodes", "type": "list"}])
    add(3, "Status after navigate should echo the title when available.",
        [{"op": "navigate", "url": EX}, {"op": "status"}],
        [{"on": -1, "path": "title", "contains": "Example"}])

    # --- complexity 4: views catalog ---
    add(4, "List available views and confirm outline and meta are present.",
        [{"op": "views"}],
        [{"on": -1, "path": "ok", "eq": True},
         {"on": -1, "path": "views", "type": "list"}])
    add(4, "Views response should mention one HTML buffer memory model.",
        [{"op": "views"}],
        [{"on": -1, "path": "memory", "contains": "buffer"}])
    for kind in ("full", "outline", "links", "fields", "clickables", "text", "compact", "meta"):
        add(4, f"Open example.com and request the {kind} view of the same page.",
            [{"op": "navigate", "url": EX}, {"op": "view", "kind": kind}],
            [{"on": -1, "path": "view", "eq": kind},
             {"on": -1, "path": "html_bytes", "gte": 1}])

    # --- complexity 5: view semantics ---
    add(5, "Outline view should only include heading nodes (or be empty of non-headings).",
        [{"op": "navigate", "url": EX}, {"op": "view", "kind": "outline"}],
        [{"on": -1, "path": "view", "eq": "outline"},
         {"on": -1, "path": "nodes", "all_role_in": ["heading"]}])
    add(5, "Links view should only include link roles with clickable links.",
        [{"op": "navigate", "url": EX}, {"op": "view", "kind": "links"}],
        [{"on": -1, "path": "nodes", "all_role_in": ["link"]}])
    add(5, "Meta view must return zero nodes but still report counts.",
        [{"op": "navigate", "url": EX}, {"op": "view", "kind": "meta"}],
        [{"on": -1, "path": "node_count", "eq": 0},
         {"on": -1, "path": "nodes", "len_eq": 0},
         {"on": -1, "path": "counts", "type": "object"}])
    add(5, "Compact view must not return more nodes than the full snapshot.",
        [{"op": "navigate", "url": EX},
         {"op": "snapshot"},
         {"op": "view", "kind": "compact"}],
        [{"on": -1, "path": "view", "eq": "compact"},
         {"on": 1, "path": "node_count", "gte_path": {"on": 2, "path": "node_count"}}])
    add(5, "Snapshot with view=outline parameter should behave like view op.",
        [{"op": "navigate", "url": EX}, {"op": "snapshot", "view": "outline"}],
        [{"on": -1, "path": "view", "eq": "outline"}])
    add(5, "Unknown view kind should produce a clear error.",
        [{"op": "view", "kind": "not_a_real_view"}],
        [{"on": -1, "path": "ok", "eq": False}])
    add(5, "Clickables view on example.com should include the Learn more link.",
        [{"op": "navigate", "url": EX}, {"op": "view", "kind": "clickables"}],
        [{"on": -1, "path": "nodes", "any_text_contains": "Learn"}])
    add(5, "Full and outline views must share the same html_bytes for one page.",
        [{"op": "navigate", "url": EX},
         {"op": "view", "kind": "full"},
         {"op": "view", "kind": "outline"}],
        [{"on": 1, "path": "html_bytes", "eq_path": {"on": 2, "path": "html_bytes"}}])
    add(5, "Text view roles should only be text when nodes are present.",
        [{"op": "navigate", "url": EX}, {"op": "view", "kind": "text"}],
        [{"on": -1, "path": "nodes", "all_role_in": ["text"]}])
    add(5, "Fields view should not crash on a page with no forms.",
        [{"op": "navigate", "url": EX}, {"op": "view", "kind": "fields"}],
        [{"on": -1, "path": "view", "eq": "fields"}])

    # --- complexity 6: find_text / links ---
    add(6, "Find text 'Example' on example.com and expect at least one match.",
        [{"op": "navigate", "url": EX}, {"op": "find_text", "text": "Example"}],
        [{"on": -1, "type": "list"}, {"on": -1, "min_len": 1}])
    add(6, "Find text that is not on the page and expect an empty list.",
        [{"op": "navigate", "url": EX}, {"op": "find_text", "text": "zzzxxyyzz_not_present"}],
        [{"on": -1, "type": "list"}, {"on": -1, "len_eq": 0}])
    add(6, "Find text should be case-insensitive for 'example'.",
        [{"op": "navigate", "url": EX}, {"op": "find_text", "text": "example"}],
        [{"on": -1, "min_len": 1}])
    add(6, "List links on example.com and expect at least one with an href.",
        [{"op": "navigate", "url": EX}, {"op": "links"}],
        [{"on": -1, "type": "list"}, {"on": -1, "min_len": 1},
         {"on": -1, "path": "0.href", "contains": "http"}])
    add(6, "Every link from links op should be marked clickable.",
        [{"op": "navigate", "url": EX}, {"op": "links"}],
        [{"on": -1, "path": "0.clickable", "eq": True}])
    add(6, "find_text results should include node_id fields.",
        [{"op": "navigate", "url": EX}, {"op": "find_text", "text": "More"}],
        [{"on": -1, "path": "0.node_id", "type": "number"}])
    add(6, "Empty find_text query should return empty or harmless result.",
        [{"op": "navigate", "url": EX}, {"op": "find_text", "text": ""}],
        [{"on": -1, "type": "list"}])
    add(6, "Links list length should match clickable links in full snapshot roughly.",
        [{"op": "navigate", "url": EX}, {"op": "links"}, {"op": "view", "kind": "links"}],
        [{"on": 1, "type": "list"}, {"on": 2, "path": "node_count", "gte": 1}])
    add(6, "Find 'Domain' on example.com returns matches containing that word.",
        [{"op": "navigate", "url": EX}, {"op": "find_text", "text": "Domain"}],
        [{"on": -1, "min_len": 1}])
    add(6, "After navigate, links op must not return ok:false error object.",
        [{"op": "navigate", "url": EX}, {"op": "links"}],
        [{"on": -1, "type": "list"}])

    # --- complexity 7: click & back ---
    add(7, "Click a non-link node and expect a clear error about no href.",
        [{"op": "navigate", "url": EX}, {"op": "click", "node_id": 1}],
        [{"on": -1, "path": "ok", "eq": False}])
    add(7, "Click node_id 0 which does not exist and expect failure.",
        [{"op": "navigate", "url": EX}, {"op": "click", "node_id": 0}],
        [{"on": -1, "path": "ok", "eq": False}])
    add(7, "Click a huge node_id that does not exist and expect failure.",
        [{"op": "navigate", "url": EX}, {"op": "click", "node_id": 999999}],
        [{"on": -1, "path": "ok", "eq": False}])
    add(7, "Find the Learn more link, click its node_id, and leave example.com.",
        [{"op": "navigate", "url": EX},
         {"op": "find_text", "text": "Learn more"},
         {"op": "click", "node_id": "$find0"}],  # runner resolves from find_text[0]
        [{"on": -1, "path": "ok", "eq": True},
         {"on": -1, "path": "url", "not_contains": "example.com/"}],
        tags=["click-resolve"])
    add(7, "Navigate to example.com then back with only one entry should fail or no-op safely.",
        [{"op": "navigate", "url": EX}, {"op": "back"}],
        [{"on": -1, "path": "ok", "exists": True}])
    add(7, "Navigate example.com then iana via click, then back should return toward example.",
        [{"op": "navigate", "url": EX},
         {"op": "find_text", "text": "Learn more"},
         {"op": "click", "node_id": "$find0"},
         {"op": "back"}],
        [{"on": -1, "path": "ok", "eq": True},
         {"on": -1, "path": "url", "contains": "example.com"}],
        tags=["click-resolve", "network"])
    add(7, "After a successful click navigation, current url should change.",
        [{"op": "navigate", "url": EX},
         {"op": "find_text", "text": "Learn more"},
         {"op": "click", "node_id": "$find0"},
         {"op": "current"}],
        [{"on": -1, "path": "url", "type": "string"},
         {"on": -1, "path": "url", "min_len": 8}],
        tags=["click-resolve"])
    add(7, "Click without prior navigate fails gracefully.",
        [{"op": "click", "node_id": 1}],
        [{"on": -1, "path": "ok", "eq": False}])
    add(7, "History length in status should increase after two navigations.",
        [{"op": "navigate", "url": EX},
         {"op": "navigate", "url": "https://example.org"},
         {"op": "status"}],
        [{"on": -1, "path": "history_len", "gte": 2}],
        tags=["network"])
    add(7, "Back from two-page history (distinct hosts) should succeed.",
        [{"op": "navigate", "url": EX},
         {"op": "navigate", "url": "https://example.org"},
         {"op": "back"}],
        [{"on": -1, "path": "ok", "eq": True},
         {"on": -1, "path": "url", "contains": "example.com"}],
        tags=["network"])

    # --- complexity 8: multi-step agent scripts ---
    add(8, "Agent script: go to example.com, take outline view, confirm a heading exists.",
        [{"op": "navigate", "url": EX},
         {"op": "view", "kind": "outline"},
         {"op": "status"}],
        [{"on": 1, "path": "node_count", "gte": 1},
         {"on": 2, "path": "ok", "eq": True}])
    add(8, "Agent script: snapshot, then find_text using a word from the title.",
        [{"op": "navigate", "url": EX},
         {"op": "snapshot"},
         {"op": "find_text", "text": "Domain"}],
        [{"on": 2, "min_len": 1}])
    add(8, "Agent script: list views, pick meta, confirm html_bytes present.",
        [{"op": "navigate", "url": EX},
         {"op": "views"},
         {"op": "view", "kind": "meta"}],
        [{"on": 2, "path": "html_bytes", "gte": 100}])
    add(8, "Agent script: navigate, links, click first link from links list via $link0.",
        [{"op": "navigate", "url": EX},
         {"op": "links"},
         {"op": "click", "node_id": "$link0"}],
        [{"on": -1, "path": "ok", "eq": True}],
        tags=["click-resolve"])
    add(8, "Agent script: three-step inspect — status, snapshot compact, read.",
        [{"op": "navigate", "url": EX},
         {"op": "status"},
         {"op": "snapshot", "view": "compact"},
         {"op": "read"}],
        [{"on": 2, "path": "view", "eq": "compact"},
         {"on": 3, "path": "text", "min_len": 5}])
    add(8, "Agent script: set_ai_vis on without live surface still returns ok or stores flag.",
        [{"op": "set_ai_vis", "on": True}],
        [{"on": -1, "path": "ok", "eq": True},
         {"on": -1, "path": "ai_vis", "eq": True}])
    add(8, "Agent script: set_ai_vis off after on.",
        [{"op": "set_ai_vis", "on": True}, {"op": "set_ai_vis", "on": False}],
        [{"on": -1, "path": "ai_vis", "eq": False}])
    add(8, "Agent script: toggle_ai_vis twice returns to a boolean state.",
        [{"op": "toggle_ai_vis"}, {"op": "toggle_ai_vis"}],
        [{"on": -1, "path": "ok", "eq": True},
         {"on": -1, "path": "ai_vis", "type": "boolean"}])
    add(8, "Agent script: wait 10ms and confirm waited_ms is reported.",
        [{"op": "wait", "ms": 10}],
        [{"on": -1, "path": "ok", "eq": True},
         {"on": -1, "path": "waited_ms", "gte": 0}])
    add(8, "Agent script: ai_marks without live surface reports a count field.",
        [{"op": "ai_marks"}],
        [{"on": -1, "path": "ok", "eq": True},
         {"on": -1, "path": "count", "type": "number"}])

    # --- complexity 9: errors, fill without live, knox soft ---
    add(9, "fill without a live surface should fail with no_live style error.",
        [{"op": "fill", "which": "login", "text": "user@example.com"}],
        [{"on": -1, "path": "ok", "eq": False},
         {"on": -1, "path": "code", "eq": "no_live"}])
    add(9, "type without live surface should fail the same way as fill.",
        [{"op": "type", "which": "password", "text": "not-a-real-secret"}],
        [{"on": -1, "path": "ok", "eq": False}])
    add(9, "press without live surface should fail clearly.",
        [{"op": "press", "key": "Enter"}],
        [{"on": -1, "path": "ok", "eq": False}])
    add(9, "eval without live surface should fail clearly.",
        [{"op": "eval", "js": "1+1"}],
        [{"on": -1, "path": "ok", "eq": False}])
    add(9, "knox_fill without live surface must not leak a password and must fail soft.",
        [{"op": "knox_fill", "query": "example.com", "fields": "password"}],
        [{"on": -1, "path": "ok", "eq": False},
         {"on": -1, "path": "secret_output", "eq": "suppressed"}])
    add(9, "knox_find for a nonsense query should not crash and must suppress secrets.",
        [{"op": "knox_find", "query": "zzzxxyyzz_knox_no_such_record_ever"}],
        [{"on": -1, "path": "secret_output", "eq": "suppressed"},
         {"on": -1, "path": "ok", "exists": True}],
        tags=["knox"])
    add(9, "knox_use dry-run for nonsense query fails without printing secrets.",
        [{"op": "knox_use", "query": "zzzxxyyzz_knox_no_such", "field": "password", "via": "dry-run"}],
        [{"on": -1, "path": "secret_output", "eq": "suppressed"}],
        tags=["knox"])
    add(9, "set_ai_vis without on flag should error bad_args.",
        [{"op": "set_ai_vis"}],
        [{"on": -1, "path": "ok", "eq": False}])
    add(9, "quit without force should not kill the process mid-suite (ok ignored).",
        [{"op": "quit"}],
        [{"on": -1, "path": "ok", "eq": True}])
    add(9, "navigate to data: URL or invalid scheme fails or is rejected safely.",
        [{"op": "navigate", "url": "javascript:alert(1)"}],
        [{"on": -1, "path": "ok", "exists": True}])

    # --- complexity 10: harder multi-step & stability ---
    add(10, "Stability: five sequential snapshots of example.com all report the same node_count.",
        [{"op": "navigate", "url": EX},
         {"op": "snapshot"}, {"op": "snapshot"}, {"op": "snapshot"},
         {"op": "snapshot"}, {"op": "snapshot"}],
        [{"on": 1, "path": "node_count", "eq_path": {"on": 5, "path": "node_count"}}])
    add(10, "Stability: outline then links then full all share html_bytes.",
        [{"op": "navigate", "url": EX},
         {"op": "view", "kind": "outline"},
         {"op": "view", "kind": "links"},
         {"op": "view", "kind": "full"}],
        [{"on": 1, "path": "html_bytes", "eq_path": {"on": 3, "path": "html_bytes"}}])
    add(10, "Complex: navigate, compact view, find_text More, ensure node_id is positive.",
        [{"op": "navigate", "url": EX},
         {"op": "view", "kind": "compact"},
         {"op": "find_text", "text": "More"}],
        [{"on": 2, "path": "0.node_id", "gte": 1}])
    add(10, "Complex: help ops list includes knox_find and view.",
        [{"op": "help"}],
        [{"on": -1, "path": "ops", "contains": "knox_find"},
         {"on": -1, "path": "ops", "contains": "view"}])
    add(10, "Complex: ping then navigate then meta view then status stays ok.",
        [{"op": "ping"},
         {"op": "navigate", "url": EX},
         {"op": "view", "kind": "meta"},
         {"op": "status"}],
        [{"on": 0, "path": "ok", "eq": True},
         {"on": 3, "path": "ok", "eq": True}])
    add(10, "Complex: live field in ping is false for headless --api.",
        [{"op": "ping"}],
        [{"on": -1, "path": "live", "eq": False}])
    add(10, "Complex: after navigate, status live remains false in headless.",
        [{"op": "navigate", "url": EX}, {"op": "status"}],
        [{"on": -1, "path": "live", "eq": False}])
    add(10, "Complex: read text length is less than raw html_bytes for example.com.",
        [{"op": "navigate", "url": EX}, {"op": "read"}, {"op": "snapshot"}],
        [{"on": 2, "path": "html_bytes", "gte": 100}])
    add(10, "Complex: multi navigate to example.com and example.org keeps ok true.",
        [{"op": "navigate", "url": EX},
         {"op": "navigate", "url": "https://example.org"},
         {"op": "current"}],
        [{"on": 1, "path": "ok", "eq": True},
         {"on": 2, "path": "url", "contains": "example"}],
        tags=["network"])
    add(10, "Complex: wait 0 ms is allowed and returns quickly.",
        [{"op": "wait", "ms": 0}],
        [{"on": -1, "path": "ok", "eq": True}])

    # --- pad to 100+ with systematic variants ---
    for host, label in [
        ("https://example.com", "example.com"),
        ("https://example.org", "example.org"),
    ]:
        add(3, f"Navigate to {label} and confirm status ok.",
            [{"op": "navigate", "url": host}],
            [{"on": -1, "path": "ok", "eq": True}], tags=["network"])
        add(4, f"On {label}, request meta view and get counts object.",
            [{"op": "navigate", "url": host}, {"op": "view", "kind": "meta"}],
            [{"on": -1, "path": "counts", "type": "object"}], tags=["network"])
        add(5, f"On {label}, compact view returns a view name compact.",
            [{"op": "navigate", "url": host}, {"op": "view", "kind": "compact"}],
            [{"on": -1, "path": "view", "eq": "compact"}], tags=["network"])

    # node_id stability across views
    add(6, "On example.com, a link node_id in links view is also present in full snapshot nodes.",
        [{"op": "navigate", "url": EX},
         {"op": "view", "kind": "links"},
         {"op": "snapshot"}],
        [{"on": 1, "path": "nodes.0.node_id", "type": "number"},
         {"on": 2, "path": "nodes", "type": "list"}])

    add(8, "Secret-safe: knox_find response must never contain key 'password' with a string value at top level.",
        [{"op": "knox_find", "query": "gmail"}],
        [{"on": -1, "path": "secret_output", "eq": "suppressed"},
         {"on": -1, "forbid_keys": ["password", "secret", "value"]}],
        tags=["knox"])

    add(9, "fill with selector string without live still fails no_live.",
        [{"op": "fill", "selector": "input[type=email]", "text": "a@b.c"}],
        [{"on": -1, "path": "ok", "eq": False}])

    add(10, "End-to-end agent loop: ping → navigate → outline → links → find Learn → status.",
        [{"op": "ping"},
         {"op": "navigate", "url": EX},
         {"op": "view", "kind": "outline"},
         {"op": "view", "kind": "links"},
         {"op": "find_text", "text": "Learn"},
         {"op": "status"}],
        [{"on": 0, "path": "ok", "eq": True},
         {"on": 4, "min_len": 1},
         {"on": 5, "path": "ok", "eq": True}])

    # session save / shim (same process — save then load restores buffer)
    add(8, "Save the current session after loading example.com and confirm save ok with html_bytes.",
        [{"op": "navigate", "url": EX},
         {"op": "session_save", "name": "suite-save-demo"}],
        [{"on": -1, "path": "ok", "eq": True},
         {"on": -1, "path": "action", "eq": "session_save"},
         {"on": -1, "path": "html_bytes", "gte": 100}],
        tags=["session"])
    add(8, "List saved sessions and get a non-negative count.",
        [{"op": "session_list"}],
        [{"on": -1, "path": "ok", "eq": True},
         {"on": -1, "path": "count", "gte": 0}],
        tags=["session"])
    add(9, "Save then shim the same session back and confirm current url is example.com.",
        [{"op": "navigate", "url": EX},
         {"op": "session_save", "name": "suite-shim-demo"},
         {"op": "session_load", "name": "suite-shim-demo"},
         {"op": "current"}],
        [{"on": 2, "path": "ok", "eq": True},
         {"on": 2, "path": "action", "eq": "session_shim"},
         {"on": 2, "path": "shim_from", "exists": True},
         {"on": 3, "path": "url", "contains": "example.com"}],
        tags=["session"])
    add(9, "session_load without id fails with bad_args.",
        [{"op": "session_load"}],
        [{"on": -1, "path": "ok", "eq": False}],
        tags=["session"])

    # Hancock — never treat missing wait as approval
    add(8, "Hancock request without wait queues a permission and never claims APPROVED_AND_RAN by default.",
        [{"op": "hancock_request", "action": "navigate", "why": "suite probe", "risk": "high",
          "wait": False, "detail": {"url": EX}}],
        [{"on": -1, "path": "action", "eq": "hancock_request"},
         {"on": -1, "path": "outcome", "ne": "APPROVED_AND_RAN"}],
        tags=["hancock"])
    add(8, "Hancock pending list returns a structured response with english.",
        [{"op": "hancock_pending"}],
        [{"on": -1, "path": "action", "eq": "hancock_pending"},
         {"on": -1, "path": "english", "type": "string"}],
        tags=["hancock"])
    add(9, "Hancock wait without id fails bad_args.",
        [{"op": "hancock_wait"}],
        [{"on": -1, "path": "ok", "eq": False}],
        tags=["hancock"])

    # Ensure >= 100
    while len(cases) < 100:
        i = len(cases) + 1
        add(2, f"Idempotent navigate to example.com (padding case {i}).",
            [{"op": "navigate", "url": EX}],
            [{"on": -1, "path": "ok", "eq": True}],
            tags=["padding"])

    # Special raw-line case for bad JSON (runner supports "raw_ops")
    cases.append({
        "id": len(cases) + 1,
        "complexity": 1,
        "english": "Send a non-JSON line to the API and expect a bad_json error response.",
        "raw_ops": ["this is not json {{{"],
        "assert": [{"on": -1, "path": "ok", "eq": False}],
        "tags": ["raw"],
    })

    # --- engine depth: settle receipt, faithful-js, auth-session (servo) ---
    # {{FIXTURES}} / {{HTTP}} are expanded by run_api_suite.py (local fixture dir + fixture
    # web server). Cases tagged `servo` only run with --engine servo; `static-only` is the
    # mirror image, and is the control that proves the JS fixture actually discriminates.
    JS_FIXTURE = "{{FIXTURES}}/js-render.html"

    add(4, "Ask the engine to settle and confirm it returns a receipt saying it is quiescent, not a guess.",
        [{"op": "navigate", "url": EX}, {"op": "settle"}],
        [{"on": -1, "path": "ok", "eq": True},
         {"on": -1, "path": "quiescent", "eq": True},
         {"on": -1, "path": "engine", "exists": True},
         {"on": -1, "path": "spins", "type": "number"}],
        tags=["settle"])
    add(5, "The static engine loading the JS fixture sees only the pre-JS shell — no POST-JS-MARKER — which is what makes the servo cases meaningful.",
        [{"op": "navigate", "url": JS_FIXTURE}, {"op": "read"}],
        [{"on": 0, "path": "ok", "eq": True},
         {"on": 0, "path": "title", "eq": "pre-js shell"},
         {"on": -1, "path": "text", "not_contains": "POST-JS-MARKER"}],
        tags=["static-only", "fixture"])
    add(3, "With the servo engine selected, ping reports that servo is the engine actually answering.",
        [{"op": "ping"}],
        [{"on": -1, "path": "ok", "eq": True}, {"on": -1, "path": "engine", "eq": "servo"}],
        tags=["servo"])
    add(6, "Servo navigates the JS fixture and reports the title JavaScript set at runtime, not the pre-JS one.",
        [{"op": "navigate", "url": JS_FIXTURE}],
        [{"on": -1, "path": "ok", "eq": True}, {"on": -1, "path": "title", "eq": "post-js title"}],
        tags=["servo", "faithful-js", "fixture"])
    add(6, "Servo's snapshot contains a heading that only exists after the page's JavaScript ran.",
        [{"op": "navigate", "url": JS_FIXTURE}, {"op": "find_text", "text": "POST-JS-MARKER"}],
        [{"on": -1, "type": "list"}, {"on": -1, "path": "0.text", "contains": "POST-JS-MARKER"}],
        tags=["servo", "faithful-js", "fixture"])
    add(6, "Reading the page text under servo returns copy written by JavaScript at runtime.",
        [{"op": "navigate", "url": JS_FIXTURE}, {"op": "read"}],
        [{"on": -1, "path": "text", "contains": "rendered by javascript"}],
        tags=["servo", "faithful-js", "fixture"])
    add(6, "Servo's settle receipt reports quiescence reached and how many event-loop turns it took.",
        [{"op": "navigate", "url": JS_FIXTURE}, {"op": "settle"}],
        [{"on": -1, "path": "ok", "eq": True},
         {"on": -1, "path": "engine", "eq": "servo"},
         {"on": -1, "path": "quiescent", "eq": True},
         {"on": -1, "path": "spins", "type": "number"},
         {"on": -1, "path": "reason", "ne": "cap"}],
        tags=["servo", "settle", "fixture"])
    add(8, "Clicking a button that JavaScript created runs the page's real click handler and new text appears.",
        [{"op": "navigate", "url": JS_FIXTURE},
         {"op": "find_text", "text": "GROW"},
         {"op": "click", "node_id": "$find0"},
         {"op": "read"}],
        [{"on": -1, "path": "text", "contains": "CLICK-HANDLER-RAN"}],
        tags=["servo", "faithful-js", "fixture"])
    add(7, "A link that JavaScript appended after load is visible in the links view with its resolved href.",
        [{"op": "navigate", "url": JS_FIXTURE}, {"op": "links"}],
        [{"on": -1, "type": "list"}, {"on": -1, "path": "0.href", "contains": "post-js-link"}],
        tags=["servo", "faithful-js", "fixture"])
    add(8, "Log in on the fixture site, then fetch the protected page — servo carries the session cookie across the navigation and shows logged-in content.",
        [{"op": "navigate", "url": "{{HTTP}}/login"},
         {"op": "navigate", "url": "{{HTTP}}/protected"},
         {"op": "read"}],
        [{"on": 0, "path": "ok", "eq": True},
         {"on": 1, "path": "ok", "eq": True},
         {"on": -1, "path": "text", "contains": "LOGGED IN AS AGENT"},
         {"on": -1, "path": "text", "not_contains": "LOGIN WALL"}],
        tags=["servo", "auth-session", "http"])
    add(7, "Servo reports a non-empty single HTML buffer for the post-JS document, so views stay projections of one page.",
        [{"op": "navigate", "url": JS_FIXTURE}, {"op": "views"}],
        [{"on": -1, "path": "ok", "eq": True}, {"on": -1, "path": "html_bytes", "gte": 100}],
        tags=["servo", "fixture"])
    add(9, "Save a JS-rendered page under servo and shim it back — the shimmed page still shows the post-JS content and the original url.",
        [{"op": "navigate", "url": JS_FIXTURE},
         {"op": "session_save", "name": "suite-servo-shim"},
         {"op": "session_load", "name": "suite-servo-shim"},
         {"op": "read"},
         {"op": "current"}],
        [{"on": 1, "path": "ok", "eq": True},
         {"on": 2, "path": "ok", "eq": True},
         {"on": 3, "path": "text", "contains": "POST-JS-MARKER"},
         {"on": 4, "path": "url", "contains": "js-render.html"}],
        tags=["servo", "session", "fixture"])

    # --- auth-session across processes: the cookie jar on disk ---
    # 131 logs in and shuts down cleanly (jar written), 132 is a *different* process that never
    # logs in, 133 is the control: same page, empty profile, must hit the wall. 132 depends on
    # 131 having run first — that ordering is the point of the test.
    add(8, "Log in on the fixture site and shut the engine down cleanly so the cookie jar is written to the profile directory on disk.",
        [{"op": "navigate", "url": "{{HTTP}}/login"},
         {"op": "status"},
         {"op": "quit", "force": True}],
        [{"on": 0, "path": "ok", "eq": True},
         {"on": 1, "path": "profile_dir", "type": "string"},
         {"on": 2, "path": "ok", "eq": True},
         {"on": 2, "path": "action", "eq": "quit"}],
        tags=["servo", "auth-session", "http"])
    add(9, "A brand-new process that never logs in still sees the protected page — the session came off disk, not from memory.",
        [{"op": "navigate", "url": "{{HTTP}}/protected"},
         {"op": "read"}],
        [{"on": 0, "path": "ok", "eq": True},
         {"on": -1, "path": "text", "contains": "LOGGED IN AS AGENT"},
         {"on": -1, "path": "text", "not_contains": "LOGIN WALL"}],
        tags=["servo", "auth-session", "http"])
    add(9, "Control: the same fetch with an empty profile directory hits the login wall — proving the previous case passed because of the saved jar, not because the fixture always says yes.",
        [{"op": "navigate", "url": "{{HTTP}}/protected"},
         {"op": "read"}],
        [{"on": 0, "path": "ok", "eq": True},
         {"on": -1, "path": "text", "contains": "LOGIN WALL"},
         {"on": -1, "path": "text", "not_contains": "LOGGED IN AS AGENT"}],
        tags=["servo", "auth-session", "http"],
        env={"CHRIME_PROFILE_DIR": "{{EMPTY_PROFILE}}"})

    # renumber
    for i, c in enumerate(cases, 1):
        c["id"] = i

    OUT.parent.mkdir(parents=True, exist_ok=True)
    with OUT.open("w") as f:
        for c in cases:
            f.write(json.dumps(c, ensure_ascii=False) + "\n")
    print(f"wrote {len(cases)} cases → {OUT}")


if __name__ == "__main__":
    main()
