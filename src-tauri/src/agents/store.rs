//! Durable app state: projects, thread metadata, per-agent preferences, and
//! one JSONL transcript per thread. Everything lives under
//! `~/.local/share/smithy/`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Thread {
    pub id: String,
    pub project: String,
    pub agent: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub title: String,
    /// Where the agent works. `None` means the project folder itself; a path
    /// means a git worktree of it on `branch`.
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    pub created: u64,
    pub updated: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub projects: Vec<Project>,
    #[serde(default)]
    pub threads: Vec<Thread>,
    /// agent id → config option id → last chosen value (model, thinking level…).
    #[serde(default)]
    pub prefs: HashMap<String, HashMap<String, String>>,
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn new_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    format!("{:x}{:04x}", now(), SEQ.fetch_add(1, Ordering::Relaxed) & 0xffff)
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("smithy")
}

fn state_file(dir: &Path) -> PathBuf {
    dir.join("state.json")
}

pub fn transcript_file(dir: &Path, thread: &str) -> PathBuf {
    dir.join("threads").join(format!("{thread}.jsonl"))
}

impl Thread {
    /// The folder the agent, terminal and review panel work in.
    pub fn workdir(&self) -> &str {
        self.cwd.as_deref().unwrap_or(&self.project)
    }
}

impl State {
    pub fn load(dir: &Path) -> State {
        fs::read_to_string(state_file(dir))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Write-then-rename so a crash mid-save can't truncate the state.
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let tmp = dir.join("state.json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self).unwrap()).map_err(|e| e.to_string())?;
        fs::rename(tmp, state_file(dir)).map_err(|e| e.to_string())
    }

    pub fn thread_mut(&mut self, id: &str) -> Option<&mut Thread> {
        self.threads.iter_mut().find(|t| t.id == id)
    }
}

pub fn append(dir: &Path, thread: &str, event: &Value) -> Result<(), String> {
    let path = transcript_file(dir, thread);
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let mut f = fs::OpenOptions::new().create(true).append(true).open(path).map_err(|e| e.to_string())?;
    writeln!(f, "{event}").map_err(|e| e.to_string())
}

/// Skips lines that don't parse, e.g. one cut short by a crash.
pub fn history(dir: &Path, thread: &str) -> Vec<Value> {
    fs::read_to_string(transcript_file(dir, thread))
        .map(|s| s.lines().filter_map(|l| serde_json::from_str(l).ok()).collect())
        .unwrap_or_default()
}

/// A thread title from the first prompt: first line, cut at a word boundary.
pub fn title_from(prompt: &str) -> String {
    let line = prompt.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    if line.chars().count() <= 60 {
        return line.to_string();
    }
    let cut: String = line.chars().take(60).collect();
    match cut.rfind(' ') {
        Some(i) if i > 30 => format!("{}…", &cut[..i]),
        _ => format!("{cut}…"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("smithy-store-{}", new_id()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn state_round_trips() {
        let dir = tmp();
        let mut s = State::default();
        s.projects.push(Project { path: "/p".into(), name: "p".into() });
        s.prefs.entry("pi".into()).or_default().insert("model".into(), "bonsai/x".into());
        s.save(&dir).unwrap();
        let back = State::load(&dir);
        assert_eq!(back.projects, s.projects);
        assert_eq!(back.prefs["pi"]["model"], "bonsai/x");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn transcript_skips_torn_lines() {
        let dir = tmp();
        append(&dir, "t", &json!({"t": "user", "text": "hi"})).unwrap();
        let path = transcript_file(&dir, "t");
        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        write!(f, "{{\"t\":\"upd").unwrap();
        assert_eq!(history(&dir, "t").len(), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn titles() {
        assert_eq!(title_from("\n  fix the test\nmore"), "fix the test");
        let long = "refactor the runtime manager so that profiles can be reloaded without restarting the app";
        let t = title_from(long);
        assert!(t.ends_with('…') && t.chars().count() <= 61, "{t}");
    }

    #[test]
    fn ids_are_unique() {
        assert_ne!(new_id(), new_id());
    }
}
