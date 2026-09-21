<script lang="ts">
  import { open as pickFolder } from "@tauri-apps/plugin-dialog";
  import { app } from "./app.svelte";
  import { ago } from "./format";
  import VramMeter from "./VramMeter.svelte";
  import LocalModel from "./LocalModel.svelte";
  import Spinner from "./Spinner.svelte";

  let now = $state(Date.now());
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(t);
  });

  let collapsed = $state<Record<string, boolean>>({});
  let confirmDelete = $state<string | null>(null);

  async function addProject() {
    const dir = await pickFolder({ directory: true, title: "Add project folder" });
    if (typeof dir === "string") await app.addProject(dir);
  }

  function threadsOf(path: string) {
    return app.threads.filter((t) => t.project === path);
  }

  function del(e: MouseEvent, id: string) {
    e.stopPropagation();
    if (confirmDelete === id) {
      app.deleteThread(id);
      confirmDelete = null;
    } else confirmDelete = id;
  }

  const active = $derived(app.view.name === "thread" ? app.view.id : null);
</script>

<nav>
  <div class="brand">smithy</div>

  <div class="tree">
    {#each app.projects as p (p.path)}
      <div class="project">
        <div class="phead">
          <button class="pname" onclick={() => (collapsed[p.path] = !collapsed[p.path])} title={p.path}>
            <span class="chev">{collapsed[p.path] ? "▸" : "▾"}</span>{p.name}
          </button>
          {#if app.branches[p.path]}<span class="branch">{app.branches[p.path]}</span>{/if}
          <button class="add" title="New thread in {p.name}" onclick={() => (app.view = { name: "new", project: p.path })}>+</button>
        </div>
        {#if !collapsed[p.path]}
          {#each threadsOf(p.path) as t (t.id)}
            {@const v = app.viewOf(t.id)}
            <div
              class="thread"
              class:active={active === t.id}
              role="button"
              tabindex="0"
              onclick={() => app.open(t.id)}
              onkeydown={(e) => e.key === "Enter" && app.open(t.id)}
              onmouseleave={() => confirmDelete === t.id && (confirmDelete = null)}
            >
              <span class="state">
                {#if app.inflight[t.id] && v.waiting}<span class="ask">?</span>
                {:else if app.inflight[t.id]}<Spinner />
                {:else}<span class="idle">·</span>{/if}
              </span>
              <span class="ttitle">{t.title || "New thread"}</span>
              <span class="when">{ago(t.updated, now)}</span>
              <button class="del" class:confirm={confirmDelete === t.id} title="Delete thread" onclick={(e) => del(e, t.id)}>
                {confirmDelete === t.id ? "delete?" : "×"}
              </button>
            </div>
          {/each}
          {#if threadsOf(p.path).length === 0}
            <button class="thread ghost" onclick={() => (app.view = { name: "new", project: p.path })}>start a thread</button>
          {/if}
        {/if}
      </div>
    {/each}
    <button class="addp" onclick={addProject}>+ add project</button>
  </div>

  <div class="foot">
    <button class="models" class:active={app.view.name === "models"} onclick={() => (app.view = { name: "models" })}>
      <LocalModel />
    </button>
    <VramMeter />
    <div class="links">
      <button class:active={app.view.name === "models"} onclick={() => (app.view = { name: "models" })}>servers</button>
      <button class:active={app.view.name === "settings"} onclick={() => (app.view = { name: "settings" })}>settings</button>
    </div>
  </div>
</nav>

<style>
  nav {
    display: flex;
    flex-direction: column;
    background: var(--dark-background);
    border-right: 1px solid var(--muted);
    min-height: 0;
    font-size: 12px;
  }
  .brand {
    padding: 12px 14px 10px;
    color: var(--accent);
    font-weight: 700;
    letter-spacing: 0.25em;
    text-transform: uppercase;
    font-size: 11px;
  }
  .tree {
    flex: 1;
    overflow-y: auto;
    padding: 0 6px 12px;
  }
  button {
    all: unset;
    box-sizing: border-box;
    cursor: pointer;
  }
  .project {
    margin-bottom: 10px;
  }
  .phead {
    display: flex;
    align-items: baseline;
    gap: 1ch;
    padding: 3px 8px;
  }
  .pname {
    font-weight: 700;
    color: var(--foreground);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chev {
    color: var(--dark-foreground);
    display: inline-block;
    width: 2ch;
  }
  .branch {
    color: var(--dark-foreground);
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .add {
    color: var(--dark-foreground);
    padding: 0 4px;
    margin-left: auto;
  }
  .add:hover {
    color: var(--accent);
  }
  .thread {
    display: flex;
    align-items: baseline;
    gap: 1ch;
    width: 100%;
    padding: 3px 8px 3px 10px;
    color: var(--light-foreground);
    border-left: 2px solid transparent;
    cursor: pointer;
    outline: 0;
  }
  .thread:hover,
  .thread:focus-visible {
    background: var(--lighter-background);
  }
  .thread.active {
    background: var(--selection);
    border-left-color: var(--accent);
    color: var(--foreground);
  }
  .thread.ghost {
    color: var(--dark-foreground);
    font-style: italic;
  }
  .state {
    width: 1ch;
    flex: none;
  }
  .idle {
    color: var(--dark-foreground);
  }
  .ask {
    color: var(--yellow);
    font-weight: 700;
  }
  .ttitle {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .when {
    color: var(--dark-foreground);
    font-size: 11px;
  }
  .del {
    display: none;
    color: var(--dark-foreground);
    font-size: 11px;
  }
  .thread:hover .when {
    display: none;
  }
  .thread:hover .del,
  .del.confirm {
    display: inline;
  }
  .del:hover,
  .del.confirm {
    color: var(--red);
  }
  .addp {
    padding: 6px 8px;
    color: var(--dark-foreground);
  }
  .addp:hover {
    color: var(--accent);
  }
  .foot {
    border-top: 1px solid var(--muted);
    padding: 10px 14px 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .models {
    display: block;
  }
  .links {
    display: flex;
    gap: 2ch;
    color: var(--dark-foreground);
  }
  .links button:hover,
  .links button.active {
    color: var(--accent);
  }
  .models:hover :global(.lm),
  .models.active :global(.lm) {
    color: var(--foreground);
  }
</style>
