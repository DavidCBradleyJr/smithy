import { invoke } from "@tauri-apps/api/core";
import type { ConfigOption, RawEvent } from "./reduce";

export type Kind = "llama-server" | "lm-studio" | "ollama";
export type State = "stopped" | "starting" | "running";

export interface Profile {
  id: string;
  name: string;
  kind: Kind;
  start: string[];
  stop: string[];
  host: string;
  port: number;
  vram_mib: number | null;
  expect_ftype: string | null;
}

export interface Status {
  profile: Profile;
  state: State;
  models: string[];
  n_ctx: number | null;
  ftype: string | null;
  ftype_ok: boolean | null;
  pid: number | null;
  can_start: boolean;
  can_stop: boolean;
  log: string | null;
}

export interface Vram {
  name: string;
  used_mib: number;
  total_mib: number;
}

export interface Agent {
  id: string;
  name: string;
  blurb: string;
  requires: string;
  available: boolean;
}

export interface Project {
  path: string;
  name: string;
}

export interface Thread {
  id: string;
  project: string;
  agent: string;
  session_id: string | null;
  title: string;
  /** Git worktree folder, when the thread has its own. */
  cwd: string | null;
  branch: string | null;
  created: number;
  updated: number;
}

export function workdir(t: Thread) {
  return t.cwd ?? t.project;
}

export interface FileChange {
  path: string;
  status: "M" | "A" | "D" | "R" | "?" | "U";
  orig: string | null;
  additions: number | null;
  deletions: number | null;
}

export interface Changes {
  repo: boolean;
  branch: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  files: FileChange[];
}

export interface FileDiff {
  old: string | null;
  new: string | null;
  binary: boolean;
  too_large: boolean;
}

export interface Settings {
  model_dirs: string[];
  llama_server: string;
  port: number;
}

export interface LocalModel {
  id: string;
  path: string;
  size: number;
  vision: boolean;
  status: string;
  ctx: number | null;
  folder: string;
}

export interface LocalState {
  settings: Settings;
  missing: string[];
  presets: string;
  router: boolean;
  port_taken: boolean;
  models: LocalModel[];
  loaded: { id: string; n_ctx: number | null; ftype: string | null } | null;
}

export const api = {
  settings: () => invoke<Settings>("settings_get"),
  saveSettings: (settings: Settings) => invoke<Settings>("settings_set", { settings }),
  local: () => invoke<LocalState>("local_state"),
  loadModel: (id: string) => invoke<{ id: string; n_ctx: number | null }>("local_load", { id }),
  unloadModel: (id: string) => invoke<void>("local_unload", { id }),

  theme: () => invoke<Record<string, string>>("get_theme"),
  vram: () => invoke<Vram | null>("get_vram"),
  status: () => invoke<Status[]>("runtime_status"),
  start: (id: string, force = false) => invoke<number>("runtime_start", { id, force }),
  stop: (id: string) => invoke<void>("runtime_stop", { id }),

  agents: () => invoke<Agent[]>("agents_list"),
  projects: () => invoke<Project[]>("projects_list"),
  addProject: (path: string) => invoke<Project>("project_add", { path }),
  removeProject: (path: string) => invoke<void>("project_remove", { path }),
  branch: (path: string) => invoke<string | null>("project_branch", { path }),

  threads: () => invoke<Thread[]>("threads_list"),
  live: () => invoke<string[]>("thread_live"),
  createThread: (project: string, agent: string, worktree: boolean) =>
    invoke<Thread>("thread_create", { project, agent, worktree }),
  history: (id: string) => invoke<RawEvent[]>("thread_history", { id }),
  deleteThread: (id: string, force = false) => invoke<void>("thread_delete", { id, force }),
  prompt: (id: string, text: string) => invoke<string>("thread_prompt", { id, text }),
  cancel: (id: string) => invoke<void>("thread_cancel", { id }),
  setConfig: (id: string, configId: string, value: string) =>
    invoke<ConfigOption[]>("thread_set_config", { id, configId, value }),
  respond: (key: string, optionId: string | null) => invoke<void>("permission_respond", { key, optionId }),

  changes: (cwd: string) => invoke<Changes>("git_changes", { cwd }),
  fileDiff: (cwd: string, path: string, orig: string | null) => invoke<FileDiff>("git_file_diff", { cwd, path, orig }),
  commit: (cwd: string, message: string) => invoke<string>("git_commit", { cwd, message }),
  push: (cwd: string) => invoke<string>("git_push", { cwd }),
  merge: (project: string, branch: string) => invoke<string>("git_merge", { project, branch }),

  ptyOpen: (id: string, cwd: string, cols: number, rows: number) => invoke<string>("pty_open", { id, cwd, cols, rows }),
  ptyWrite: (id: string, data: string) => invoke<void>("pty_write", { id, data }),
  ptyResize: (id: string, cols: number, rows: number) => invoke<void>("pty_resize", { id, cols, rows }),
  ptyClose: (id: string) => invoke<void>("pty_close", { id }),
};
