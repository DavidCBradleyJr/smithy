<script lang="ts">
  import { onMount } from "svelte";
  import { local, ctxLabel } from "./local.svelte";
  import Spinner from "./Spinner.svelte";

  // Sidebar summary: which local model holds the GPU right now.
  onMount(() => local.start());

  const loaded = $derived(local.state?.loaded ?? null);
  const loadingId = $derived(local.loading?.id ?? local.state?.models.find((m) => m.status === "loading")?.id ?? null);
</script>

<div class="lm">
  <div class="label">in vram</div>
  {#if loadingId}
    <div class="row"><Spinner /><span class="name">{loadingId}</span></div>
  {:else if loaded}
    <div class="row">
      <span class="dot">●</span>
      <span class="name">{loaded.id}</span>
      {#if loaded.n_ctx}<span class="ctx">{ctxLabel(loaded.n_ctx)}</span>{/if}
    </div>
  {:else}
    <div class="row none">{local.state?.router ? "no model loaded" : "local server off"}</div>
  {/if}
</div>

<style>
  .lm {
    color: var(--light-foreground);
  }
  .label {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--dark-foreground);
    margin-bottom: 3px;
  }
  .row {
    display: flex;
    gap: 1ch;
    align-items: baseline;
  }
  .dot {
    color: var(--green);
  }
  .name {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ctx,
  .none {
    color: var(--dark-foreground);
  }
</style>
