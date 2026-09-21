//! Model runtime manager: profiles, status, start/stop with a VRAM guard.

pub mod probe;
pub mod process;
pub mod profiles;

use crate::vram;
use probe::{Probe, State};
use profiles::Profile;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub profile: Profile,
    #[serde(flatten)]
    pub probe: Probe,
    pub pid: Option<u32>,
    pub can_start: bool,
    pub can_stop: bool,
    pub log: Option<String>,
}

async fn status_of(p: Profile) -> Status {
    let probe = probe::probe(&p).await;
    let port = p.port;
    let pid = tauri::async_runtime::spawn_blocking(move || process::pid_on_port(port))
        .await
        .ok()
        .flatten();
    Status {
        can_start: !p.start.is_empty() && probe.state == State::Stopped,
        can_stop: probe.state != State::Stopped && (!p.stop.is_empty() || pid.is_some()),
        log: process::log_path(&p.id)
            .filter(|l| l.exists())
            .map(|l| l.to_string_lossy().into_owned()),
        pid,
        probe,
        profile: p,
    }
}

fn find(id: &str) -> Result<Profile, String> {
    profiles::load()
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("unknown profile {id}"))
}

#[tauri::command]
pub async fn runtime_status() -> Vec<Status> {
    let mut out = Vec::new();
    for p in profiles::load() {
        out.push(status_of(p).await);
    }
    out
}

/// Starts a profile. Refuses when its expected VRAM doesn't fit in what's
/// free right now, unless `force` is set.
#[tauri::command]
pub async fn runtime_start(id: String, force: bool) -> Result<u32, String> {
    let p = find(&id)?;
    if probe::probe(&p).await.state != State::Stopped {
        return Err(format!("{} is already running on port {}", p.name, p.port));
    }
    if let (false, Some(need)) = (force, p.vram_mib) {
        let gpu = tauri::async_runtime::spawn_blocking(vram::query)
            .await
            .map_err(|e| e.to_string())?;
        if let Some(gpu) = gpu.filter(|g| !g.fits(need)) {
            return Err(format!(
                "VRAM: {} needs ~{need} MiB but only {} MiB is free on {}",
                p.name,
                gpu.free_mib(),
                gpu.name
            ));
        }
    }
    if p.kind == profiles::Kind::LlamaRouter {
        let st = crate::settings::Settings::load();
        let presets = crate::local::ensure_presets(&st)?;
        return process::spawn_argv(&p.id, &crate::local::router_argv(&st, &presets));
    }
    process::spawn(&p)
}

#[tauri::command]
pub async fn runtime_stop(id: String) -> Result<(), String> {
    let p = find(&id)?;
    tauri::async_runtime::spawn_blocking(move || {
        if !p.stop.is_empty() {
            return process::run_stop(&p);
        }
        let pid = process::pid_on_port(p.port)
            .ok_or_else(|| format!("nothing of yours is listening on port {}", p.port))?;
        process::terminate(pid)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod live {
    /// Against this machine's real servers: `cargo test -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn live_status() {
        for s in super::runtime_status().await {
            println!(
                "{:<14} {:?} port={} pid={:?} n_ctx={:?} ftype_ok={:?} models={:?} start={} stop={}",
                s.profile.id, s.probe.state, s.profile.port, s.pid, s.probe.n_ctx,
                s.probe.ftype_ok, s.probe.models, s.can_start, s.can_stop
            );
        }
        println!("{:?}", crate::vram::query());
    }
}
