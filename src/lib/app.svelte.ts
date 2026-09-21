// App-wide state. One instance, imported where needed.

import { listen } from "@tauri-apps/api/event";
import { api, type Agent, type Project, type Thread } from "./api";
import { reduce, type RawEvent, type ThreadView } from "./reduce";
import type { ReviewComment } from "./review";

export type { ReviewComment } from "./review";
export { reviewPrompt } from "./review";

export type View = { name: "thread"; id: string } | { name: "new"; project: string } | { name: "models" } | { name: "settings" } | { name: "home" };

class App {
  projects = $state<Project[]>([]);
  threads = $state<Thread[]>([]);
  agents = $state<Agent[]>([]);
  branches = $state<Record<string, string | null>>({});
  view = $state<View>({ name: "home" });
  /** Raw transcript per thread, loaded lazily. */
  events = $state<Record<string, RawEvent[]>>({});
  /** Threads with a prompt in flight from this window. */
  inflight = $state<Record<string, boolean>>({});
  /** Review comments not yet sent to the agent, per thread. */
  comments = $state<Record<string, ReviewComment[]>>({});
  /** Which side panels are open (shared across threads). */
  panels = $state({ changes: false, terminal: false });

  current = $derived(this.view.name === "thread" ? this.threads.find((t) => t.id === (this.view as any).id) ?? null : null);

  async init() {
    [this.projects, this.threads, this.agents] = await Promise.all([api.projects(), api.threads(), api.agents()]);
    for (const p of this.projects) this.refreshBranch(p.path);
    await listen<{ thread: string; event: RawEvent }>("thread-event", ({ payload }) => {
      const list = this.events[payload.thread];
      if (list) list.push(payload.event);
    });
    const first = this.threads[0];
    if (first) this.open(first.id);
    else if (this.projects[0]) this.view = { name: "new", project: this.projects[0].path };
  }

  async refreshBranch(path: string) {
    this.branches[path] = await api.branch(path);
  }

  viewOf(id: string): ThreadView {
    return reduce(this.events[id] ?? []);
  }

  /** Threads whose agent this window has (re)connected. */
  #connected = new Set<string>();

  async open(id: string) {
    this.view = { name: "thread", id };
    if (!this.events[id]) this.events[id] = await api.history(id);
    // Reconnect in the background so the composer's options are current.
    if (!this.#connected.has(id)) {
      this.#connected.add(id);
      api.resume(id).catch(() => this.#connected.delete(id));
    }
  }

  async addProject(path: string) {
    const p = await api.addProject(path);
    if (!this.projects.some((x) => x.path === p.path)) this.projects.push(p);
    this.refreshBranch(p.path);
    this.view = { name: "new", project: p.path };
  }

  async removeProject(path: string) {
    await api.removeProject(path);
    this.projects = this.projects.filter((p) => p.path !== path);
    if (this.view.name === "new" && this.view.project === path) this.view = { name: "home" };
  }

  async newThread(project: string, agent: string, worktree = false) {
    // Show the thread immediately; the agent's startup events stream in.
    const t = await api.createThread(project, agent, worktree);
    this.threads.unshift(t);
    this.#connected.add(t.id);
    this.events[t.id] = await api.history(t.id);
    this.view = { name: "thread", id: t.id };
  }

  /** Throws "uncommitted changes in the worktree" unless `force`. */
  async deleteThread(id: string, force = false) {
    await api.deleteThread(id, force);
    this.threads = this.threads.filter((t) => t.id !== id);
    delete this.events[id];
    if (this.view.name === "thread" && this.view.id === id) this.view = { name: "home" };
  }

  async send(id: string, text: string) {
    this.inflight[id] = true;
    const t = this.threads.find((x) => x.id === id);
    if (t) {
      t.updated = Date.now();
      this.threads.sort((a, b) => b.updated - a.updated);
    }
    try {
      await api.prompt(id, text);
    } catch {
      // Already recorded as an error event in the transcript.
    } finally {
      this.inflight[id] = false;
      // The backend titled the thread from its first prompt.
      const fresh = await api.threads();
      for (const f of fresh) {
        const mine = this.threads.find((x) => x.id === f.id);
        if (mine) mine.title = f.title;
      }
    }
  }
}

export const app = new App();
