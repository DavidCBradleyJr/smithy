// Shared, self-refreshing view of the local models folder and router.

import { listen } from "@tauri-apps/api/event";
import { api, type LocalState } from "./api";

class Local {
  state = $state<LocalState | null>(null);
  /** Model currently being loaded from this window, with its start time. */
  loading = $state<{ id: string; since: number } | null>(null);
  error = $state("");
  #started = false;

  start() {
    if (this.#started) return;
    this.#started = true;
    this.refresh();
    setInterval(() => this.refresh(), 5000);
    listen("local-models-changed", () => this.refresh());
  }

  async refresh() {
    try {
      this.state = await api.local();
    } catch (e) {
      this.error = String(e);
    }
  }

  /** Loads `id` as the one model in VRAM. Resolves once it can serve. */
  async load(id: string) {
    this.loading = { id, since: Date.now() };
    this.error = "";
    try {
      await api.loadModel(id);
    } catch (e) {
      this.error = String(e);
      throw e;
    } finally {
      this.loading = null;
      await this.refresh();
    }
  }
}

export const local = new Local();

export function gb(bytes: number) {
  return `${(bytes / 1e9).toFixed(1)} GB`;
}

export function ctxLabel(n: number | null | undefined) {
  if (!n) return "";
  return n >= 1024 ? `${Math.round(n / 1024)}k` : `${n}`;
}

/** "External 4TB" for a removable drive, "~/models" for a home folder. */
export function folderLabel(path: string) {
  const drive = path.match(/^\/run\/media\/[^/]+\/([^/]+)/);
  if (drive) return drive[1];
  return path.replace(/^\/home\/[^/]+/, "~");
}
