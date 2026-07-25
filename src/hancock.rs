//! Native Hancock bridge — ask the human to sign before consequential Chrime actions.
//!
//! Uses the local `hancock` CLI (sign-to-approve). Chrime never pretends approval:
//! only `OUTCOME: APPROVED` / exit 0 from `hancock wait` means the human signed.
//!
//! Breadcrumb kinds: `hancock_request`, `hancock_wait`, `hancock_pending` (see docs/BREADCRUMBS.md).

use serde_json::{json, Value};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::trace;

#[derive(Debug, Clone)]
pub struct HancockResult {
    pub ok: bool,
    pub outcome: String,
    pub hancock_id: Option<String>,
    pub risk: String,
    pub action: String,
    pub english: String,
    pub raw: String,
    pub error: Option<String>,
}

impl HancockResult {
    pub fn to_json(&self) -> String {
        json!({
            "ok": self.ok,
            "action": "hancock_request",
            "outcome": self.outcome,
            "hancock_id": self.hancock_id,
            "risk": self.risk,
            "chrime_action": self.action,
            "english": self.english,
            "error": self.error,
            // never dump full raw if it might grow — truncate for agents
            "raw_tail": tail(&self.raw, 800),
            "secret_output": "suppressed",
        })
        .to_string()
    }
}

fn tail(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        s[s.len() - n..].to_string()
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn which_hancock() -> Result<String, String> {
    if let Ok(p) = std::env::var("HANCOCK_BIN") {
        if !p.is_empty() {
            return Ok(p);
        }
    }
    which("hancock")
        .ok_or_else(|| "hancock CLI not found on PATH (install hancock or set HANCOCK_BIN)".into())
}

fn which(bin: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(bin);
        if candidate.is_file() {
            return Some(candidate.display().to_string());
        }
    }
    None
}

/// Build a shell command that records the permit when Hancock runs it after signature.
/// The human signs; on approval this runs under their signature and leaves an audit line.
fn permit_command(action: &str, detail: &Value, permit_token: &str) -> String {
    // Keep command short and safe — no network, no secrets.
    // printf is always allow-or-sign; we force risk high/critical for real waits.
    let detail_s = serde_json::to_string(detail).unwrap_or_else(|_| "{}".into());
    let detail_s = detail_s.replace('\'', "");
    format!(
        "printf '%s\\n' 'CHRIME_PERMIT' 'token={permit_token}' 'action={action}' 'detail={detail_s}' 'ts={ts}'",
        permit_token = shell_single(permit_token),
        action = shell_single(action),
        detail_s = shell_single(&detail_s),
        ts = now_ms(),
    )
}

fn shell_single(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "")
}

/// Parse `queued req_XXXX` from hancock add output.
pub fn parse_queued_id(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        // "⏸ queued req_1785… (risk=medium)"
        if let Some(idx) = line.find("req_") {
            let rest = &line[idx..];
            let id: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if id.starts_with("req_") {
                return Some(id);
            }
        }
    }
    None
}

/// Queue a permission request with Hancock and optionally wait for the human signature.
// ponytail: 8 flat args beats inventing a Request struct used at exactly one call site.
#[allow(clippy::too_many_arguments)]
pub fn request(
    sess_root: &str,
    action: &str,
    why: &str,
    risk: &str,
    telos: Option<&str>,
    detail: Value,
    wait: bool,
    timeout_secs: u64,
) -> HancockResult {
    let bin = match which_hancock() {
        Ok(b) => b,
        Err(e) => {
            return HancockResult {
                ok: false,
                outcome: "HANCOCK_MISSING".into(),
                hancock_id: None,
                risk: risk.into(),
                action: action.into(),
                english: format!("Cannot ask Hancock: {e}"),
                raw: String::new(),
                error: Some(e),
            };
        }
    };

    let permit_token = format!("cpt_{}", now_ms());
    let cmd = permit_command(action, &detail, &permit_token);
    let why = if why.trim().is_empty() {
        format!("Chrime asks permission for action `{action}`")
    } else {
        why.to_string()
    };
    let risk = normalize_risk(risk);
    let telos = telos.unwrap_or("chrime agent-native browser control; human co-surf approval");

    let mut args = vec![
        "add".to_string(),
        cmd.clone(),
        "-why".into(),
        why.clone(),
        "-risk".into(),
        risk.clone(),
        "-as".into(),
        "chrime".into(),
        "--source".into(),
        "chrime".into(),
        "--telos".into(),
        telos.into(),
        "--rationale".into(),
        format!(
            "Chrime session {sess_root} requests human permission.\naction={action}\ndetail={detail}\npermit_token={permit_token}",
            detail = detail
        ),
    ];
    if let Ok(cwd) = std::env::current_dir() {
        args.push("-cwd".into());
        args.push(cwd.display().to_string());
    }

    let add = Command::new(&bin).args(&args).output();
    let add = match add {
        Ok(o) => o,
        Err(e) => {
            return HancockResult {
                ok: false,
                outcome: "HANCOCK_SPAWN_FAILED".into(),
                hancock_id: None,
                risk: risk.clone(),
                action: action.into(),
                english: format!("Failed to spawn hancock: {e}"),
                raw: String::new(),
                error: Some(e.to_string()),
            };
        }
    };

    let stdout = String::from_utf8_lossy(&add.stdout).to_string();
    let stderr = String::from_utf8_lossy(&add.stderr).to_string();
    let raw = format!("{stdout}{stderr}");
    let hid = parse_queued_id(&raw);

    let crumb = format!(
        "{sess_root}.HANCOCK.{}",
        hid.as_deref().unwrap_or("unknown")
    );
    trace::emit(
        &crumb,
        Some(sess_root),
        "hancock_request",
        &format!("Asked Hancock for permission to `{action}` (risk={risk}). why: {why}"),
        json!({
            "hancock_id": hid,
            "action": action,
            "risk": risk,
            "why": why,
            "wait": wait,
            "permit_token": permit_token,
        }),
    );

    // Auto-ran under license? (unlikely for high risk echo with critical)
    if raw.contains("auto-approved") || raw.contains("✓ auto-approved") {
        return HancockResult {
            ok: true,
            outcome: "AUTO_APPROVED_AND_RAN".into(),
            hancock_id: hid,
            risk,
            action: action.into(),
            english: format!(
                "Hancock auto-approved action `{action}` under the local license (no human wait)."
            ),
            raw,
            error: None,
        };
    }

    let Some(id) = hid.clone() else {
        return HancockResult {
            ok: false,
            outcome: "QUEUE_FAILED".into(),
            hancock_id: None,
            risk,
            action: action.into(),
            english: "Hancock did not return a request id — see raw_tail.".into(),
            raw,
            error: Some("no req_ id in hancock output".into()),
        };
    };

    if !wait {
        return HancockResult {
            ok: true,
            outcome: "QUEUED".into(),
            hancock_id: Some(id.clone()),
            risk,
            action: action.into(),
            english: format!(
                "Permission queued as {id}. Human must sign in Hancock TUI. Call hancock_wait with this id — STILL_PENDING is not approval."
            ),
            raw,
            error: None,
        };
    }

    // Block until human signs (or timeout).
    wait_for(sess_root, &bin, &id, action, &risk, timeout_secs, &raw)
}

pub fn wait_for(
    sess_root: &str,
    bin: &str,
    id: &str,
    action: &str,
    risk: &str,
    timeout_secs: u64,
    prior_raw: &str,
) -> HancockResult {
    let timeout = timeout_secs.clamp(5, 86_400);
    let wait = Command::new(bin)
        .args(["wait", id, "--timeout", &timeout.to_string()])
        .output();

    let wait = match wait {
        Ok(o) => o,
        Err(e) => {
            return HancockResult {
                ok: false,
                outcome: "WAIT_SPAWN_FAILED".into(),
                hancock_id: Some(id.into()),
                risk: risk.into(),
                action: action.into(),
                english: format!("hancock wait failed to spawn: {e}"),
                raw: prior_raw.into(),
                error: Some(e.to_string()),
            };
        }
    };

    let stdout = String::from_utf8_lossy(&wait.stdout).to_string();
    let stderr = String::from_utf8_lossy(&wait.stderr).to_string();
    let raw = format!("{prior_raw}\n--- wait ---\n{stdout}{stderr}");
    let code = wait.status.code().unwrap_or(-1);

    let crumb = format!("{sess_root}.HANCOCK.{id}.WAIT");
    let (ok, outcome, english) = if code == 0 {
        (
            true,
            "APPROVED_AND_RAN".to_string(),
            format!(
                "Human signed Hancock request {id}. Action `{action}` is permitted (outcome APPROVED_AND_RAN)."
            ),
        )
    } else if raw.to_ascii_lowercase().contains("still_pending")
        || raw.to_ascii_lowercase().contains("pending")
            && !raw.to_ascii_lowercase().contains("denied")
    {
        (
            false,
            "STILL_PENDING".to_string(),
            format!(
                "Hancock request {id} is STILL_PENDING — human has not decided. Call hancock_wait again; do NOT treat as approval."
            ),
        )
    } else if raw.to_ascii_lowercase().contains("denied")
        || raw.to_ascii_lowercase().contains("skipped")
    {
        (
            false,
            "DENIED".to_string(),
            format!(
                "Human denied or skipped Hancock request {id}. Do not proceed with `{action}`."
            ),
        )
    } else if raw.to_ascii_lowercase().contains("expired") {
        (
            false,
            "EXPIRED".to_string(),
            format!("Hancock request {id} expired. Queue a new hancock_request."),
        )
    } else {
        (
            false,
            format!("WAIT_EXIT_{code}"),
            format!(
                "Hancock wait on {id} exited {code}. Do not assume approval. raw_tail has detail."
            ),
        )
    };

    trace::emit(
        &crumb,
        Some(sess_root),
        "hancock_wait",
        &english,
        json!({
            "hancock_id": id,
            "outcome": outcome,
            "exit_code": code,
            "action": action,
        }),
    );

    let err = if ok {
        None
    } else {
        Some(format!("outcome={outcome}"))
    };
    HancockResult {
        ok,
        outcome,
        hancock_id: Some(id.into()),
        risk: risk.into(),
        action: action.into(),
        english,
        raw,
        error: err,
    }
}

fn normalize_risk(r: &str) -> String {
    match r.trim().to_ascii_lowercase().as_str() {
        "low" | "medium" | "high" | "critical" => r.trim().to_ascii_lowercase(),
        "" => "high".into(),
        _ => "high".into(),
    }
}

/// List pending Hancock requests (human tray).
pub fn pending(sess_root: &str) -> Value {
    let bin = match which_hancock() {
        Ok(b) => b,
        Err(e) => {
            return json!({
                "ok": false,
                "action": "hancock_pending",
                "error": e,
                "english": "Hancock CLI missing — cannot list pending permissions.",
            });
        }
    };
    let out = Command::new(&bin).args(["list"]).output();
    match out {
        Ok(o) => {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            trace::emit(
                &format!("{sess_root}.HANCOCK.pending"),
                Some(sess_root),
                "hancock_pending",
                "Listed Hancock pending tray for the human signer.",
                json!({ "bytes": text.len() }),
            );
            json!({
                "ok": true,
                "action": "hancock_pending",
                "english": "Hancock pending tray (text). Sign with `hancock` TUI or `hancock approve <id>`.",
                "text": text,
            })
        }
        Err(e) => json!({
            "ok": false,
            "action": "hancock_pending",
            "error": e.to_string(),
            "english": format!("Failed to run hancock list: {e}"),
        }),
    }
}

/// Wait on an existing Hancock request id.
pub fn wait(sess_root: &str, id: &str, timeout_secs: u64) -> HancockResult {
    let bin = match which_hancock() {
        Ok(b) => b,
        Err(e) => {
            return HancockResult {
                ok: false,
                outcome: "HANCOCK_MISSING".into(),
                hancock_id: Some(id.into()),
                risk: String::new(),
                action: String::new(),
                english: e.clone(),
                raw: String::new(),
                error: Some(e),
            };
        }
    };
    wait_for(sess_root, &bin, id, "(resume)", "", timeout_secs, "")
}
