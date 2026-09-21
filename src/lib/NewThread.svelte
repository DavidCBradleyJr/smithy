<script lang="ts">
  import { app } from "./app.svelte";
  import Spinner from "./Spinner.svelte";

  let { project }: { project: string } = $props();

  const p = $derived(app.projects.find((x) => x.path === project));
  const agents = $derived(app.agents);
  let starting = $state<string | null>(null);
  let error = $state("");
  let confirmRemove = $state(false);

  async function start(id: string) {
    if (starting) return;
    starting = id;
    error = "";
    try {
      await app.newThread(project, id);
    } catch (e) {
      error = String(e);
    } finally {
      starting = null;
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLInputElement) return;
    const n = Number(e.key);
    const a = agents.filter((x) => x.available)[n - 1];
    if (a) start(a.id);
  }
</script>

<svelte:window {onkeydown} />

<section class="new">
  <div class="head">
    <div class="k">new thread in</div>
    <h1>{p?.name ?? project}</h1>
    <div class="path">{project}</div>
  </div>

  <div class="k">agent</div>
  <div class="list">
    {#each agents.filter((a) => a.available) as a, i (a.id)}
      <button class="agent" disabled={!!starting} onclick={() => start(a.id)}>
        <kbd>{i + 1}</kbd>
        <span class="name">{a.name}</span>
        <span class="blurb">{a.blurb}</span>
        {#if starting === a.id}<span class="st"><Spinner /> starting</span>{/if}
      </button>
    {/each}
    {#each agents.filter((a) => !a.available) as a (a.id)}
      <div class="agent off">
        <kbd> </kbd>
        <span class="name">{a.name}</span>
        <span class="blurb">install <code>{a.requires}</code> to use</span>
      </div>
    {/each}
  </div>
  {#if error}<div class="error">{error}</div>{/if}

  <button class="remove" class:confirm={confirmRemove} onclick={() => (confirmRemove ? app.removeProject(project) : (confirmRemove = true))}>
    {confirmRemove ? "remove from smithy? (files are untouched)" : "remove project"}
  </button>
</section>

<style>
  .new {
    padding: 48px 48px;
    max-width: 720px;
  }
  .head {
    margin-bottom: 32px;
  }
  .k {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--dark-foreground);
    margin-bottom: 6px;
  }
  h1 {
    margin: 0;
    font-size: 20px;
  }
  .path {
    color: var(--dark-foreground);
    font-size: 12px;
  }
  .list {
    border-top: 1px solid var(--muted);
  }
  .agent {
    all: unset;
    box-sizing: border-box;
    display: flex;
    align-items: baseline;
    gap: 2ch;
    width: 100%;
    padding: 10px 8px;
    border-bottom: 1px solid var(--muted);
    cursor: pointer;
  }
  .agent:hover:not(:disabled):not(.off),
  .agent:focus-visible {
    background: var(--lighter-background);
  }
  .agent:hover .name {
    color: var(--accent);
  }
  .agent.off {
    cursor: default;
    opacity: 0.45;
  }
  kbd {
    font: inherit;
    color: var(--dark-foreground);
    width: 1ch;
  }
  .name {
    width: 14ch;
    font-weight: 700;
  }
  .blurb {
    flex: 1;
    color: var(--light-foreground);
  }
  .st {
    color: var(--light-foreground);
  }
  code {
    color: var(--accent);
  }
  .error {
    margin-top: 12px;
    color: var(--red);
  }
  .remove {
    all: unset;
    margin-top: 48px;
    display: inline-block;
    cursor: pointer;
    font-size: 11px;
    color: var(--dark-foreground);
  }
  .remove:hover,
  .remove.confirm {
    color: var(--red);
  }
</style>
