//! Agents Smithy can drive over ACP. Adapter versions are pinned so a
//! breaking adapter release can't change behaviour underneath a user.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AgentDef {
    pub id: &'static str,
    pub name: &'static str,
    /// What it's for, shown in the new-thread picker.
    pub blurb: &'static str,
    #[serde(skip)]
    pub command: &'static [&'static str],
    /// The CLI that must be installed and logged in for the adapter to work.
    pub requires: &'static str,
    pub available: bool,
}

const AGENTS: &[(&str, &str, &str, &[&str], &str)] = &[
    ("pi", "Pi", "Local models. Small prompt, four tools", &["npx", "-y", "pi-acp@0.0.33"], "pi"),
    ("opencode", "OpenCode", "OpenCode subscription and providers", &["opencode", "acp"], "opencode"),
    (
        "claude",
        "Claude Code",
        "Claude subscription",
        &["npx", "-y", "@agentclientprotocol/claude-agent-acp@0.79.0"],
        "claude",
    ),
    ("codex", "Codex", "ChatGPT subscription", &["npx", "-y", "@zed-industries/codex-acp@0.16.0"], "codex"),
];

pub fn on_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

pub fn all() -> Vec<AgentDef> {
    AGENTS
        .iter()
        .map(|&(id, name, blurb, command, requires)| AgentDef {
            id,
            name,
            blurb,
            command,
            requires,
            available: on_path(requires) && on_path(command[0]),
        })
        .collect()
}

pub fn get(id: &str) -> Option<AgentDef> {
    all().into_iter().find(|a| a.id == id)
}

pub fn argv(def: &AgentDef) -> Vec<String> {
    def.command.iter().map(|s| s.to_string()).collect()
}
