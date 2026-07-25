//! Knox integration — use passwords without ever printing them.
//!
//! Policy (matches Knox itself):
//! - Touch ID unlock stays Knox's boundary.
//! - Secrets never appear in agent JSON, logs, the side panel, or stdout of `chrime --api`.
//! - Preferred path: unlock in-process via Knox's Python library, inject into the live
//!   WebView field (browser-fill). Avoids OS `--type-into-frontmost` focus races.
//! - Fallback path: shell out to `knox use --type-into-frontmost|--paste-into-frontmost`.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::process::{Command, Stdio};

/// Metadata-only match from `knox find` / the bridge. Never contains a password.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnoxMatch {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnoxFindResult {
    pub ok: bool,
    pub query: String,
    pub matches: Vec<KnoxMatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Always true in responses so agents can assert redaction.
    pub secret_output: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnoxFillResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub secret_output: &'static str,
}

const SECRET_SUPPRESSED: &str = "suppressed";

/// Python bridge: talks to Knox's library. Secrets only travel on the helper's stdout
/// as a single JSON object that Chrime consumes and never re-emits.
const BRIDGE: &str = r#"
import json, sys
try:
    from knox.cli import (
        DEFAULT_BIOMETRIC_KEYCHAIN_SERVICE,
        DEFAULT_KEYCHAIN_ACCOUNT,
        DEFAULT_STORE_PATH,
        DEFAULT_TOUCH_ID_CACHE_SECONDS,
        _read_unlocked_store,
        find_secret_records,
        secret_record_field,
    )
except Exception as e:
    json.dump({"ok": False, "error": f"knox import failed: {e}"}, sys.stdout)
    sys.exit(2)

def meta(rec):
    login = rec.get("login") or secret_record_field(rec, "login") or None
    url = rec.get("url") or secret_record_field(rec, "url") or None
    return {
        "title": str(rec.get("title") or "(untitled)"),
        "login": str(login) if login else None,
        "url": str(url) if url else None,
        "id": str(rec.get("id")) if rec.get("id") else None,
    }

req = json.loads(sys.stdin.read() or "{}")
op = req.get("op") or "find"
query = (req.get("query") or "").strip()
limit = int(req.get("limit") or 10)
field = (req.get("field") or "password").strip()
force = bool(req.get("force_touchid"))
record_id = (req.get("id") or "").strip() or None

code, output, store = _read_unlocked_store(
    service=DEFAULT_BIOMETRIC_KEYCHAIN_SERVICE,
    account=DEFAULT_KEYCHAIN_ACCOUNT,
    store_path=DEFAULT_STORE_PATH,
    force_touchid=force,
    cache_seconds=DEFAULT_TOUCH_ID_CACHE_SECONDS,
)
if code != 0 or store is None:
    json.dump({"ok": False, "error": (output or "unlock failed").splitlines()[0][:200]}, sys.stdout)
    sys.exit(1)

if op == "find":
    matches = find_secret_records(store, query, limit=limit)
    json.dump({"ok": True, "query": query, "matches": [meta(m) for m in matches]}, sys.stdout)
    sys.exit(0)

if op == "get_field":
    if not query and not record_id:
        json.dump({"ok": False, "error": "query or id required"}, sys.stdout)
        sys.exit(2)
    matches = find_secret_records(store, query or record_id, limit=8)
    if record_id:
        matches = [m for m in matches if str(m.get("id") or "") == record_id] or matches
        # also allow exact id search across store
        if not matches:
            recs = store.get("records") or []
            matches = [m for m in recs if isinstance(m, dict) and str(m.get("id") or "") == record_id]
    if not matches:
        json.dump({"ok": False, "error": f"no match for {query or record_id}"}, sys.stdout)
        sys.exit(1)
    if len(matches) > 1 and not record_id:
        json.dump({
            "ok": False,
            "error": "ambiguous — narrow query or pass id",
            "matches": [meta(m) for m in matches[:5]],
        }, sys.stdout)
        sys.exit(2)
    rec = matches[0]
    value = secret_record_field(rec, field)
    if not value:
        json.dump({"ok": False, "error": f"field {field} missing", "record": meta(rec)["title"]}, sys.stdout)
        sys.exit(1)
    # value is intentional — parent (chrime) consumes and never re-logs it
    json.dump({
        "ok": True,
        "field": field,
        "value": value,
        "record": meta(rec)["title"],
        "login": meta(rec).get("login"),
        "id": meta(rec).get("id"),
    }, sys.stdout)
    sys.exit(0)

json.dump({"ok": False, "error": f"unknown op {op}"}, sys.stdout)
sys.exit(2)
"#;

fn run_bridge(req: &serde_json::Value) -> Result<serde_json::Value, String> {
    let mut child = Command::new("python3")
        .args(["-c", BRIDGE])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn python3/knox bridge: {e}"))?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| "bridge stdin missing".to_string())?;
        let body = serde_json::to_vec(req).map_err(|e| e.to_string())?;
        stdin.write_all(&body).map_err(|e| e.to_string())?;
    }

    let out = child
        .wait_with_output()
        .map_err(|e| format!("knox bridge wait: {e}"))?;

    // stderr may contain Touch ID UI noise — never treat as secret channel
    let stdout = String::from_utf8_lossy(&out.stdout);
    if stdout.trim().is_empty() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(if err.trim().is_empty() {
            format!("knox bridge empty (exit {})", out.status)
        } else {
            err.lines()
                .next()
                .unwrap_or("knox bridge failed")
                .to_string()
        });
    }

    serde_json::from_str(stdout.trim()).map_err(|e| {
        // If parse fails, do NOT include stdout (might contain a secret value).
        format!("knox bridge bad json: {e}")
    })
}

/// Search Knox for credential metadata (no secret values).
pub fn find(query: &str, limit: usize) -> KnoxFindResult {
    let q = query.trim();
    if q.is_empty() {
        return KnoxFindResult {
            ok: false,
            query: q.into(),
            matches: vec![],
            error: Some("query required".into()),
            secret_output: SECRET_SUPPRESSED,
        };
    }
    let limit = limit.clamp(1, 25);

    // Prefer official CLI (same path humans use); fall back to in-process bridge.
    if let Ok(cli) = find_via_cli(q, limit) {
        return cli;
    }

    match run_bridge(&serde_json::json!({
        "op": "find",
        "query": q,
        "limit": limit,
    })) {
        Ok(v) => {
            if v.get("ok").and_then(|o| o.as_bool()) != Some(true) {
                return KnoxFindResult {
                    ok: false,
                    query: q.into(),
                    matches: parse_matches(&v),
                    error: Some(
                        v.get("error")
                            .and_then(|e| e.as_str())
                            .unwrap_or("find failed")
                            .into(),
                    ),
                    secret_output: SECRET_SUPPRESSED,
                };
            }
            KnoxFindResult {
                ok: true,
                query: q.into(),
                matches: parse_matches(&v),
                error: None,
                secret_output: SECRET_SUPPRESSED,
            }
        }
        Err(e) => KnoxFindResult {
            ok: false,
            query: q.into(),
            matches: vec![],
            error: Some(e),
            secret_output: SECRET_SUPPRESSED,
        },
    }
}

fn find_via_cli(query: &str, limit: usize) -> Result<KnoxFindResult, String> {
    let out = Command::new("knox")
        .args(["find", query, "--limit", &limit.to_string()])
        .output()
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout);
    let err = String::from_utf8_lossy(&out.stderr);
    let combined = if text.trim().is_empty() {
        err.to_string()
    } else {
        text.to_string()
    };
    if combined.contains("FAIL")
        || combined.contains("TIMEOUT")
        || combined.contains("not accepted")
    {
        return Err(combined
            .lines()
            .find(|l| l.contains("FAIL") || l.contains("biometric") || l.contains("TIMEOUT"))
            .unwrap_or("knox find failed — approve Touch ID")
            .to_string());
    }
    if !out.status.success() && !combined.contains("match") {
        return Err(combined
            .lines()
            .next()
            .unwrap_or("knox find failed")
            .to_string());
    }
    Ok(KnoxFindResult {
        ok: true,
        query: query.into(),
        matches: parse_cli_find(&combined),
        error: None,
        secret_output: SECRET_SUPPRESSED,
    })
}

/// Parse `knox find` human output. Format:
/// `1. Title | login: x | url: y | id: keeper:…`
fn parse_cli_find(text: &str) -> Vec<KnoxMatch> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let rest = match line.split_once(". ") {
            Some((n, r)) if n.chars().all(|c| c.is_ascii_digit()) => r,
            _ => continue,
        };
        let mut title = rest.to_string();
        let mut login = None;
        let mut url = None;
        let mut id = None;
        for part in rest.split(" | ") {
            if let Some(v) = part.strip_prefix("login: ") {
                login = Some(v.to_string());
                if let Some(t) = title.split(" | login:").next() {
                    title = t.to_string();
                }
            } else if let Some(v) = part.strip_prefix("url: ") {
                url = Some(v.to_string());
            } else if let Some(v) = part.strip_prefix("id: ") {
                id = Some(v.to_string());
            }
        }
        // title is first segment before any |
        title = rest.split(" | ").next().unwrap_or(rest).trim().to_string();
        out.push(KnoxMatch {
            title,
            login,
            url,
            id,
        });
    }
    out
}

fn parse_matches(v: &serde_json::Value) -> Vec<KnoxMatch> {
    v.get("matches")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    Some(KnoxMatch {
                        title: m.get("title")?.as_str()?.to_string(),
                        login: m
                            .get("login")
                            .and_then(|x| x.as_str())
                            .map(|s| s.to_string()),
                        url: m.get("url").and_then(|x| x.as_str()).map(|s| s.to_string()),
                        id: m.get("id").and_then(|x| x.as_str()).map(|s| s.to_string()),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Unlock one field from Knox. Caller must NOT log or serialize `value` outward.
/// Returns (record_title, field_value).
pub fn unlock_field(
    query: &str,
    field: &str,
    id: Option<&str>,
) -> Result<(String, String), KnoxFillResult> {
    let field = match field {
        "login" | "password" | "url" => field,
        _ => {
            return Err(KnoxFillResult {
                ok: false,
                record: None,
                field: Some(field.into()),
                action: None,
                error: Some("field must be login|password|url".into()),
                secret_output: SECRET_SUPPRESSED,
            });
        }
    };
    match run_bridge(&serde_json::json!({
        "op": "get_field",
        "query": query,
        "field": field,
        "id": id,
    })) {
        Ok(v) => {
            if v.get("ok").and_then(|o| o.as_bool()) != Some(true) {
                return Err(KnoxFillResult {
                    ok: false,
                    record: v
                        .get("record")
                        .and_then(|r| r.as_str())
                        .map(|s| s.to_string()),
                    field: Some(field.into()),
                    action: None,
                    error: Some(
                        v.get("error")
                            .and_then(|e| e.as_str())
                            .unwrap_or("unlock failed")
                            .into(),
                    ),
                    secret_output: SECRET_SUPPRESSED,
                });
            }
            let title = v
                .get("record")
                .and_then(|r| r.as_str())
                .unwrap_or("(untitled)")
                .to_string();
            let value = v
                .get("value")
                .and_then(|x| x.as_str())
                .ok_or_else(|| KnoxFillResult {
                    ok: false,
                    record: Some(title.clone()),
                    field: Some(field.into()),
                    action: None,
                    error: Some("bridge returned no value".into()),
                    secret_output: SECRET_SUPPRESSED,
                })?
                .to_string();
            Ok((title, value))
        }
        Err(e) => Err(KnoxFillResult {
            ok: false,
            record: None,
            field: Some(field.into()),
            action: None,
            error: Some(e),
            secret_output: SECRET_SUPPRESSED,
        }),
    }
}

/// Build JS that sets a login/password field without returning the value.
/// `value` is JSON-escaped into the script; do not log `js`.
pub fn fill_field_js(field: &str, value: &str) -> String {
    let v = serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into());
    let which = serde_json::to_string(field).unwrap_or_else(|_| "\"password\"".into());
    format!(
        r#"(function(){{
  const value = {v};
  const which = {which};
  function setNative(el, val) {{
    el.focus();
    try {{
      const proto = window.HTMLInputElement && HTMLInputElement.prototype;
      const desc = proto && Object.getOwnPropertyDescriptor(proto, 'value');
      if (desc && desc.set) desc.set.call(el, val);
      else el.value = val;
    }} catch (e) {{ el.value = val; }}
    el.dispatchEvent(new Event('input', {{bubbles:true}}));
    el.dispatchEvent(new Event('change', {{bubbles:true}}));
    el.dispatchEvent(new KeyboardEvent('keyup', {{bubbles:true}}));
  }}
  function pick() {{
    if (which === 'password') {{
      return document.querySelector('input[type="password"]:not([disabled])');
    }}
    if (which === 'login') {{
      const sels = [
        'input[type="email"]',
        'input[name*="email" i]',
        'input[id*="email" i]',
        'input[autocomplete="username"]',
        'input[name*="user" i]',
        'input[id*="user" i]',
        'input[type="text"]',
        'input:not([type])'
      ];
      for (const s of sels) {{
        const el = document.querySelector(s + ':not([disabled])');
        if (el && el.type !== 'password' && el.type !== 'hidden') return el;
      }}
      return null;
    }}
    return null;
  }}
  const el = pick();
  if (!el) return 'no-field';
  setNative(el, value);
  return 'filled:' + (el.type || el.tagName);
}})()"#
    )
}

/// Fallback: Knox CLI types/pastes into the frontmost app (or activates target_app first).
/// Never returns the secret.
pub fn use_frontmost(
    query: &str,
    field: &str,
    mode: &str, // "type" | "paste" | "dry-run"
    target_app: Option<&str>,
) -> KnoxFillResult {
    let mut cmd = Command::new("knox");
    cmd.arg("use").arg(query).arg("--field").arg(field);
    match mode {
        "paste" => {
            cmd.arg("--paste-into-frontmost");
        }
        "dry-run" => {
            cmd.arg("--dry-run");
        }
        _ => {
            cmd.arg("--type-into-frontmost");
        }
    }
    if let Some(app) = target_app {
        cmd.arg("--target-app").arg(app);
    }
    match cmd.output() {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            let err = String::from_utf8_lossy(&out.stderr);
            let combined = if text.trim().is_empty() {
                err.to_string()
            } else {
                text.to_string()
            };
            // Knox CLI never prints secrets — safe to surface status lines.
            let ok = out.status.success()
                && !combined.contains("REFUSED")
                && !combined.contains("NOT FOUND")
                && !combined.contains("AMBIGUOUS")
                && !combined.contains("FIELD MISSING");
            KnoxFillResult {
                ok,
                record: extract_record_line(&combined),
                field: Some(field.into()),
                action: Some(format!("knox-cli:{mode}")),
                error: if ok {
                    None
                } else {
                    Some(
                        combined
                            .lines()
                            .find(|l| !l.is_empty())
                            .unwrap_or("knox use failed")
                            .to_string(),
                    )
                },
                secret_output: SECRET_SUPPRESSED,
            }
        }
        Err(e) => KnoxFillResult {
            ok: false,
            record: None,
            field: Some(field.into()),
            action: None,
            error: Some(format!("knox CLI: {e}")),
            secret_output: SECRET_SUPPRESSED,
        },
    }
}

fn extract_record_line(text: &str) -> Option<String> {
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("- record: ") {
            return Some(rest.to_string());
        }
    }
    None
}

/// Guess a Knox query from a page URL (hostname without www).
pub fn query_from_url(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .map(|h| h.trim_start_matches("www.").to_string())
        .unwrap_or_default()
}
