# Smithy

A local-first desktop workbench for coding agents, built for [Omarchy](https://omarchy.org).

Smithy runs your local models and the agents that use them in one window. Open a
project folder, start a thread with an agent, and watch it read, edit and run
things, with diffs and tool output inline.

- **Local models, one click to switch.** Point Smithy at one or more model folders
  (internal drive, external drive). It serves them all through a single
  `llama-server` in router mode on one port, keeps one model in VRAM at a time, and
  swaps on demand from the composer's model picker. Any llama.cpp build works,
  including forks such as PrismML's, which Bonsai's ternary models need.
- **Any agent over ACP.** Threads talk to agents over the
  [Agent Client Protocol](https://agentclientprotocol.com): Pi for local models, and
  OpenCode, Claude Code or Codex on their own subscriptions.
- **Omarchy-native.** Colors follow your current Omarchy theme live, in your
  terminal font.

Status: early. Chat, threads, tool calls, diffs, permission prompts, session resume
and local model switching work. Diff review with comments, an integrated terminal,
git worktrees and a bar widget are next.

## Requirements

- Linux (developed on Omarchy / Hyprland)
- A `llama-server` build with router mode (`--models-dir`)
- Node 22+ and `npx` (agent adapters run through it)
- The agent CLIs you want to use: `pi`, `opencode`, `claude`, `codex`

## Develop

```bash
npm install
npm run tauri dev
```

Tests:

```bash
npm test                          # frontend reducer
cd src-tauri && cargo test        # backend
cargo test -- --ignored --nocapture   # live tests against a running local model
```

## Where things live

| | |
|---|---|
| Settings | `~/.config/smithy/settings.json` |
| Per-model presets (context, sampling) | `~/.config/smithy/models.ini` |
| Threads and transcripts | `~/.local/share/smithy/` |
| Logs | `~/.local/state/smithy/logs/` |

Smithy also maintains one provider, `local`, in Pi's `~/.pi/agent/models.json` so
Pi sees every local model. It never touches the other entries in that file.
