//! Runtime profiles: how to start a model server and where it listens.
//!
//! Built-ins: the router over the models folder, plus LM Studio and Ollama
//! when installed. Users add or override profiles by id in
//! `~/.config/smithy/profiles.toml`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// llama.cpp `llama-server`, including forks; health from `/props`.
    #[default]
    LlamaServer,
    /// llama-server in router mode over the models folder (see `local.rs`).
    LlamaRouter,
    LmStudio,
    Ollama,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub kind: Kind,
    /// argv to start the server. Empty means Smithy can't start it (e.g. a
    /// systemd-managed Ollama) and only reports its status.
    #[serde(default)]
    pub start: Vec<String>,
    /// argv to stop it. Empty means "signal the process listening on the port".
    #[serde(default)]
    pub stop: Vec<String>,
    #[serde(default = "default_host")]
    pub host: String,
    pub port: u16,
    /// Expected VRAM once loaded, used to refuse a start that would OOM.
    #[serde(default)]
    pub vram_mib: Option<u64>,
    /// Substring that must appear in `/props` `model_ftype`. Catches a runtime
    /// that silently falls back to the wrong kernels.
    #[serde(default)]
    pub expect_ftype: Option<String>,
}

fn default_host() -> String {
    "127.0.0.1".into()
}

impl Profile {
    pub fn base_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Default, Deserialize)]
struct ProfilesFile {
    #[serde(default, rename = "profile")]
    profiles: Vec<Profile>,
}

/// The router over the models folder. Its argv is filled in at start time,
/// because the presets file may not exist yet.
fn router(settings: &crate::settings::Settings) -> Profile {
    Profile {
        id: "local".into(),
        name: "Local models".into(),
        kind: Kind::LlamaRouter,
        start: vec![settings.llama_server.clone()],
        stop: vec![],
        host: default_host(),
        port: settings.port,
        vram_mib: None,
        expect_ftype: None,
    }
}

fn on_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

pub fn builtin(settings: &crate::settings::Settings, has_bin: impl Fn(&str) -> bool) -> Vec<Profile> {
    let mut out = vec![router(settings)];
    if has_bin("lms") {
        out.push(Profile {
            id: "lmstudio".into(),
            name: "LM Studio".into(),
            kind: Kind::LmStudio,
            start: vec!["lms".into(), "server".into(), "start".into()],
            stop: vec!["lms".into(), "server".into(), "stop".into()],
            host: default_host(),
            port: 1234,
            vram_mib: None,
            expect_ftype: None,
        });
    }
    if has_bin("ollama") {
        out.push(Profile {
            id: "ollama".into(),
            name: "Ollama".into(),
            kind: Kind::Ollama,
            start: vec![],
            stop: vec![],
            host: default_host(),
            port: 11434,
            vram_mib: None,
            expect_ftype: None,
        });
    }
    out
}

/// User profiles replace built-ins with the same id and append the rest.
pub fn merge(mut base: Vec<Profile>, user: Vec<Profile>) -> Vec<Profile> {
    for p in user {
        match base.iter_mut().find(|b| b.id == p.id) {
            Some(slot) => *slot = p,
            None => base.push(p),
        }
    }
    base
}

pub fn user_file() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("smithy/profiles.toml"))
}

pub fn parse_user(src: &str) -> Result<Vec<Profile>, String> {
    toml::from_str::<ProfilesFile>(src)
        .map(|f| f.profiles)
        .map_err(|e| e.to_string())
}

pub fn load() -> Vec<Profile> {
    let user = user_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| parse_user(&s).unwrap_or_default())
        .unwrap_or_default();
    merge(builtin(&crate::settings::Settings::load(), on_path), user)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_profile_follows_settings() {
        let st = crate::settings::Settings { model_dirs: vec!["/m".into()], llama_server: "/bin/ls".into(), port: 8099 };
        let ps = builtin(&st, |_| false);
        assert_eq!(ps.len(), 1);
        assert_eq!(ps[0].kind, Kind::LlamaRouter);
        assert_eq!(ps[0].port, 8099);
    }

    #[test]
    fn user_profiles_override_by_id() {
        let user = parse_user(
            r#"
            [[profile]]
            id = "ollama"
            name = "Ollama (custom)"
            kind = "ollama"
            port = 11500

            [[profile]]
            id = "qwen"
            name = "Qwen"
            start = ["llama-server", "-m", "q.gguf"]
            port = 8090
            "#,
        )
        .unwrap();
        let merged = merge(builtin(&crate::settings::Settings::default(), |b| b == "ollama"), user);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[1].port, 11500);
        assert_eq!(merged[2].kind, Kind::LlamaServer);
        assert_eq!(merged[2].host, "127.0.0.1");
    }
}
