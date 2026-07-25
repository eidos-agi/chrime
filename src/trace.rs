//! Hierarchical breadcrumb IDs + append-only trace log.
//!
//! Scheme is documented in `docs/BREADCRUMBS.md`. AI agents must use only that hierarchy.
//! Every request/response gets a unique id; every log line has plain-English `english`.

use serde_json::{json, Value};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static RUN_ID: OnceLock<String> = OnceLock::new();
static SESS_COUNTER: AtomicU64 = AtomicU64::new(0);
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();
static LOG_LOCK: Mutex<()> = Mutex::new(());
static RUN_STARTED: AtomicBool = AtomicBool::new(false);

/// Process-wide run id: `r` + base36-ish from time+pid.
pub fn run_id() -> &'static str {
    RUN_ID.get_or_init(|| {
        let ms = now_ms();
        let pid = std::process::id();
        let mix = ms ^ ((pid as u64) << 20);
        format!("r{}", base36(mix, 10))
    })
}

pub fn run_root() -> String {
    format!("CHRIME.RUN.{}", run_id())
}

/// Allocate a new session id under this run: `s0001`, `s0002`, …
pub fn next_session_id() -> String {
    let n = SESS_COUNTER.fetch_add(1, Ordering::SeqCst) + 1;
    format!("s{n:04}")
}

pub fn session_root(sess: &str) -> String {
    format!("{}.SESS.{}", run_root(), sess)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn base36(mut n: u64, width: usize) -> String {
    const C: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return format!("{:0>width$}", "0", width = width);
    }
    let mut buf = Vec::new();
    while n > 0 {
        buf.push(C[(n % 36) as usize]);
        n /= 36;
    }
    buf.reverse();
    let s = String::from_utf8(buf).unwrap_or_else(|_| "0".into());
    if s.len() >= width {
        s[s.len() - width..].to_string()
    } else {
        format!("{:0>width$}", s, width = width)
    }
}

fn iso_now() -> String {
    // Minimal RFC3339-ish UTC without chrono dep: use unix + Z
    let ms = now_ms();
    let secs = ms / 1000;
    let rem = ms % 1000;
    // good enough for agents; full calendar formatting would need more code
    format!("{secs}.{rem:03}Z")
}

fn log_file() -> PathBuf {
    LOG_PATH
        .get_or_init(|| {
            let dir = std::env::var("CHRIME_TRACE_DIR").unwrap_or_else(|_| "logs".into());
            let p = Path::new(&dir);
            let _ = std::fs::create_dir_all(p);
            p.join("trace.jsonl")
        })
        .clone()
}

/// Emit one breadcrumb. Never panics; never blocks agents on log failure.
pub fn emit(id: &str, parent: Option<&str>, kind: &str, english: &str, mut data: Value) {
    // Hard redaction: never log secret-like keys
    redact(&mut data);
    let line = json!({
        "id": id,
        "parent": parent,
        "kind": kind,
        "ts": iso_now(),
        "ts_ms": now_ms(),
        "english": english,
        "data": data,
    });
    let _guard = LOG_LOCK.lock();
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file())
    {
        let _ = writeln!(f, "{line}");
        let _ = f.flush();
    }
}

fn redact(v: &mut Value) {
    match v {
        Value::Object(map) => {
            let keys: Vec<String> = map.keys().cloned().collect();
            for k in keys {
                let kl = k.to_ascii_lowercase();
                if matches!(
                    kl.as_str(),
                    "password" | "secret" | "token" | "authorization" | "value" | "passwd"
                ) {
                    if map.get(&k).and_then(|x| x.as_str()).is_some() {
                        map.insert(k, Value::String("[redacted]".into()));
                    }
                } else if let Some(child) = map.get_mut(&k) {
                    redact(child);
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                redact(item);
            }
        }
        _ => {}
    }
}

/// Call once at process start (idempotent).
pub fn run_start() {
    if RUN_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let id = run_root();
    emit(
        &id,
        None,
        "run_start",
        &format!(
            "Chrime process started (run {}). Every later id is under this root. See docs/BREADCRUMBS.md.",
            run_id()
        ),
        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "pid": std::process::id(),
            "trace_file": log_file().display().to_string(),
            "hierarchy_doc": "docs/BREADCRUMBS.md",
        }),
    );
}

/// Session handle: owns SESS id and REQ sequence.
pub struct TraceSession {
    #[allow(dead_code)]
    pub sess: String,
    pub root: String,
    req: AtomicU64,
}

impl TraceSession {
    pub fn new() -> Self {
        let sess = next_session_id();
        let root = session_root(&sess);
        emit(
            &root,
            Some(&run_root()),
            "session_start",
            &format!("API session {sess} opened under run {}.", run_id()),
            json!({ "session": sess }),
        );
        TraceSession {
            sess,
            root,
            req: AtomicU64::new(0),
        }
    }

    pub fn next_req(&self) -> (String, String, u64) {
        let n = self.req.fetch_add(1, Ordering::SeqCst) + 1;
        let id = format!("{}.REQ.{n:08}", self.root);
        (id, self.root.clone(), n)
    }
}

/// Log request + response; inject `_trace` into the response JSON.
pub fn wrap_dispatch(trace: &TraceSession, line: &str, response_body: String) -> String {
    let (req_id, parent, n) = trace.next_req();

    let op_guess = serde_json::from_str::<Value>(line)
        .ok()
        .and_then(|v| v.get("op").and_then(|o| o.as_str()).map(|s| s.to_string()))
        .unwrap_or_else(|| "(invalid-json)".into());

    let english_req = format!(
        "Request #{n}: received op `{op_guess}` (raw len {}).",
        line.len()
    );
    emit(
        &req_id,
        Some(&parent),
        "request",
        &english_req,
        json!({
            "op": op_guess,
            "seq": n,
            "line_len": line.len(),
            // never log full line if knox — redact via parse
            "request": redact_request_line(line),
        }),
    );

    let mut resp_val: Value = serde_json::from_str(&response_body).unwrap_or_else(
        |_| json!({ "ok": false, "code": "non_json_response", "raw_len": response_body.len() }),
    );

    let ok = resp_val.get("ok").and_then(|o| o.as_bool()).unwrap_or(true);
    let kind = if ok { "response" } else { "error" };
    let op_for_res = serde_json::from_str::<Value>(line)
        .ok()
        .and_then(|v| v.get("op").and_then(|o| o.as_str()).map(|s| s.to_string()))
        .unwrap_or_else(|| "?".into());
    let english_res = format!("Response #{n}: op `{op_for_res}` finished ok={ok}.");

    // Attach unambiguous correlation block for AIs.
    if let Value::Object(ref mut map) = resp_val {
        map.insert(
            "_trace".into(),
            json!({
                "id": req_id,
                "parent": parent,
                "run": run_root(),
                "session": trace.root,
                "seq": n,
                "hierarchy": "docs/BREADCRUMBS.md",
            }),
        );
    }

    let out = resp_val.to_string();
    emit(
        &req_id,
        Some(&parent),
        kind,
        &english_res,
        json!({
            "ok": ok,
            "seq": n,
            "response": redact_response_summary(&resp_val),
        }),
    );
    out
}

fn redact_request_line(line: &str) -> Value {
    match serde_json::from_str::<Value>(line) {
        Ok(mut v) => {
            redact(&mut v);
            v
        }
        Err(_) => json!({ "raw_len": line.len(), "parse": "bad_json" }),
    }
}

fn redact_response_summary(v: &Value) -> Value {
    let mut c = v.clone();
    redact(&mut c);
    // Drop huge node arrays from trace — only keep counts
    if let Value::Object(ref mut map) = c {
        if let Some(Value::Array(nodes)) = map.get("nodes") {
            let n = nodes.len();
            map.insert("nodes".into(), json!({ "_omitted": true, "len": n }));
        }
        if let Some(Value::String(text)) = map.get("text") {
            if text.len() > 200 {
                map.insert(
                    "text".into(),
                    json!({ "_omitted": true, "len": text.len() }),
                );
            }
        }
        // _trace is correlation; keep it
    }
    c
}
