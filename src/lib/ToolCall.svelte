<script lang="ts">
  import type { ToolItem } from "./reduce";
  import Diff from "./Diff.svelte";
  import Spinner from "./Spinner.svelte";
  import { duration } from "./format";

  let { tool, root, now }: { tool: ToolItem; root: string; now: number } = $props();

  // Edits start open so changes are never invisible; the rest start closed.
  let open = $state<boolean | null>(null);
  const expanded = $derived(open ?? (tool.diffs.length > 0 || tool.status === "failed"));

  const glyph: Record<string, string> = {
    read: "▤",
    edit: "✎",
    delete: "⌫",
    move: "⇄",
    search: "⌕",
    execute: "$",
    think: "∴",
    fetch: "⇣",
    switch_mode: "⇆",
    other: "•",
  };

  const label = $derived.by(() => {
    let t = tool.title || tool.toolKind;
    if (root) t = t.replaceAll(root + "/", "").replaceAll(root, ".");
    // "cd <root> && cmd" is noise once the root is stripped.
    return t.replace(/^cd \. && /, "");
  });
  const running = $derived(tool.status === "pending" || tool.status === "in_progress");
  const took = $derived(tool.ended ? tool.ended - tool.started : running ? now - tool.started : null);
  const hasBody = $derived(tool.diffs.length > 0 || tool.output || tool.text.some(Boolean) || tool.rawInput !== undefined);
  const outputLines = $derived(tool.output.replace(/\n$/, "").split("\n"));
</script>

<div class="tool {tool.status}">
  <button class="line" onclick={() => (open = !expanded)} disabled={!hasBody} aria-expanded={expanded}>
    <span class="glyph">{glyph[tool.toolKind] ?? glyph.other}</span>
    <span class="title">{label}</span>
    {#if tool.exitCode != null && tool.exitCode !== 0}<span class="exit">exit {tool.exitCode}</span>{/if}
    <span class="meta">
      {#if took != null && took > 400}{duration(took)}{/if}
    </span>
    <span class="status">
      {#if running}<Spinner />{:else if tool.status === "completed"}✓{:else}✗{/if}
    </span>
  </button>

  {#if expanded && hasBody}
    <div class="body">
      {#each tool.diffs as d}<Diff diff={d} {root} />{/each}
      {#if tool.output}
        <pre class="out">{outputLines.length > 400 ? outputLines.slice(-400).join("\n") : tool.output}</pre>
      {/if}
      {#each tool.text.filter(Boolean) as t}<pre class="out">{t}</pre>{/each}
      {#if !tool.diffs.length && !tool.output && tool.rawInput !== undefined}
        <pre class="out raw">{JSON.stringify(tool.rawInput, null, 2)}</pre>
      {/if}
    </div>
  {/if}
</div>

<style>
  .tool {
    margin: 2px 0;
  }
  .line {
    all: unset;
    box-sizing: border-box;
    display: flex;
    align-items: baseline;
    gap: 1ch;
    width: 100%;
    padding: 2px 6px;
    cursor: pointer;
    color: var(--light-foreground);
  }
  .line:disabled {
    cursor: default;
  }
  .line:hover:not(:disabled) {
    background: var(--dark-background);
  }
  .line:focus-visible {
    outline: 1px solid var(--accent);
  }
  .glyph {
    width: 2ch;
    text-align: center;
    color: var(--accent);
    flex: none;
  }
  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta,
  .exit {
    color: var(--dark-foreground);
    font-size: 11px;
    flex: none;
  }
  .exit {
    color: var(--red);
  }
  .status {
    width: 2ch;
    text-align: center;
    flex: none;
    color: var(--green);
  }
  .failed .status,
  .failed .glyph {
    color: var(--red);
  }
  .pending .line,
  .in_progress .line {
    color: var(--foreground);
  }
  .body {
    margin: 4px 0 8px 3ch;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .out {
    margin: 0;
    padding: 8px 12px;
    background: var(--darker-background);
    border-left: 2px solid var(--muted);
    font-size: 12px;
    line-height: 1.5;
    max-height: 320px;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-all;
    color: var(--light-foreground);
  }
  .raw {
    color: var(--dark-foreground);
  }
</style>
