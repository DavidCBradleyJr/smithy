<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Vram } from "./api";

  let vram = $state<Vram | null>(null);
  const pct = $derived(vram ? (vram.used_mib / vram.total_mib) * 100 : 0);
  const gib = (mib: number) => (mib / 1024).toFixed(1);

  onMount(() => {
    const tick = async () => (vram = await api.vram());
    tick();
    const t = setInterval(tick, 3000);
    return () => clearInterval(t);
  });
</script>

{#if vram}
  <div class="vram" title={vram.name}>
    <div class="label">
      <span>VRAM</span>
      <span>{gib(vram.used_mib)} / {gib(vram.total_mib)} GiB</span>
    </div>
    <div class="bar"><div class="fill" class:hot={pct > 90} style:width="{pct}%"></div></div>
  </div>
{/if}

<style>
  .vram {
    font-size: 11px;
    color: var(--dark-foreground);
  }
  .label {
    display: flex;
    justify-content: space-between;
    margin-bottom: 4px;
  }
  .bar {
    height: 4px;
    background: var(--muted);
  }
  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.4s;
  }
  .fill.hot {
    background: var(--red);
  }
</style>
