<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Status } from "./api";

  let statuses = $state<Status[]>([]);
  let loaded = $state(false);
  let busy = $state<Record<string, boolean>>({});
  let errors = $state<Record<string, string>>({});
  /** Stop needs a second click; stopping a model mid-session is costly. */
  let confirmStop = $state<string | null>(null);

  const kindLabel = { "llama-server": "llama.cpp", "lm-studio": "LM Studio", ollama: "Ollama" };

  async function refresh() {
    statuses = await api.status();
    loaded = true;
  }

  async function run(id: string, action: () => Promise<unknown>) {
    busy[id] = true;
    delete errors[id];
    try {
      await action();
    } catch (e) {
      errors[id] = String(e);
    } finally {
      busy[id] = false;
      confirmStop = null;
      await refresh();
    }
  }

  function stop(id: string) {
    if (confirmStop !== id) {
      confirmStop = id;
      return;
    }
    run(id, () => api.stop(id));
  }

  onMount(() => {
    refresh();
    const t = setInterval(refresh, 2000);
    return () => clearInterval(t);
  });
</script>

<section class="models">
<div class="k">local models</div>
<h1>Model servers</h1>
<p class="lede">Start, stop and check the servers your local agents talk to. Add your own in <code>~/.config/smithy/profiles.toml</code>.</p>

{#if loaded && statuses.length === 0}
  <p class="empty">
    No model servers found.
  </p>
{/if}

<div class="cards">
  {#each statuses as s (s.profile.id)}
    {@const id = s.profile.id}
    <section class="card">
      <header>
        <span class="dot {s.state}"></span>
        <h2>{s.profile.name}</h2>
        <span class="kind">{kindLabel[s.profile.kind]}</span>
        <span class="state">{s.state}</span>
      </header>

      <dl>
        <dt>endpoint</dt>
        <dd>{s.profile.host}:{s.profile.port}{s.pid ? ` · pid ${s.pid}` : ""}</dd>
        {#if s.models.length}
          <dt>{s.models.length > 1 ? "models" : "model"}</dt>
          <dd>{s.models.join(", ")}</dd>
        {/if}
        {#if s.n_ctx}
          <dt>context</dt>
          <dd>{s.n_ctx.toLocaleString()} tokens</dd>
        {/if}
        {#if s.ftype}
          <dt>kernels</dt>
          <dd>
            {s.ftype}
            {#if s.ftype_ok === true}
              <span class="ok" title="Matches expected {s.profile.expect_ftype}">✓ verified</span>
            {:else if s.ftype_ok === false}
              <span class="bad" title="Expected {s.profile.expect_ftype}">✗ expected {s.profile.expect_ftype}</span>
            {/if}
          </dd>
        {/if}
        {#if s.profile.vram_mib}
          <dt>vram</dt>
          <dd>~{(s.profile.vram_mib / 1024).toFixed(1)} GiB when loaded</dd>
        {/if}
      </dl>

      {#if errors[id]}
        <div class="error">
          {errors[id]}
          {#if errors[id].startsWith("VRAM")}
            <button class="danger" onclick={() => run(id, () => api.start(id, true))}>Start anyway</button>
          {/if}
        </div>
      {/if}

      <footer>
        {#if s.state === "stopped"}
          <button class="primary" disabled={!s.can_start || busy[id]} onclick={() => run(id, () => api.start(id))}>
            {busy[id] ? "Starting…" : "Start"}
          </button>
          {#if !s.can_start}<span class="hint">managed outside Smithy</span>{/if}
        {:else}
          <button class="danger" disabled={!s.can_stop || busy[id]} onclick={() => stop(id)}>
            {confirmStop === id ? "Confirm stop" : "Stop"}
          </button>
        {/if}
        {#if s.log}<span class="hint" title={s.log}>log: {s.log.split("/").pop()}</span>{/if}
      </footer>
    </section>
  {/each}
</div>
</section>

<style>
  .models {
    padding: 48px;
    max-width: 900px;
  }
  .k {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--dark-foreground);
    margin-bottom: 6px;
  }
  h1 {
    font-size: 20px;
    margin: 0;
  }
  .lede,
  .empty {
    color: var(--dark-foreground);
    margin: 4px 0 24px;
  }
  .cards {
    border-top: 1px solid var(--muted);
  }
  .card {
    border-bottom: 1px solid var(--muted);
    padding: 16px 8px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  h2 {
    font-size: 14px;
    margin: 0;
    flex: 1;
  }
  .kind,
  .state {
    font-size: 11px;
    color: var(--dark-foreground);
  }
  .dot {
    width: 7px;
    height: 7px;
    background: var(--muted);
  }
  .dot.running {
    background: var(--green);
  }
  .dot.starting {
    background: var(--yellow);
    animation: pulse 1s infinite alternate;
  }
  @keyframes pulse {
    to {
      opacity: 0.3;
    }
  }
  dl {
    display: grid;
    grid-template-columns: 80px 1fr;
    gap: 4px 12px;
    margin: 0;
  }
  dt {
    color: var(--dark-foreground);
  }
  dd {
    margin: 0;
    word-break: break-all;
  }
  .ok {
    color: var(--green);
    margin-left: 6px;
  }
  .bad {
    color: var(--red);
    margin-left: 6px;
  }
  .error {
    color: var(--red);
    border-left: 2px solid var(--red);
    padding-left: 8px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
  }
  footer {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .hint {
    color: var(--dark-foreground);
    font-size: 11px;
  }
  code {
    color: var(--accent);
  }
</style>
