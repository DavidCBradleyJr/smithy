//! Minimal ACP (Agent Client Protocol) connection: newline-delimited JSON-RPC
//! 2.0 over an agent subprocess's stdio.
//!
//! Hand-rolled instead of a crate because the client side we need is small
//! (requests, notifications, and answering the agent's own requests), and the
//! protocol's unstable surface moves faster than the Rust bindings.

use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::Path,
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::{mpsc, oneshot},
};

pub const PROTOCOL_VERSION: u64 = 1;

/// What the agent sends us that isn't a response to one of our requests.
#[derive(Debug)]
pub enum Incoming {
    Notification { method: String, params: Value },
    Request { id: Value, method: String, params: Value },
    Closed,
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

pub struct Conn {
    tx: mpsc::UnboundedSender<String>,
    pending: Pending,
    next_id: AtomicU64,
    pgid: Option<u32>,
    _child: Mutex<Child>,
}

impl Drop for Conn {
    fn drop(&mut self) {
        if let Some(pgid) = self.pgid {
            let _ = std::process::Command::new("kill").args(["-TERM", "--", &format!("-{pgid}")]).status();
        }
    }
}

/// Classifies one line from the agent. Responses go to `pending`.
pub fn route(line: &str, pending: &Pending) -> Option<Incoming> {
    let msg: Value = serde_json::from_str(line).ok()?;
    let method = msg["method"].as_str().map(str::to_string);
    match (method, msg.get("id").cloned()) {
        (Some(method), Some(id)) => Some(Incoming::Request { id, method, params: msg["params"].clone() }),
        (Some(method), None) => Some(Incoming::Notification { method, params: msg["params"].clone() }),
        (None, Some(id)) => {
            let waiter = pending.lock().unwrap().remove(&id.as_u64()?)?;
            let result = match msg.get("error") {
                Some(e) => Err(e["message"].as_str().unwrap_or("agent error").to_string()),
                None => Ok(msg["result"].clone()),
            };
            let _ = waiter.send(result);
            None
        }
        (None, None) => None,
    }
}

impl Conn {
    pub fn spawn(
        argv: &[String],
        cwd: &Path,
        log: Option<std::fs::File>,
        on_incoming: impl Fn(Incoming) + Send + 'static,
    ) -> Result<Arc<Conn>, String> {
        let (bin, args) = argv.split_first().ok_or("empty agent command")?;
        let stderr = log.map(Stdio::from).unwrap_or_else(Stdio::null);
        let mut child = Command::new(bin)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr)
            .kill_on_drop(true)
            // Own process group: adapters run via npx spawn grandchildren
            // (node → pi) that must die with the thread.
            .process_group(0)
            .spawn()
            .map_err(|e| format!("{bin}: {e}"))?;
        let pgid = child.id();
        let mut stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let pending: Pending = Arc::default();

        tauri::async_runtime::spawn(async move {
            while let Some(line) = rx.recv().await {
                if stdin.write_all(line.as_bytes()).await.is_err() || stdin.write_all(b"\n").await.is_err() {
                    break;
                }
                let _ = stdin.flush().await;
            }
        });

        let reader_pending = pending.clone();
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(msg) = route(&line, &reader_pending) {
                    on_incoming(msg);
                }
            }
            // Fail anything still waiting rather than hanging the UI forever.
            for (_, w) in reader_pending.lock().unwrap().drain() {
                let _ = w.send(Err("agent exited".into()));
            }
            on_incoming(Incoming::Closed);
        });

        Ok(Arc::new(Conn { tx, pending, next_id: AtomicU64::new(1), pgid, _child: Mutex::new(child) }))
    }

    fn send(&self, msg: Value) -> Result<(), String> {
        self.tx.send(msg.to_string()).map_err(|_| "agent is not running".to_string())
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (w, r) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, w);
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))?;
        r.await.map_err(|_| "agent exited".to_string())?
    }

    pub fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }))
    }

    pub fn respond(&self, id: Value, result: Result<Value, (i64, String)>) -> Result<(), String> {
        let msg = match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err((code, message)) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
        };
        self.send(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_requests_notifications_and_responses() {
        let pending: Pending = Arc::default();
        let (w, mut r) = oneshot::channel();
        pending.lock().unwrap().insert(7, w);

        assert!(route(r#"{"jsonrpc":"2.0","id":7,"result":{"ok":true}}"#, &pending).is_none());
        assert_eq!(r.try_recv().unwrap().unwrap()["ok"], true);

        match route(r#"{"jsonrpc":"2.0","method":"session/update","params":{"x":1}}"#, &pending) {
            Some(Incoming::Notification { method, params }) => {
                assert_eq!(method, "session/update");
                assert_eq!(params["x"], 1);
            }
            other => panic!("{other:?}"),
        }
        match route(r#"{"jsonrpc":"2.0","id":"p1","method":"session/request_permission","params":{}}"#, &pending) {
            Some(Incoming::Request { id, method, .. }) => {
                assert_eq!(id, "p1");
                assert_eq!(method, "session/request_permission");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn error_responses_carry_message() {
        let pending: Pending = Arc::default();
        let (w, mut r) = oneshot::channel();
        pending.lock().unwrap().insert(1, w);
        route(r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"bad model"}}"#, &pending);
        assert_eq!(r.try_recv().unwrap().unwrap_err(), "bad model");
    }

    #[test]
    fn ignores_non_json() {
        assert!(route("Debugger attached.", &Pending::default()).is_none());
    }
}
