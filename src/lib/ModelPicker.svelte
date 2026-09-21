<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "./app.svelte";
  import { api } from "./api";
  import { local, gb, ctxLabel, folderLabel } from "./local.svelte";
  import type { ConfigOption } from "./reduce";
  import Spinner from "./Spinner.svelte";
  import { duration } from "./format";

  let {
    threadId,
    option,
    disabled,
  }: { threadId: string; option: ConfigOption; disabled: boolean } = $props();

  const PREFIX = "local/";
  let open = $state(false);
  let cursor = $state(0);
  let error = $state("");
  let now = $state(Date.now());
  let root: HTMLDivElement;

  const current = $derived(option.currentValue);
  const currentLocal = $derived(current.startsWith(PREFIX) ? current.slice(PREFIX.length) : null);
  const models = $derived(local.state?.models ?? []);
  // Anything the agent offers that isn't a local model (e.g. a Pi cloud login).
  const others = $derived(option.options.filter((o) => !o.value.startsWith(PREFIX) && !o.value.startsWith("llama.cpp/")));
  const rows = $derived([
    ...models.map((m) => ({ kind: "local" as const, id: m.id, value: PREFIX + m.id, m })),
    ...others.map((o) => ({ kind: "other" as const, id: o.value, value: o.value, name: o.name })),
  ]);
  const loadedHere = $derived(models.find((m) => m.id === currentLocal)?.status ?? "unloaded");
  const label = $derived(currentLocal ?? current.replace(/^[\w.-]+\//, ""));

  onMount(() => {
    local.start();
    const t = setInterval(() => (now = Date.now()), 250);
    return () => clearInterval(t);
  });

  function toggle() {
    if (disabled) return;
    open = !open;
    error = "";
    if (open) {
      local.refresh();
      cursor = Math.max(0, rows.findIndex((r) => r.value === current));
    }
  }

  async function choose(row: (typeof rows)[number]) {
    error = "";
    try {
      if (row.kind === "local" && row.m.status !== "loaded") await local.load(row.id);
      if (row.value !== current) await api.setConfig(threadId, option.id, row.value);
      open = false;
    } catch (e) {
      error = String(e);
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      open = false;
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      cursor = (cursor + 1) % rows.length;
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      cursor = (cursor - 1 + rows.length) % rows.length;
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (rows[cursor]) choose(rows[cursor]);
    }
  }

  function onpointerdown(e: PointerEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window {onkeydown} {onpointerdown} />

<div class="picker" bind:this={root}>
  <button class="trigger" class:open {disabled} onclick={toggle} title="Change model">
    <span class="k">model:</span>
    <span class="v">{label}</span>
    {#if currentLocal && loadedHere !== "loaded" && !local.loading}<span class="cold" title="Not in VRAM; loads on first message">○</span>{/if}
    <span class="chev">▴</span>
  </button>

  {#if open}
    <div class="pop" role="listbox">
      <div class="head">
        <span>local models</span>
        <span class="dir">{(local.state?.settings.model_dirs ?? []).map(folderLabel).join(" · ")}</span>
      </div>
      {#each local.state?.missing ?? [] as m}
        <div class="missing">{folderLabel(m)} isn't mounted; its models are hidden.</div>
      {/each}
      {#if models.length === 0}
        <div class="empty">
          No models found. Put <code>.gguf</code> files (or folders of them) in a model folder, or add a folder in
          settings.
        </div>
      {/if}
      {#each rows as r, i (r.value)}
        {@const isCurrent = r.value === current}
        {@const isLoading = local.loading?.id === r.id}
        <button
          class="row"
          class:cursor={i === cursor}
          class:current={isCurrent}
          disabled={!!local.loading}
          onmouseenter={() => (cursor = i)}
          onclick={() => choose(r)}
          role="option"
          aria-selected={isCurrent}
        >
          <span class="mark">{isCurrent ? "›" : ""}</span>
          {#if r.kind === "local"}
            <span class="name">{r.id}</span>
            <span class="tags">
              {#if r.m.vision}<span class="tag">vision</span>{/if}
              {#if r.m.ctx}<span class="tag">{ctxLabel(r.m.ctx)} ctx</span>{/if}
              <span class="tag">{gb(r.m.size)}</span>
              <span class="tag where" title={r.m.folder}>{folderLabel(r.m.folder)}</span>
            </span>
            <span class="st">
              {#if isLoading}<Spinner /> {duration(now - local.loading!.since)}
              {:else if r.m.status === "loaded"}<span class="on">● in vram</span>
              {:else if r.m.status === "loading"}<Spinner />
              {/if}
            </span>
          {:else}
            <span class="name">{r.name}</span>
            <span class="tags"><span class="tag">{r.id.split("/")[0]}</span></span>
            <span class="st"></span>
          {/if}
        </button>
      {/each}
      {#if error || local.error}<div class="err">{error || local.error}</div>{/if}
      <div class="foot">
        <span>↑↓ ⏎ · loading swaps the model in VRAM</span>
        <button class="link" onclick={() => ((open = false), (app.view = { name: "settings" }))}>settings</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .picker {
    position: relative;
  }
  button {
    all: unset;
    box-sizing: border-box;
    cursor: pointer;
  }
  .trigger {
    display: flex;
    gap: 0.6ch;
    align-items: baseline;
    color: var(--dark-foreground);
  }
  .trigger:disabled {
    cursor: default;
    opacity: 0.6;
  }
  .v {
    color: var(--light-foreground);
  }
  .trigger:hover:not(:disabled) .v,
  .trigger.open .v {
    color: var(--accent);
  }
  .cold {
    color: var(--dark-foreground);
  }
  .chev {
    font-size: 9px;
  }
  .pop {
    position: absolute;
    bottom: calc(100% + 10px);
    left: -12px;
    width: min(560px, calc(100vw - 320px));
    background: var(--dark-background);
    border: 1px solid var(--accent);
    box-shadow: 0 6px 24px rgb(0 0 0 / 0.35);
    z-index: 10;
    font-size: 12px;
  }
  .head,
  .foot {
    display: flex;
    justify-content: space-between;
    gap: 2ch;
    padding: 6px 12px;
    color: var(--dark-foreground);
  }
  .head {
    border-bottom: 1px solid var(--muted);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.12em;
  }
  .dir {
    text-transform: none;
    letter-spacing: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .foot {
    border-top: 1px solid var(--muted);
    font-size: 11px;
  }
  .link {
    color: var(--light-foreground);
  }
  .link:hover {
    color: var(--accent);
  }
  .row {
    display: flex;
    align-items: baseline;
    gap: 1ch;
    width: 100%;
    padding: 6px 12px 6px 6px;
  }
  .row.cursor {
    background: var(--selection);
  }
  .row:disabled {
    cursor: progress;
  }
  .mark {
    width: 1ch;
    color: var(--accent);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--foreground);
  }
  .row.current .name {
    color: var(--accent);
  }
  .tags {
    display: flex;
    gap: 1ch;
  }
  .tag {
    color: var(--dark-foreground);
  }
  .st {
    width: 10ch;
    text-align: right;
    color: var(--light-foreground);
  }
  .on {
    color: var(--green);
  }
  .where {
    color: var(--light-foreground);
    opacity: 0.7;
    min-width: 10ch;
    text-align: right;
  }
  .missing {
    padding: 5px 12px;
    color: var(--yellow);
    border-bottom: 1px solid var(--muted);
  }
  .empty {
    padding: 10px 12px;
    color: var(--light-foreground);
  }
  code {
    color: var(--accent);
  }
  .err {
    padding: 6px 12px;
    color: var(--red);
    border-top: 1px solid var(--muted);
    white-space: pre-wrap;
  }
</style>
