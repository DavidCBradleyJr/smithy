//! Threads: one ACP agent session per thread, rooted in a project directory.
//!
//! Every event is appended to the thread's transcript *and* emitted to the UI
//! as `thread-event`, so the UI renders live sessions and reopened history
//! through the same reducer.

pub mod acp;
pub mod registry;
pub mod store;

use acp::{Conn, Incoming};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock, Weak,
    },
};
use store::{Project, State, Thread};
use tauri::{AppHandle, Emitter, Manager as _, Runtime};

struct Live {
    conn: Arc<Conn>,
    session_id: String,
}

pub struct Manager {
    dir: PathBuf,
    state: Mutex<State>,
    live: Mutex<HashMap<String, Arc<Live>>>,
    /// Permission requests awaiting the user, by `thread:requestId`.
    perms: Mutex<HashMap<String, (String, Arc<Conn>, Value)>>,
    /// Serialises agent startup so two prompts can't spawn two sessions.
    starting: tokio::sync::Mutex<()>,
}

impl Manager {
    pub fn new() -> Manager {
        Manager::with_dir(store::data_dir())
    }

    pub fn with_dir(dir: PathBuf) -> Manager {
        Manager {
            state: Mutex::new(State::load(&dir)),
            dir,
            live: Mutex::default(),
            perms: Mutex::default(),
            starting: tokio::sync::Mutex::new(()),
        }
    }

    /// Drops every running agent (killing its process group). Called on exit.
    pub fn shutdown(&self) {
        self.perms.lock().unwrap().clear();
        self.live.lock().unwrap().clear();
    }

    fn save(&self) -> Result<(), String> {
        self.state.lock().unwrap().save(&self.dir)
    }

    fn thread(&self, id: &str) -> Result<Thread, String> {
        let st = self.state.lock().unwrap();
        st.threads.iter().find(|t| t.id == id).cloned().ok_or_else(|| format!("no thread {id}"))
    }

    fn record<R: Runtime>(&self, app: &AppHandle<R>, thread: &str, mut event: Value) {
        if event.get("at").is_none() {
            event["at"] = json!(store::now());
        }
        if let Err(e) = store::append(&self.dir, thread, &event) {
            eprintln!("smithy: transcript write failed for {thread}: {e}");
        }
        let _ = app.emit("thread-event", json!({ "thread": thread, "event": event }));
    }

    fn touch(&self, thread: &str, title_from: Option<&str>) {
        let mut st = self.state.lock().unwrap();
        if let Some(t) = st.thread_mut(thread) {
            t.updated = store::now();
            if let Some(p) = title_from.filter(|_| t.title.is_empty()) {
                t.title = store::title_from(p);
            }
        }
        drop(st);
        let _ = self.save();
    }
}

fn handle_incoming<R: Runtime>(app: &AppHandle<R>, thread: &str, conn: &OnceLock<Weak<Conn>>, replaying: &AtomicBool, msg: Incoming) {
    let mgr = app.state::<Manager>();
    match msg {
        Incoming::Notification { method, params } if method == "session/update" => {
            // session/load replays the agent's history; we already have ours.
            if !replaying.load(Ordering::Relaxed) {
                mgr.record(app, thread, json!({ "t": "update", "update": params["update"] }));
            }
        }
        Incoming::Notification { .. } => {}
        Incoming::Request { id, method, params } if method == "session/request_permission" => {
            let Some(conn) = conn.get().and_then(Weak::upgrade) else { return };
            let key = format!("{thread}:{id}");
            mgr.perms.lock().unwrap().insert(key.clone(), (thread.to_string(), conn.clone(), id));
            mgr.record(
                app,
                thread,
                json!({ "t": "permission", "key": key, "toolCall": params["toolCall"], "options": params["options"] }),
            );
            notify_desktop("Smithy: approval needed", &mgr.thread(thread).map(|t| t.title).unwrap_or_default());
        }
        // We advertise no fs/terminal capabilities, so agents shouldn't ask.
        Incoming::Request { id, method, .. } => {
            if let Some(c) = conn.get().and_then(Weak::upgrade) {
                let _ = c.respond(id, Err((-32601, format!("{method} not supported by Smithy"))));
            }
        }
        Incoming::Closed => {
            if mgr.live.lock().unwrap().remove(thread).is_some() {
                mgr.record(app, thread, json!({ "t": "exit" }));
            }
        }
    }
}

fn notify_desktop(summary: &str, body: &str) {
    let _ = std::process::Command::new("notify-send").args(["-a", "Smithy", summary, body]).spawn();
}

/// Returns the thread's running session, starting (or resuming) the agent.
async fn ensure_live<R: Runtime>(app: &AppHandle<R>, thread_id: &str) -> Result<Arc<Live>, String> {
    let mgr = app.state::<Manager>();
    if let Some(l) = mgr.live.lock().unwrap().get(thread_id) {
        return Ok(l.clone());
    }
    let _guard = mgr.starting.lock().await;
    if let Some(l) = mgr.live.lock().unwrap().get(thread_id) {
        return Ok(l.clone());
    }

    let t = mgr.thread(thread_id)?;
    let def = registry::get(&t.agent).ok_or_else(|| format!("unknown agent {}", t.agent))?;
    if !def.available {
        return Err(format!("{} needs `{}` on your PATH", def.name, def.requires));
    }
    // Pi reads local models from its models.json; make sure it's current.
    if t.agent == "pi" {
        if let Err(e) = crate::local::sync_pi() {
            mgr.record(app, thread_id, json!({ "t": "error", "message": e }));
        }
    }
    let log = crate::runtime::process::log_path(&format!("agent-{thread_id}"))
        .and_then(|p| std::fs::create_dir_all(p.parent()?).ok().map(|_| p))
        .and_then(|p| std::fs::File::create(p).ok());

    // Weak: the reader task must not keep a deleted thread's agent alive.
    let cell: Arc<OnceLock<Weak<Conn>>> = Arc::default();
    let replaying = Arc::new(AtomicBool::new(false));
    let conn = {
        let (app, tid, cell, replaying) = (app.clone(), thread_id.to_string(), cell.clone(), replaying.clone());
        Conn::spawn(&registry::argv(&def), Path::new(t.workdir()), log, move |m| {
            handle_incoming(&app, &tid, &cell, &replaying, m)
        })?
    };
    let _ = cell.set(Arc::downgrade(&conn));

    let init = conn
        .request(
            "initialize",
            json!({
                "protocolVersion": acp::PROTOCOL_VERSION,
                "clientCapabilities": { "fs": { "readTextFile": false, "writeTextFile": false }, "terminal": false },
                "clientInfo": { "name": "smithy", "version": env!("CARGO_PKG_VERSION") },
            }),
        )
        .await?;
    let can_load = init["agentCapabilities"]["loadSession"].as_bool().unwrap_or(false);
    let base = json!({ "cwd": t.workdir(), "mcpServers": [] });

    let (session_id, mut config, resumed) = match t.session_id.clone().filter(|_| can_load) {
        Some(sid) => {
            replaying.store(true, Ordering::Relaxed);
            let mut params = base.clone();
            params["sessionId"] = json!(sid);
            let r = conn.request("session/load", params).await;
            replaying.store(false, Ordering::Relaxed);
            match r {
                Ok(r) => (sid, r["configOptions"].clone(), true),
                Err(_) => new_session(&conn, &base).await?,
            }
        }
        None => new_session(&conn, &base).await?,
    };

    // Re-apply what the user last picked for this agent, where still offered.
    let prefs = mgr.state.lock().unwrap().prefs.get(&t.agent).cloned().unwrap_or_default();
    for (config_id, value) in prefs {
        if wants_change(&config, &config_id, &value) {
            if let Ok(r) = conn
                .request(
                    "session/set_config_option",
                    json!({ "sessionId": session_id, "configId": config_id, "value": value }),
                )
                .await
            {
                config = r["configOptions"].clone();
            }
        }
    }

    // Local models: a thread connects on whatever is already in VRAM, so
    // opening a thread never makes the router swap models behind your back.
    // (A resumed session whose model was renamed also falls back to Pi's first
    // model, which is rarely the loaded one.) Switching is an explicit choice
    // in the model picker.
    if t.agent == "pi" {
        if let Some(loaded) = crate::local::state().await.loaded {
            let value = format!("{}/{}", crate::local::PI_PROVIDER, loaded.id);
            if wants_change(&config, "model", &value) {
                if let Ok(r) = conn
                    .request("session/set_config_option", json!({ "sessionId": session_id, "configId": "model", "value": value }))
                    .await
                {
                    config = r["configOptions"].clone();
                }
            }
        }
    }

    {
        let mut st = mgr.state.lock().unwrap();
        if let Some(th) = st.thread_mut(thread_id) {
            th.session_id = Some(session_id.clone());
        }
    }
    mgr.save()?;
    mgr.record(
        app,
        thread_id,
        json!({ "t": "session", "resumed": resumed, "agentInfo": init["agentInfo"], "configOptions": config }),
    );

    let live = Arc::new(Live { conn, session_id });
    mgr.live.lock().unwrap().insert(thread_id.to_string(), live.clone());
    Ok(live)
}

async fn new_session(conn: &Conn, base: &Value) -> Result<(String, Value, bool), String> {
    let r = conn.request("session/new", base.clone()).await?;
    let sid = r["sessionId"].as_str().ok_or("agent returned no sessionId")?.to_string();
    Ok((sid, r["configOptions"].clone(), false))
}

/// True when `config_id` exists, offers `value`, and isn't already set to it.
pub fn wants_change(config: &Value, config_id: &str, value: &str) -> bool {
    config.as_array().into_iter().flatten().any(|opt| {
        opt["id"] == config_id
            && opt["currentValue"] != value
            && opt["options"].as_array().into_iter().flatten().any(|o| o["value"] == value)
    })
}

// ---- commands ----

#[tauri::command]
pub fn agents_list() -> Vec<registry::AgentDef> {
    registry::all()
}

#[tauri::command]
pub fn projects_list(mgr: tauri::State<Manager>) -> Vec<Project> {
    mgr.state.lock().unwrap().projects.clone()
}

#[tauri::command]
pub fn project_add(mgr: tauri::State<Manager>, path: String) -> Result<Project, String> {
    let p = Path::new(&path);
    if !p.is_dir() {
        return Err(format!("{path} is not a directory"));
    }
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.clone());
    let project = Project { path: path.clone(), name };
    {
        let mut st = mgr.state.lock().unwrap();
        if !st.projects.iter().any(|x| x.path == path) {
            st.projects.push(project.clone());
        }
    }
    mgr.save()?;
    Ok(project)
}

#[tauri::command]
pub fn project_remove(mgr: tauri::State<Manager>, path: String) -> Result<(), String> {
    mgr.state.lock().unwrap().projects.retain(|p| p.path != path);
    mgr.save()
}

/// Current git branch, if the project is a repo.
#[tauri::command]
pub async fn project_branch(path: String) -> Option<String> {
    let out = tokio::process::Command::new("git")
        .args(["-C", &path, "rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .await
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[tauri::command]
pub fn threads_list(mgr: tauri::State<Manager>) -> Vec<Thread> {
    let mut ts = mgr.state.lock().unwrap().threads.clone();
    ts.sort_by(|a, b| b.updated.cmp(&a.updated));
    ts
}

#[tauri::command]
pub fn thread_live(mgr: tauri::State<Manager>) -> Vec<String> {
    mgr.live.lock().unwrap().keys().cloned().collect()
}

#[tauri::command]
pub async fn thread_create<R: Runtime>(
    app: AppHandle<R>,
    project: String,
    agent: String,
    worktree: bool,
) -> Result<Thread, String> {
    let mgr = app.state::<Manager>();
    let now = store::now();
    let id = store::new_id();
    let (branch, cwd) = if worktree {
        let (b, p) = crate::git::worktree_add(&project, &id).await?;
        (Some(b), Some(p))
    } else {
        (None, None)
    };
    let t = Thread { id, project, agent, session_id: None, title: String::new(), cwd, branch, created: now, updated: now };
    mgr.state.lock().unwrap().threads.push(t.clone());
    mgr.save()?;
    // Start the agent now so the composer can show its models straight away.
    // A failure here is recorded in the thread, not fatal: the user can retry.
    if let Err(e) = ensure_live(&app, &t.id).await {
        mgr.record(&app, &t.id, json!({ "t": "error", "message": e }));
    }
    Ok(t)
}

/// Reconnects a thread's agent in the background when it's opened, so the
/// composer shows the agent's current options rather than stale ones.
#[tauri::command]
pub async fn thread_resume<R: Runtime>(app: AppHandle<R>, id: String) -> Result<(), String> {
    ensure_live(&app, &id).await.map(|_| ())
}

#[tauri::command]
pub fn thread_history(mgr: tauri::State<Manager>, id: String) -> Vec<Value> {
    store::history(&mgr.dir, &id)
}

/// Deletes a thread. A worktree with uncommitted changes is kept (and the
/// thread with it) unless `force`; its branch is only deleted once merged.
#[tauri::command]
pub async fn thread_delete<R: Runtime>(app: AppHandle<R>, id: String, force: bool) -> Result<(), String> {
    let mgr = app.state::<Manager>();
    let t = mgr.thread(&id)?;
    mgr.live.lock().unwrap().remove(&id);
    if let (Some(cwd), Some(branch)) = (&t.cwd, &t.branch) {
        crate::git::worktree_remove(&t.project, cwd, branch, force).await?;
    }
    app.state::<crate::pty::Ptys>().close(&id);
    mgr.state.lock().unwrap().threads.retain(|t| t.id != id);
    let _ = std::fs::remove_file(store::transcript_file(&mgr.dir, &id));
    mgr.save()
}

/// Pi retries a failing model request and then ends the turn quietly, with
/// only "Retrying…" messages to show for it.
pub fn is_retry_notice(text: &str) -> bool {
    let t = text.trim();
    (t.starts_with("Retrying (attempt ") && t.ends_with("...")) || t == "Retry finished, resuming."
}

/// The last error llama-server logged, from lines like
/// `... got exception: {"error":{"code":500,"message":"...Error: Jinja Exception: ..."}}`.
pub fn router_error(log: &str) -> Option<String> {
    let line = log.lines().rev().find(|l| l.contains("got exception: "))?;
    let json = &line[line.find("got exception: ")? + "got exception: ".len()..];
    let msg = serde_json::from_str::<Value>(json).ok()?["error"]["message"].as_str()?.to_string();
    let msg = msg.rsplit("Error: ").next().unwrap_or(&msg).trim().to_string();
    Some(msg)
}

fn router_log_len() -> u64 {
    crate::runtime::process::log_path("local-router").and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len()).unwrap_or(0)
}

fn router_log_since(offset: u64) -> String {
    use std::io::{Read, Seek};
    let Some(path) = crate::runtime::process::log_path("local-router") else { return String::new() };
    let Ok(mut f) = std::fs::File::open(path) else { return String::new() };
    let mut s = String::new();
    if f.seek(std::io::SeekFrom::Start(offset)).is_ok() {
        let _ = f.take(1 << 20).read_to_string(&mut s);
    }
    s
}

/// After a turn that produced nothing but retry notices, explain why.
fn explain_empty_turn<R: Runtime>(app: &AppHandle<R>, mgr: &Manager, id: &str, log_offset: u64) {
    let hist = store::history(&mgr.dir, id);
    let Some(start) = hist.iter().rposition(|e| e["t"] == "user") else { return };
    let turn = &hist[start + 1..];
    let produced = turn.iter().any(|e| {
        let u = &e["update"];
        match u["sessionUpdate"].as_str() {
            Some("agent_message_chunk") => u["content"]["text"].as_str().is_some_and(|t| !t.trim().is_empty() && !is_retry_notice(t)),
            Some("tool_call") => true,
            _ => false,
        }
    });
    let retried = turn.iter().any(|e| e["update"]["content"]["text"].as_str().is_some_and(is_retry_notice));
    if produced || !retried {
        return;
    }
    let message = match router_error(&router_log_since(log_offset)) {
        Some(e) => format!("The local model rejected the request: {e}"),
        None => "The model request failed, and the agent gave up after retrying. Check the local server log.".into(),
    };
    mgr.record(app, id, json!({ "t": "error", "message": message }));
}

#[tauri::command]
pub async fn thread_prompt<R: Runtime>(app: AppHandle<R>, id: String, text: String) -> Result<String, String> {
    let mgr = app.state::<Manager>();
    let log_offset = router_log_len();
    mgr.record(&app, &id, json!({ "t": "user", "text": text }));
    mgr.touch(&id, Some(&text));
    let result = async {
        let live = ensure_live(&app, &id).await?;
        live.conn
            .request(
                "session/prompt",
                json!({ "sessionId": live.session_id, "prompt": [{ "type": "text", "text": text }] }),
            )
            .await
    }
    .await;
    mgr.touch(&id, None);
    match result {
        Ok(r) => {
            let reason = r["stopReason"].as_str().unwrap_or("end_turn").to_string();
            explain_empty_turn(&app, &mgr, &id, log_offset);
            mgr.record(&app, &id, json!({ "t": "stop", "reason": reason }));
            if !app.get_webview_window("main").and_then(|w| w.is_focused().ok()).unwrap_or(false) {
                notify_desktop("Smithy: agent finished", &mgr.thread(&id).map(|t| t.title).unwrap_or_default());
            }
            Ok(reason)
        }
        Err(e) => {
            mgr.record(&app, &id, json!({ "t": "error", "message": e }));
            Err(e)
        }
    }
}

#[tauri::command]
pub fn thread_cancel<R: Runtime>(app: AppHandle<R>, id: String) -> Result<(), String> {
    let mgr = app.state::<Manager>();
    // Pending approvals are answered "cancelled" as the protocol requires.
    let waiting: Vec<String> = mgr.perms.lock().unwrap().iter().filter(|(_, v)| v.0 == id).map(|(k, _)| k.clone()).collect();
    for key in waiting {
        let _ = permission_respond(app.clone(), key, None);
    }
    let live = mgr.live.lock().unwrap().get(&id).cloned();
    match live {
        Some(l) => l.conn.notify("session/cancel", json!({ "sessionId": l.session_id })),
        None => Ok(()),
    }
}

#[tauri::command]
pub async fn thread_set_config<R: Runtime>(app: AppHandle<R>, id: String, config_id: String, value: String) -> Result<Value, String> {
    let mgr = app.state::<Manager>();
    let live = ensure_live(&app, &id).await?;
    let r = live
        .conn
        .request(
            "session/set_config_option",
            json!({ "sessionId": live.session_id, "configId": config_id, "value": value }),
        )
        .await?;
    let agent = mgr.thread(&id)?.agent;
    mgr.state.lock().unwrap().prefs.entry(agent).or_default().insert(config_id, value);
    mgr.save()?;
    let config = r["configOptions"].clone();
    if config.is_array() {
        mgr.record(&app, &id, json!({ "t": "update", "update": { "sessionUpdate": "config_option_update", "configOptions": config } }));
    }
    Ok(config)
}

/// `option_id: None` answers "cancelled".
#[tauri::command]
pub fn permission_respond<R: Runtime>(app: AppHandle<R>, key: String, option_id: Option<String>) -> Result<(), String> {
    let mgr = app.state::<Manager>();
    let (thread, conn, id) = mgr.perms.lock().unwrap().remove(&key).ok_or("that request is no longer pending")?;
    let outcome = match &option_id {
        Some(o) => json!({ "outcome": "selected", "optionId": o }),
        None => json!({ "outcome": "cancelled" }),
    };
    conn.respond(id, Ok(json!({ "outcome": outcome })))?;
    mgr.record(&app, &thread, json!({ "t": "permission_resolved", "key": key, "optionId": option_id }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_pi_retries_and_router_errors() {
        assert!(is_retry_notice("Retrying (attempt 2/3, waiting 4s)..."));
        assert!(is_retry_notice("Retry finished, resuming."));
        assert!(!is_retry_notice("Retrying is a word I might use in an answer."));
        let log = "265.10 I srv proxy\n[50179] 265.17 W srv    operator(): got exception: {\"error\":{\"code\":500,\"message\":\"\\n---\\nError: Jinja Exception: Unexpected reasoning effort high. Supported types are xhigh (default), medium, and low.\",\"type\":\"server_error\"}}\n";
        assert_eq!(
            router_error(log).as_deref(),
            Some("Jinja Exception: Unexpected reasoning effort high. Supported types are xhigh (default), medium, and low.")
        );
        assert_eq!(router_error("all fine\n"), None);
    }

    #[test]
    fn prefs_apply_only_when_offered_and_different() {
        let config = json!([{
            "id": "model", "currentValue": "a/x",
            "options": [{ "value": "a/x" }, { "value": "bonsai/b" }]
        }]);
        assert!(wants_change(&config, "model", "bonsai/b"));
        assert!(!wants_change(&config, "model", "a/x"));
        assert!(!wants_change(&config, "model", "gone/model"));
        assert!(!wants_change(&config, "thought_level", "high"));
        assert!(!wants_change(&Value::Null, "model", "a/x"));
    }

    /// Drives Pi on the local model end to end through the real command
    /// functions: `cargo test live_thread -- --ignored --nocapture`.
    /// Needs the local router (Smithy starts it) with a model it can load.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn live_thread() {
        let dir = std::env::temp_dir().join(format!("smithy-live-{}", store::new_id()));
        let proj = dir.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join("calc.py"), "def add(a, b):\n    return a - b\n").unwrap();

        let app = tauri::test::mock_app();
        app.manage(Manager::with_dir(dir.join("data")));
        let h = app.handle().clone();
        project_add(h.state(), proj.to_string_lossy().into()).unwrap();
        let t = thread_create(h.clone(), proj.to_string_lossy().into(), "pi".into(), false).await.unwrap();
        let reason = thread_prompt(
            h.clone(),
            t.id.clone(),
            "calc.py has a bug: add subtracts. Fix it. Don't run anything, just edit the file.".into(),
        )
        .await
        .unwrap();

        let hist = thread_history(h.state(), t.id.clone());
        let kinds: Vec<String> = hist
            .iter()
            .map(|e| match e["t"].as_str().unwrap() {
                "update" => e["update"]["sessionUpdate"].as_str().unwrap_or("?").to_string(),
                k => k.to_string(),
            })
            .collect();
        let mut compact = kinds.clone();
        compact.dedup();
        println!("stop={reason}\nevents={compact:?}");
        let fixed = std::fs::read_to_string(proj.join("calc.py")).unwrap();
        println!("calc.py:\n{fixed}");
        let title = threads_list(h.state())[0].title.clone();
        println!("title={title}");

        assert_eq!(reason, "end_turn");
        assert!(kinds.iter().any(|k| k == "session"));
        assert!(kinds.iter().any(|k| k == "tool_call"));
        assert!(fixed.contains("a + b"));
        assert!(!title.is_empty());
        app.state::<Manager>().shutdown();
        drop(app);

        // A second app instance (as after a restart) resumes the same session.
        let app2 = tauri::test::mock_app();
        app2.manage(Manager::with_dir(dir.join("data")));
        let h2 = app2.handle().clone();
        let r2 = thread_prompt(h2.clone(), t.id.clone(), "In one word: which file did you just edit?".into())
            .await
            .unwrap();
        let hist2 = thread_history(h2.state(), t.id.clone());
        let session2 = hist2.iter().filter(|e| e["t"] == "session").last().unwrap();
        let answer: String = hist2
            .iter()
            .skip(hist.len())
            .filter(|e| e["update"]["sessionUpdate"] == "agent_message_chunk")
            .filter_map(|e| e["update"]["content"]["text"].as_str())
            .collect();
        println!("resumed={} stop={r2} answer={answer:?}", session2["resumed"]);
        assert_eq!(session2["resumed"], true);
        assert!(answer.to_lowercase().contains("calc"));
        app2.state::<Manager>().shutdown();
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Switches a Pi thread between two local models through the real
    /// commands: `cargo test live_switch -- --ignored --nocapture`.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn live_switch() {
        let dir = std::env::temp_dir().join(format!("smithy-switch-{}", store::new_id()));
        let proj = dir.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        let app = tauri::test::mock_app();
        app.manage(Manager::with_dir(dir.join("data")));
        let h = app.handle().clone();

        let before = crate::local::state().await.loaded.map(|l| l.id);
        let t = thread_create(h.clone(), proj.to_string_lossy().into(), "pi".into(), false).await.unwrap();
        let model_of = |hist: &[Value]| -> String {
            hist.iter()
                .rev()
                .find_map(|e| {
                    let opts = if e["t"] == "session" { &e["configOptions"] } else { &e["update"]["configOptions"] };
                    opts.as_array()?.iter().find(|o| o["id"] == "model")?["currentValue"].as_str().map(str::to_string)
                })
                .unwrap_or_default()
        };
        let start_model = model_of(&thread_history(h.state(), t.id.clone()));
        println!("loaded before={before:?} thread starts on={start_model}");
        if let Some(b) = &before {
            assert_eq!(start_model, format!("local/{b}"));
        }

        for id in ["Qwen3.5-4B", "Ternary-Bonsai-2-27B"] {
            let started = std::time::Instant::now();
            crate::local::local_load(h.clone(), id.into()).await.unwrap();
            let load_s = started.elapsed().as_secs_f32();
            thread_set_config(h.clone(), t.id.clone(), "model".into(), format!("local/{id}")).await.unwrap();
            let n = thread_history(h.state(), t.id.clone()).len();
            thread_prompt(h.clone(), t.id.clone(), "Reply with only the word: ready".into()).await.unwrap();
            let hist = thread_history(h.state(), t.id.clone());
            let reply: String = hist[n..]
                .iter()
                .filter(|e| e["update"]["sessionUpdate"] == "agent_message_chunk")
                .filter_map(|e| e["update"]["content"]["text"].as_str())
                .collect();
            let st = crate::local::state().await;
            let loaded: Vec<_> = st.models.iter().filter(|m| m.status == "loaded").map(|m| m.id.clone()).collect();
            println!("{id}: load {load_s:.1}s, in vram={loaded:?}, thread model={}, reply={reply:?}", model_of(&hist));
            assert_eq!(loaded, vec![id.to_string()]);
            assert_eq!(model_of(&hist), format!("local/{id}"));
            assert!(reply.to_lowercase().contains("ready"));
        }
        app.state::<Manager>().shutdown();
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The whole review loop through the real commands, with Pi on the loaded
    /// local model: worktree thread → edit → review comment → revise → commit →
    /// merge → delete. `cargo test live_review -- --ignored --nocapture`.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn live_review() {
        use crate::git;
        let dir = std::env::temp_dir().join(format!("smithy-review-{}", store::new_id()));
        let proj = dir.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        let p = proj.to_string_lossy().into_owned();
        let sh = |args: &[&str]| {
            assert!(std::process::Command::new("git").arg("-C").arg(&p).args(args).status().unwrap().success());
        };
        sh(&["init", "-q", "-b", "main"]);
        sh(&["config", "user.email", "t@t"]);
        sh(&["config", "user.name", "t"]);
        std::fs::write(proj.join("greet.py"), "def greet(name):\n    return 'hi ' + name\n").unwrap();
        sh(&["add", "-A"]);
        sh(&["commit", "-qm", "init"]);

        let app = tauri::test::mock_app();
        app.manage(Manager::with_dir(dir.join("data")));
        app.manage(crate::pty::Ptys::default());
        let h = app.handle().clone();
        let t = thread_create(h.clone(), p.clone(), "pi".into(), true).await.unwrap();
        let wt = t.cwd.clone().expect("worktree thread has its own folder");
        let branch = t.branch.clone().unwrap();
        println!("worktree {wt} on {branch}");
        assert_ne!(wt, p);

        let started = std::time::Instant::now();
        thread_prompt(
            h.clone(),
            t.id.clone(),
            "In greet.py, make greet return 'hello ' + name instead of 'hi '. Edit the file; don't run anything.".into(),
        )
        .await
        .unwrap();
        let c = git::git_changes(wt.clone()).await;
        println!("after edit ({:.0}s): {:?}", started.elapsed().as_secs_f32(), c.files.iter().map(|f| (&f.path, &f.status, f.additions, f.deletions)).collect::<Vec<_>>());
        assert_eq!(c.files.len(), 1);
        assert!(std::fs::read_to_string(proj.join("greet.py")).unwrap().contains("'hi '"), "main checkout untouched");
        let d = git::git_file_diff(wt.clone(), "greet.py".into(), None).await.unwrap();
        assert!(d.new.as_deref().unwrap().contains("hello"));

        // What the review panel sends for a comment on line 2.
        let review = "I reviewed your changes. Please address these comments:\n\ngreet.py\n- line 2: `return 'hello ' + name`\n  Use an f-string here instead of concatenation.";
        thread_prompt(h.clone(), t.id.clone(), review.into()).await.unwrap();
        let now = std::fs::read_to_string(std::path::Path::new(&wt).join("greet.py")).unwrap();
        println!("after review ({:.0}s):\n{now}", started.elapsed().as_secs_f32());
        assert!(now.contains("f\"hello {name}\"") || now.contains("f'hello {name}'"), "{now}");

        let sha = git::git_commit(wt.clone(), "greet with hello".into()).await.unwrap();
        let into = git::git_merge(p.clone(), branch.clone()).await.unwrap();
        println!("committed {sha}, merged into {into}");
        assert!(std::fs::read_to_string(proj.join("greet.py")).unwrap().contains("hello {name}"));

        thread_delete(h.clone(), t.id.clone(), false).await.unwrap();
        assert!(!std::path::Path::new(&wt).exists(), "worktree removed");
        let branches = String::from_utf8(std::process::Command::new("git").arg("-C").arg(&p).args(["branch", "--list", &branch]).output().unwrap().stdout).unwrap();
        assert!(branches.trim().is_empty(), "merged branch deleted");
        app.state::<Manager>().shutdown();
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Starts a session with every installed agent without prompting, so no
    /// subscription usage: `cargo test live_agents -- --ignored --nocapture`.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn live_agents() {
        let dir = std::env::temp_dir().join(format!("smithy-agents-{}", store::new_id()));
        let proj = dir.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        let app = tauri::test::mock_app();
        app.manage(Manager::with_dir(dir.join("data")));
        app.manage(crate::pty::Ptys::default());
        let h = app.handle().clone();
        for a in registry::all().into_iter().filter(|a| a.available) {
            let started = std::time::Instant::now();
            let t = thread_create(h.clone(), proj.to_string_lossy().into(), a.id.into(), false).await.unwrap();
            let hist = thread_history(h.state(), t.id.clone());
            let session = hist.iter().find(|e| e["t"] == "session");
            let err = hist.iter().find(|e| e["t"] == "error").map(|e| e["message"].to_string());
            let config: Vec<String> = session
                .and_then(|s| s["configOptions"].as_array().cloned())
                .unwrap_or_default()
                .iter()
                .map(|o| format!("{}={} ({} options)", o["id"].as_str().unwrap_or("?"), o["currentValue"].as_str().unwrap_or("?"), o["options"].as_array().map_or(0, |v| v.len())))
                .collect();
            println!(
                "{:<9} {:>5.1}s agent={} config={config:?} error={err:?}",
                a.id,
                started.elapsed().as_secs_f32(),
                session.map(|s| s["agentInfo"]["name"].to_string()).unwrap_or_default(),
            );
            thread_delete(h.clone(), t.id.clone(), false).await.unwrap();
        }
        app.state::<Manager>().shutdown();
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The case that used to fail: Bonsai with thinking "high", which its
    /// template rejects. `cargo test live_thinking -- --ignored --nocapture`.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn live_thinking() {
        let dir = std::env::temp_dir().join(format!("smithy-think-{}", store::new_id()));
        let proj = dir.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        let app = tauri::test::mock_app();
        app.manage(Manager::with_dir(dir.join("data")));
        app.manage(crate::pty::Ptys::default());
        let h = app.handle().clone();
        let t = thread_create(h.clone(), proj.to_string_lossy().into(), "pi".into(), false).await.unwrap();
        let cfg = thread_set_config(h.clone(), t.id.clone(), "thought_level".into(), "high".into()).await.unwrap();
        let level = cfg.as_array().unwrap().iter().find(|o| o["id"] == "thought_level").unwrap()["currentValue"].clone();
        let n = thread_history(h.state(), t.id.clone()).len();
        thread_prompt(h.clone(), t.id.clone(), "who are you? one sentence.".into()).await.unwrap();
        let hist = thread_history(h.state(), t.id.clone());
        let reply: String = hist[n..].iter().filter(|e| e["update"]["sessionUpdate"] == "agent_message_chunk").filter_map(|e| e["update"]["content"]["text"].as_str()).collect();
        let errors: Vec<_> = hist[n..].iter().filter(|e| e["t"] == "error").map(|e| e["message"].to_string()).collect();
        println!("asked high → pi uses {level}; reply={:?}; errors={errors:?}", reply.lines().last().unwrap_or(""));
        assert!(errors.is_empty());
        assert!(!reply.lines().last().unwrap_or("").starts_with("Retry"));
        // Don't leave "high" as this machine's saved Pi preference.
        app.state::<Manager>().state.lock().unwrap().prefs.clear();
        app.state::<Manager>().shutdown();
        let _ = std::fs::remove_dir_all(dir);
    }
}

