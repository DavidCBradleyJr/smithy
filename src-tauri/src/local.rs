//! Local models: everything in the model folders, served by one llama-server
//! in router mode. Switching models is unload-then-load on that router, so
//! only one model holds VRAM at a time and every agent keeps the same endpoint.
//!
//! The router reads a single `--models-dir`, so Smithy points it at a folder
//! of symlinks (`~/.local/share/smithy/models-view/`) that it keeps in step
//! with every configured folder. A folder on an unmounted drive simply
//! contributes no links.

use crate::settings::{self, Settings};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Runtime};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LocalModel {
    /// The router's name for it: the file stem, or the folder name.
    pub id: String,
    pub path: String,
    pub size: u64,
    pub vision: bool,
    /// "loaded" | "loading" | "unloaded" (and whatever else the router reports).
    pub status: String,
    /// Context size its preset gives it.
    #[serde(default)]
    pub ctx: Option<u64>,
    /// The configured folder it was found in.
    #[serde(default)]
    pub folder: String,
}

fn is_gguf(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("gguf"))
}

fn is_mmproj(p: &Path) -> bool {
    p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.to_ascii_lowercase().starts_with("mmproj"))
}

fn size_of(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

/// Mirrors the router's `--models-dir` layout rules: top-level `.gguf` files,
/// or folders of weights (possibly sharded) plus an optional `mmproj*` file.
pub fn scan(dir: &Path) -> Vec<LocalModel> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<LocalModel> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.is_file() && is_gguf(&path) && !is_mmproj(&path) {
                return Some(LocalModel {
                    id: path.file_stem()?.to_string_lossy().into_owned(),
                    size: size_of(&path),
                    path: path.to_string_lossy().into_owned(),
                    vision: false,
                    status: "unloaded".into(),
                    ctx: None,
                    folder: String::new(),
                });
            }
            if !path.is_dir() {
                return None;
            }
            let files: Vec<PathBuf> = std::fs::read_dir(&path).ok()?.flatten().map(|f| f.path()).filter(|f| is_gguf(f)).collect();
            let weights: Vec<&PathBuf> = files.iter().filter(|f| !is_mmproj(f)).collect();
            if weights.is_empty() {
                return None;
            }
            Some(LocalModel {
                id: path.file_name()?.to_string_lossy().into_owned(),
                size: weights.iter().map(|w| size_of(w)).sum(),
                path: path.to_string_lossy().into_owned(),
                vision: files.iter().any(|f| is_mmproj(f)),
                status: "unloaded".into(),
                ctx: None,
                folder: String::new(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.id.to_lowercase().cmp(&b.id.to_lowercase()));
    out
}

/// Every model across `dirs`, first folder winning on a name clash.
pub fn scan_all(dirs: &[String]) -> Vec<LocalModel> {
    let mut out: Vec<LocalModel> = Vec::new();
    for dir in dirs {
        for mut m in scan(Path::new(dir)) {
            if !out.iter().any(|o| o.id == m.id) {
                m.folder = dir.clone();
                out.push(m);
            }
        }
    }
    out.sort_by(|a, b| a.id.to_lowercase().cmp(&b.id.to_lowercase()));
    out
}

pub fn view_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_default().join("smithy/models-view")
}

/// The link name that makes the router use `m.id` as the model's name.
fn link_name(m: &LocalModel) -> String {
    if Path::new(&m.path).is_dir() {
        m.id.clone()
    } else {
        format!("{}.gguf", m.id)
    }
}

/// Makes `view` hold exactly one symlink per model. Returns whether anything
/// changed, in which case a running router must rescan.
pub fn sync_view(view: &Path, models: &[LocalModel]) -> Result<bool, String> {
    std::fs::create_dir_all(view).map_err(|e| e.to_string())?;
    let want: std::collections::HashMap<String, PathBuf> =
        models.iter().map(|m| (link_name(m), PathBuf::from(&m.path))).collect();
    let mut changed = false;
    for entry in std::fs::read_dir(view).map_err(|e| e.to_string())?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let current = std::fs::read_link(entry.path()).ok();
        if want.get(&name) != current.as_ref() {
            // Only ever remove our own symlinks, never a real file.
            if current.is_some() {
                std::fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
                changed = true;
            }
        }
    }
    for (name, target) in &want {
        let link = view.join(name);
        if std::fs::read_link(&link).ok().as_ref() != Some(target) {
            std::os::unix::fs::symlink(target, &link).map_err(|e| format!("{}: {e}", link.display()))?;
            changed = true;
        }
    }
    Ok(changed)
}

pub fn presets_path() -> PathBuf {
    settings::config_dir().join("models.ini")
}

/// Starting presets. Every model gets the `[*]` section; Bonsai gets its
/// full native context because its KV cache is unusually cheap.
pub fn default_presets(models: &[LocalModel]) -> String {
    let mut s = String::from(
        "; Smithy model presets for llama-server's router mode.\n\
         ; [*] applies to every model; a [model name] section overrides it for one model.\n\
         ; Keys are llama-server flags without the leading dashes. Restart the local\n\
         ; server from Smithy's Models page after editing.\n\n\
         [*]\n\
         n-gpu-layers = 99\n\
         flash-attn = on\n\
         ; One slot, so a single conversation gets the whole context.\n\
         parallel = 1\n\
         ctx-size = 32768\n\
         ; Quantized KV halves cache memory for a negligible quality cost.\n\
         cache-type-k = q8_0\n\
         cache-type-v = q8_0\n",
    );
    for m in models.iter().filter(|m| m.id.to_lowercase().contains("bonsai")) {
        s.push_str(&format!(
            "\n[{}]\n\
             ; Only 16 of 64 layers use attention, so 262k context fits in ~16.4 GiB.\n\
             ctx-size = 262144\n\
             temp = 1.0\n\
             top-p = 0.95\n\
             top-k = 20\n\
             min-p = 0.0\n",
            m.id
        ));
    }
    s
}

pub fn ensure_presets(s: &Settings) -> Result<PathBuf, String> {
    let path = presets_path();
    if !path.exists() {
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&path, default_presets(&scan_all(&s.model_dirs))).map_err(|e| e.to_string())?;
    }
    Ok(path)
}

pub fn router_argv(s: &Settings, presets: &Path) -> Vec<String> {
    [
        s.llama_server.as_str(),
        "--models-dir",
        &view_dir().to_string_lossy(),
        "--models-preset",
        &presets.to_string_lossy(),
        // One model in VRAM at a time; loading another evicts it.
        "--models-max",
        "1",
        "--host",
        "127.0.0.1",
        "--port",
        &s.port.to_string(),
    ]
    .iter()
    .map(|x| x.to_string())
    .collect()
}

fn client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder().timeout(timeout).build().expect("static client config")
}

/// The router's model list, or `None` if nothing (or not a router) is listening.
pub async fn router_models(s: &Settings) -> Option<Vec<Value>> {
    let body: Value = client(Duration::from_secs(2)).get(format!("{}/models", s.base_url())).send().await.ok()?.json().await.ok()?;
    let data = body["data"].as_array()?;
    // A plain single-model llama-server also answers /models, without status.
    data.iter().all(|m| m.get("status").is_some()).then(|| data.clone())
}

/// Asks the router to re-read its models folder.
async fn rescan(s: &Settings) -> Option<Vec<Value>> {
    let body: Value =
        client(Duration::from_secs(5)).get(format!("{}/models?reload=1", s.base_url())).send().await.ok()?.json().await.ok()?;
    body["data"].as_array().cloned()
}

pub fn status_of(m: &Value) -> String {
    m["status"]["value"].as_str().unwrap_or("unloaded").to_string()
}

pub fn merge(mut scanned: Vec<LocalModel>, router: &[Value]) -> Vec<LocalModel> {
    for m in &mut scanned {
        if let Some(r) = router.iter().find(|r| r["id"] == m.id.as_str()) {
            m.status = status_of(r);
        }
    }
    scanned
}

#[derive(Debug, Serialize)]
pub struct LocalState {
    pub settings: Settings,
    /// Configured folders that aren't there right now (e.g. drive unmounted).
    pub missing: Vec<String>,
    pub presets: String,
    /// `true` when our router answers; `false` when the port is free.
    pub router: bool,
    /// Something that isn't a router holds the port (e.g. an old single-model server).
    pub port_taken: bool,
    pub models: Vec<LocalModel>,
    pub loaded: Option<LoadedInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadedInfo {
    pub id: String,
    pub n_ctx: Option<u64>,
    pub ftype: Option<String>,
}

async fn loaded_info(s: &Settings, id: &str) -> LoadedInfo {
    let props: Option<Value> = async {
        let url = reqwest::Url::parse_with_params(&format!("{}/props", s.base_url()), &[("model", id)]).ok()?;
        client(Duration::from_secs(3))
            .get(url)
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()
    }
    .await;
    let p = props.unwrap_or(Value::Null);
    LoadedInfo {
        id: id.to_string(),
        n_ctx: p["default_generation_settings"]["n_ctx"].as_u64(),
        ftype: p["model_ftype"].as_str().map(str::to_string),
    }
}

pub async fn state() -> LocalState {
    if let Err(e) = sync_pi() {
        eprintln!("smithy: {e}");
    }
    let s = Settings::load();
    let ini = std::fs::read_to_string(presets_path()).unwrap_or_default();
    let mut scanned = scan_all(&s.model_dirs);
    for m in &mut scanned {
        m.ctx = preset_ctx(&ini, &m.id);
    }
    let mut router = router_models(&s).await;
    // A drive was plugged in or a model added: let a running router rescan.
    if sync_view(&view_dir(), &scanned).unwrap_or(false) && router.is_some() {
        router = rescan(&s).await.or(router);
    }
    let port_taken = router.is_none() && crate::runtime::process::pid_on_port(s.port).is_some();
    let models = merge(scanned, router.as_deref().unwrap_or(&[]));
    let loaded = match models.iter().find(|m| m.status == "loaded") {
        Some(m) => Some(loaded_info(&s, &m.id).await),
        None => None,
    };
    LocalState {
        missing: s.model_dirs.iter().filter(|d| !Path::new(d).is_dir()).cloned().collect(),
        presets: presets_path().to_string_lossy().into_owned(),
        router: router.is_some(),
        port_taken,
        models,
        loaded,
        settings: s,
    }
}

async fn ensure_router(s: &Settings) -> Result<(), String> {
    if router_models(s).await.is_some() {
        return Ok(());
    }
    let models = scan_all(&s.model_dirs);
    if models.is_empty() {
        let missing: Vec<&String> = s.model_dirs.iter().filter(|d| !Path::new(d).is_dir()).collect();
        return Err(if missing.is_empty() {
            "no models found in your model folders".into()
        } else {
            format!("no models available; missing folders (drive not mounted?): {missing:?}")
        });
    }
    sync_view(&view_dir(), &models)?;
    if let Some(pid) = crate::runtime::process::pid_on_port(s.port) {
        return Err(format!(
            "port {} is held by another server (pid {pid}). Stop it on the Models page first.",
            s.port
        ));
    }
    let presets = ensure_presets(s)?;
    crate::runtime::process::spawn_argv("local-router", &router_argv(s, &presets))?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if router_models(s).await.is_some() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err("the local server didn't start; see ~/.local/state/smithy/logs/local-router.log".into())
}

async fn post(s: &Settings, path: &str, model: &str) -> Result<(), String> {
    let r = client(Duration::from_secs(30))
        .post(format!("{}{path}", s.base_url()))
        .json(&json!({ "model": model }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if r.status().is_success() {
        Ok(())
    } else {
        Err(format!("{path}: {}", r.text().await.unwrap_or_default()))
    }
}

async fn wait_for(s: &Settings, id: &str, want: &str, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let models = router_models(s).await.ok_or("the local server stopped")?;
        if let Some(m) = models.iter().find(|m| m["id"] == id) {
            if status_of(m) == want {
                return Ok(());
            }
            if m["status"]["failed"].as_bool() == Some(true) {
                return Err(format!(
                    "{id} failed to load (exit {}). Check the log or its preset.",
                    m["status"]["exit_code"]
                ));
            }
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
    Err(format!("timed out waiting for {id} to be {want}"))
}

/// Loads `id` as the only model in VRAM. Unloads first so two models never
/// have to fit at once.
#[tauri::command]
pub async fn local_load<R: Runtime>(app: AppHandle<R>, id: String) -> Result<LoadedInfo, String> {
    let s = Settings::load();
    if !scan_all(&s.model_dirs).iter().any(|m| m.id == id) {
        return Err(format!("{id} isn't in any model folder (is its drive mounted?)"));
    }
    ensure_router(&s).await?;
    let _ = app.emit("local-models-changed", ());
    for m in router_models(&s).await.unwrap_or_default() {
        let other = m["id"].as_str().unwrap_or_default();
        if other != id && status_of(&m) != "unloaded" {
            post(&s, "/models/unload", other).await?;
            wait_for(&s, other, "unloaded", Duration::from_secs(30)).await?;
        }
    }
    let already = router_models(&s).await.unwrap_or_default().iter().any(|m| m["id"] == id.as_str() && status_of(m) == "loaded");
    if !already {
        post(&s, "/models/load", &id).await?;
        let _ = app.emit("local-models-changed", ());
        wait_for(&s, &id, "loaded", Duration::from_secs(300)).await?;
    }
    let _ = app.emit("local-models-changed", ());
    Ok(loaded_info(&s, &id).await)
}

#[tauri::command]
pub async fn local_unload<R: Runtime>(app: AppHandle<R>, id: String) -> Result<(), String> {
    let s = Settings::load();
    post(&s, "/models/unload", &id).await?;
    wait_for(&s, &id, "unloaded", Duration::from_secs(30)).await?;
    let _ = app.emit("local-models-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn local_state() -> LocalState {
    state().await
}

/// `ctx-size` for a model from the presets INI: its own section, else `[*]`.
/// Accepts the short and long flag spellings the router does.
pub fn preset_ctx(ini: &str, id: &str) -> Option<u64> {
    let mut section = String::new();
    let (mut global, mut own) = (None, None);
    for line in ini.lines().map(str::trim) {
        if line.starts_with(';') || line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.trim().to_string();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        if !matches!(k.trim(), "ctx-size" | "c" | "LLAMA_ARG_CTX_SIZE") {
            continue;
        }
        let v = v.trim().parse().ok();
        if section == "*" {
            global = v;
        } else if section == id {
            own = v;
        }
    }
    own.or(global)
}

/// Pi's provider id for local models. `local/<model>` in its model picker.
pub const PI_PROVIDER: &str = "local";

pub fn pi_provider(s: &Settings, models: &[LocalModel], ini: &str) -> Value {
    let entries: Vec<Value> = models
        .iter()
        .map(|m| {
            let ctx = preset_ctx(ini, &m.id).unwrap_or(32768);
            json!({
                "id": m.id,
                "name": m.id,
                "reasoning": true,
                "input": if m.vision { json!(["text", "image"]) } else { json!(["text"]) },
                "contextWindow": ctx,
                "maxTokens": (ctx / 4).clamp(4096, 32768),
                "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0 },
            })
        })
        .collect();
    json!({
        "baseUrl": format!("{}/v1", s.base_url()),
        "api": "openai-completions",
        "apiKey": "local",
        // llama.cpp templates expect a plain system message.
        "compat": { "supportsDeveloperRole": false },
        "models": entries,
    })
}

fn pi_models_file() -> PathBuf {
    std::env::var_os("PI_CODING_AGENT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".pi/agent"))
        .join("models.json")
}

/// Keeps Pi's `local` provider in step with the models folder. Only that one
/// key is written; any other providers the user has are left as they are.
/// Refuses to touch a models.json that doesn't parse.
pub fn sync_pi() -> Result<bool, String> {
    let s = Settings::load();
    let models = scan_all(&s.model_dirs);
    let ini = std::fs::read_to_string(ensure_presets(&s)?).unwrap_or_default();
    let path = pi_models_file();
    let mut doc: Value = match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{} is not valid JSON ({e}); not touching it", path.display()))?,
        Err(_) => json!({}),
    };
    if !doc.is_object() {
        return Err(format!("{} is not a JSON object; not touching it", path.display()));
    }
    let provider = pi_provider(&s, &models, &ini);
    if doc["providers"][PI_PROVIDER] == provider {
        return Ok(false);
    }
    if !doc["providers"].is_object() {
        doc["providers"] = json!({});
    }
    doc["providers"][PI_PROVIDER] = provider;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&path, serde_json::to_vec_pretty(&doc).unwrap()).map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_files_and_folders_like_the_router() {
        let d = std::env::temp_dir().join(format!("smithy-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("Big-Model")).unwrap();
        std::fs::create_dir_all(d.join("empty")).unwrap();
        std::fs::write(d.join("small.gguf"), [0u8; 10]).unwrap();
        std::fs::write(d.join("mmproj-stray.gguf"), [0u8; 3]).unwrap();
        std::fs::write(d.join("notes.txt"), "x").unwrap();
        std::fs::write(d.join("Big-Model/w-00001-of-00002.gguf"), [0u8; 5]).unwrap();
        std::fs::write(d.join("Big-Model/w-00002-of-00002.gguf"), [0u8; 5]).unwrap();
        std::fs::write(d.join("Big-Model/mmproj-Q8_0.gguf"), [0u8; 2]).unwrap();
        let ms = scan(&d);
        std::fs::remove_dir_all(&d).unwrap();
        let ids: Vec<&str> = ms.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["Big-Model", "small"]);
        assert_eq!(ms[0].size, 10);
        assert!(ms[0].vision);
        assert!(!ms[1].vision);
    }

    #[test]
    fn view_links_every_folder_and_prunes() {
        let root = std::env::temp_dir().join(format!("smithy-view-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (fast, slow, view) = (root.join("nvme"), root.join("usb"), root.join("view"));
        std::fs::create_dir_all(fast.join("Big")).unwrap();
        std::fs::create_dir_all(&slow).unwrap();
        std::fs::write(fast.join("Big/w.gguf"), [0u8; 4]).unwrap();
        std::fs::write(slow.join("small.gguf"), [0u8; 2]).unwrap();
        // Same name in the slower folder loses to the first folder.
        std::fs::create_dir_all(slow.join("Big")).unwrap();
        std::fs::write(slow.join("Big/other.gguf"), [0u8; 9]).unwrap();

        let dirs = vec![fast.to_string_lossy().into_owned(), slow.to_string_lossy().into_owned(), "/not/mounted".into()];
        let ms = scan_all(&dirs);
        assert_eq!(ms.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), vec!["Big", "small"]);
        assert_eq!(ms[0].size, 4);
        assert_eq!(ms[0].folder, dirs[0]);

        assert!(sync_view(&view, &ms).unwrap());
        assert_eq!(std::fs::read_link(view.join("Big")).unwrap(), fast.join("Big"));
        assert_eq!(std::fs::read_link(view.join("small.gguf")).unwrap(), slow.join("small.gguf"));
        assert!(!sync_view(&view, &ms).unwrap(), "second sync is a no-op");

        // Drive unplugged: its models' links go away, the rest stay.
        std::fs::write(view.join("keep-me.txt"), "not a link").unwrap();
        assert!(sync_view(&view, &ms[..1]).unwrap());
        assert!(view.join("Big").exists());
        assert!(std::fs::symlink_metadata(view.join("small.gguf")).is_err());
        assert!(view.join("keep-me.txt").exists(), "never deletes real files");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn presets_give_bonsai_full_context() {
        let bonsai = LocalModel { id: "Ternary-Bonsai-2-27B".into(), path: String::new(), size: 0, vision: true, status: String::new(), ctx: None, folder: String::new() };
        let ini = default_presets(&[bonsai]);
        assert!(ini.contains("[*]\n"));
        assert!(ini.contains("[Ternary-Bonsai-2-27B]\n"));
        assert!(ini.contains("ctx-size = 262144"));
    }

    #[test]
    fn ctx_from_presets() {
        let ini = "[*]\nctx-size = 32768\n; c = 1\n[Big]\nc = 262144\n";
        assert_eq!(preset_ctx(ini, "Big"), Some(262144));
        assert_eq!(preset_ctx(ini, "Other"), Some(32768));
        assert_eq!(preset_ctx("", "x"), None);
    }

    #[test]
    fn pi_provider_lists_every_model() {
        let s = Settings { model_dirs: vec!["/m".into()], llama_server: "x".into(), port: 8081 };
        let ms = vec![
            LocalModel { id: "A".into(), path: String::new(), size: 0, vision: true, status: String::new(), ctx: None, folder: String::new() },
            LocalModel { id: "B".into(), path: String::new(), size: 0, vision: false, status: String::new(), ctx: None, folder: String::new() },
        ];
        let p = pi_provider(&s, &ms, "[*]\nctx-size = 8192\n[A]\nctx-size = 262144\n");
        assert_eq!(p["baseUrl"], "http://127.0.0.1:8081/v1");
        assert_eq!(p["models"][0]["contextWindow"], 262144);
        assert_eq!(p["models"][0]["input"], json!(["text", "image"]));
        assert_eq!(p["models"][1]["contextWindow"], 8192);
        assert_eq!(p["models"][1]["maxTokens"], 4096);
    }

    #[test]
    fn merge_takes_router_status() {
        let scanned = vec![LocalModel { id: "a".into(), path: String::new(), size: 1, vision: false, status: "unloaded".into(), ctx: None, folder: String::new() }];
        let router = vec![json!({ "id": "a", "status": { "value": "loaded" } })];
        assert_eq!(merge(scanned, &router)[0].status, "loaded");
    }
}

#[cfg(test)]
mod live {
    /// Against the real router and models folder: `cargo test live_local -- --ignored --nocapture`.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn live_local() {
        let st = super::state().await;
        println!("router={} port_taken={} loaded={:?}", st.router, st.port_taken, st.loaded);
        for m in &st.models {
            println!("  {:<24} {:>6.1} GB vision={} {}", m.id, m.size as f64 / 1e9, m.vision, m.status);
        }
        let pi: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(super::pi_models_file()).unwrap()).unwrap();
        println!("pi local provider: {}", pi["providers"]["local"]["models"].as_array().unwrap().iter().map(|m| format!("{}@{}", m["id"], m["contextWindow"])).collect::<Vec<_>>().join(", "));
    }
}
