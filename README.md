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
- **Review like a pull request.** The changes panel (`Ctrl+Shift+G`) shows every
  changed file. Click any line to leave a comment, then send all your comments to the
  agent as one message. Commit, push, and merge from the same panel.
- **Worktrees per thread.** A thread can work on its own branch in its own folder, so
  parallel agents never trip over each other or your checkout.
- **A terminal per thread** (``Ctrl+` ``) in the thread's folder, themed to match.
- **Omarchy-native.** Colors follow your current Omarchy theme live, in your
  terminal font. Desktop notifications when an agent finishes or needs approval.

Status: early but usable. A bar widget for omarchy-shell is next.

## Install

Arch / Omarchy: download the package from the
[latest release](https://github.com/DavidCBradleyJr/smithy/releases/latest) and install it:

```bash
sudo pacman -U smithy-0.1.0-1-x86_64.pkg.tar.zst
```

Or build it from source with `makepkg -si` in `packaging/`.

It installs `smithy`, a desktop entry and icons. You also need a `llama-server` with
router mode for local models, and the CLIs of the agents you want.

## Requirements

- Linux (developed on Omarchy / Hyprland)
- A `llama-server` build with router mode (`--models-dir`)
- Node 22+ and `npx` (agent adapters run through it)
- The agent CLIs you want to use: `pi`, `opencode`, `claude`, `codex`

## Keys

| | |
|---|---|
| `Enter` / `Shift+Enter` | send / new line |
| `Esc` | stop the agent |
| `Ctrl+N` | new thread |
| `Ctrl+Shift+G` | changes panel |
| ``Ctrl+` `` | terminal |
| `1`–`9` | answer a permission prompt, or pick an agent |

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
| Thread worktrees | `~/.local/share/smithy/worktrees/` |
| Logs | `~/.local/state/smithy/logs/` |

Smithy also maintains one provider, `local`, in Pi's `~/.pi/agent/models.json` so
Pi sees every local model. It never touches the other entries in that file.
