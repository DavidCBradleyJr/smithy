<script lang="ts">
  import { api } from "./api";
  import type { ConfigOption } from "./reduce";
  import ModelPicker from "./ModelPicker.svelte";
  import { local } from "./local.svelte";

  let {
    threadId,
    config,
    busy,
    onsend,
  }: { threadId: string; config: ConfigOption[]; busy: boolean; onsend: (text: string) => void } = $props();

  let text = $state("");
  let box: HTMLTextAreaElement;
  let setting = $state<string | null>(null);
  let configError = $state("");

  // Drafts survive switching threads.
  const drafts: Record<string, string> = (globalThis as any).__smithyDrafts ??= {};
  let loadedFor = "";
  $effect(() => {
    if (threadId !== loadedFor) {
      drafts[loadedFor] = text;
      loadedFor = threadId;
      text = drafts[threadId] ?? "";
      queueMicrotask(() => box?.focus());
    }
  });

  $effect(() => {
    text;
    if (!box) return;
    box.style.height = "auto";
    box.style.height = Math.min(box.scrollHeight, 320) + "px";
  });

  function submit() {
    const t = text.trim();
    if (!t || busy) return;
    onsend(t);
    text = "";
    drafts[threadId] = "";
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      submit();
    }
  }

  async function change(opt: ConfigOption, value: string) {
    setting = opt.id;
    configError = "";
    try {
      await api.setConfig(threadId, opt.id, value);
    } catch (e) {
      configError = String(e);
    } finally {
      setting = null;
    }
  }

  // A local model whose chat template only takes some reasoning efforts:
  // offer just those (plus off), so the menu can't pick one it rejects.
  const localModel = $derived.by(() => {
    const v = config.find((o) => o.id === "model")?.currentValue ?? "";
    return v.startsWith("local/") ? local.state?.models.find((m) => m.id === v.slice(6)) : undefined;
  });
  function choices(opt: ConfigOption) {
    const efforts = localModel?.efforts;
    if (opt.id !== "thought_level" || !efforts) return opt.options;
    return opt.options.filter((o) => o.value === "off" || efforts.includes(o.value) || o.value === opt.currentValue);
  }

  /** Strip the agent's redundant prefixes ("Thinking: high", "bonsai/Bonsai…"). */
  function short(opt: ConfigOption, name: string) {
    return name.replace(/^Thinking:\s*/i, "").replace(/^[\w.-]+\//, "");
  }
</script>

<div class="composer">
  <textarea
    bind:this={box}
    bind:value={text}
    {onkeydown}
    rows="1"
    placeholder={busy ? "Agent is working. Esc to stop." : "Ask, or describe a change…"}
    spellcheck="false"
  ></textarea>
  <div class="bar">
    {#each config as opt (opt.id)}
      {#if opt.id === "model" && opt.options.some((o) => o.value.startsWith("local/"))}
        <ModelPicker {threadId} option={opt} disabled={busy} />
      {:else}
      <label class="opt" class:setting={setting === opt.id} title={opt.name}>
        <span class="k">{(opt.category ?? opt.id).replace("thought_level", "thinking")}</span>
        <select value={opt.currentValue} disabled={busy} onchange={(e) => change(opt, e.currentTarget.value)}>
          {#each choices(opt) as o}
            <option value={o.value}>{short(opt, o.name)}</option>
          {/each}
        </select>
      </label>
      {/if}
    {/each}
    {#if configError}<span class="err">{configError}</span>{/if}
    <span class="grow"></span>
    {#if busy}
      <span class="hint">esc stop</span>
    {:else}
      <span class="hint">⏎ send · ⇧⏎ newline</span>
    {/if}
  </div>
</div>

<style>
  .composer {
    border: 1px solid var(--muted);
    background: var(--dark-background);
  }
  .composer:focus-within {
    border-color: var(--accent);
  }
  textarea {
    display: block;
    width: 100%;
    resize: none;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--foreground);
    font: inherit;
    line-height: 1.55;
    padding: 10px 12px 6px;
    min-height: 40px;
  }
  textarea::placeholder {
    color: var(--dark-foreground);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 4px 12px 6px;
    font-size: 11px;
  }
  .opt {
    display: flex;
    gap: 0.6ch;
    align-items: baseline;
    color: var(--dark-foreground);
  }
  .opt.setting {
    opacity: 0.5;
  }
  .k::after {
    content: ":";
  }
  select {
    font: inherit;
    color: var(--light-foreground);
    background: transparent;
    border: 0;
    padding: 0;
    cursor: pointer;
    appearance: none;
    outline: 0;
  }
  select:hover,
  select:focus-visible {
    color: var(--accent);
  }
  select option {
    background: var(--dark-background);
    color: var(--foreground);
  }
  .grow {
    flex: 1;
  }
  .hint {
    color: var(--dark-foreground);
  }
  .err {
    color: var(--red);
  }
</style>
