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
  created: number;
  updated: number;
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
  createThread: (project: string, agent: string) => invoke<Thread>("thread_create", { project, agent }),
  history: (id: string) => invoke<RawEvent[]>("thread_history", { id }),
  deleteThread: (id: string) => invoke<void>("thread_delete", { id }),
  prompt: (id: string, text: string) => invoke<string>("thread_prompt", { id, text }),
  cancel: (id: string) => invoke<void>("thread_cancel", { id }),
  setConfig: (id: string, configId: string, value: string) =>
    invoke<ConfigOption[]>("thread_set_config", { id, configId, value }),
  respond: (key: string, optionId: string | null) => invoke<void>("permission_respond", { key, optionId }),
};
