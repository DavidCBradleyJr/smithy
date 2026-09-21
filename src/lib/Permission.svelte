<script lang="ts">
  import { api } from "./api";
  import type { PermissionOption } from "./reduce";

  let {
    key,
    title,
    options,
    chosen,
    active,
  }: { key: string; title: string; options: PermissionOption[]; chosen: string | null | undefined; active: boolean } =
    $props();

  let error = $state("");
  const pending = $derived(chosen === undefined);
  const chosenName = $derived(options.find((o) => o.optionId === chosen)?.name ?? (chosen === null ? "Cancelled" : ""));

  async function pick(optionId: string | null) {
    try {
      await api.respond(key, optionId);
    } catch (e) {
      error = String(e);
    }
  }

  // Number keys answer the newest pending request.
  function onkeydown(e: KeyboardEvent) {
    if (!pending || !active || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLInputElement) return;
    const n = Number(e.key);
    if (n >= 1 && n <= options.length) {
      e.preventDefault();
      pick(options[n - 1].optionId);
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="perm" class:pending>
  <div class="q">
    <span class="tag">{pending ? "approve?" : "approved"}</span>
    <span class="what">{title}</span>
  </div>
  {#if pending}
    <div class="opts">
      {#each options as o, i}
        <button class:reject={o.kind.startsWith("reject")} onclick={() => pick(o.optionId)}>
          <kbd>{i + 1}</kbd>
          {o.name}
        </button>
      {/each}
    </div>
  {:else}
    <div class="answer">→ {chosenName}</div>
  {/if}
  {#if error}<div class="err">{error}</div>{/if}
</div>

<style>
  .perm {
    margin: 6px 0;
    padding: 8px 12px;
    border-left: 2px solid var(--muted);
    background: var(--dark-background);
  }
  .perm.pending {
    border-left-color: var(--yellow);
  }
  .q {
    display: flex;
    gap: 1ch;
    align-items: baseline;
  }
  .tag {
    color: var(--yellow);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .perm:not(.pending) .tag {
    color: var(--dark-foreground);
  }
  .what {
    word-break: break-all;
  }
  .opts {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 8px;
  }
  .reject:hover {
    border-color: var(--red) !important;
    color: var(--red) !important;
  }
  kbd {
    font: inherit;
    color: var(--dark-foreground);
    margin-right: 0.5ch;
  }
  .answer {
    color: var(--dark-foreground);
    margin-top: 2px;
  }
  .err {
    color: var(--red);
    margin-top: 6px;
  }
</style>
