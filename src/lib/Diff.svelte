<script lang="ts">
  import { diffLines } from "diff";
  import type { DiffBlock } from "./reduce";

  let { diff, root = "" }: { diff: DiffBlock; root?: string } = $props();

  interface Row {
    sign: " " | "+" | "-" | "…";
    oldNo: number | null;
    newNo: number | null;
    text: string;
  }

  /** Unified rows with 3 lines of context; long unchanged runs collapse to "…". */
  const rows = $derived.by(() => {
    const out: Row[] = [];
    let o = 1;
    let n = 1;
    const parts = diffLines(diff.oldText ?? "", diff.newText);
    parts.forEach((part, pi) => {
      const lines = part.value.replace(/\n$/, "").split("\n");
      if (part.added) for (const text of lines) out.push({ sign: "+", oldNo: null, newNo: n++, text });
      else if (part.removed) for (const text of lines) out.push({ sign: "-", oldNo: o++, newNo: null, text });
      else {
        const first = pi === 0;
        const lastPart = pi === parts.length - 1;
        const keepHead = first ? 0 : 3;
        const keepTail = lastPart ? 0 : 3;
        if (lines.length > keepHead + keepTail + 1) {
          lines.slice(0, keepHead).forEach((text) => out.push({ sign: " ", oldNo: o++, newNo: n++, text }));
          const skip = lines.length - keepHead - keepTail;
          out.push({ sign: "…", oldNo: null, newNo: null, text: `${skip} unchanged lines` });
          o += skip;
          n += skip;
          lines.slice(lines.length - keepTail).forEach((text) => out.push({ sign: " ", oldNo: o++, newNo: n++, text }));
        } else for (const text of lines) out.push({ sign: " ", oldNo: o++, newNo: n++, text });
      }
    });
    return out;
  });

  const stats = $derived({
    add: rows.filter((r) => r.sign === "+").length,
    del: rows.filter((r) => r.sign === "-").length,
  });
  const shortPath = $derived(root && diff.path.startsWith(root + "/") ? diff.path.slice(root.length + 1) : diff.path);
</script>

<div class="diff">
  <div class="head">
    <span class="path">{shortPath}</span>
    {#if diff.oldText == null}<span class="new">new file</span>{/if}
    <span class="add">+{stats.add}</span>
    <span class="del">−{stats.del}</span>
  </div>
  <div class="body">
    {#each rows as r}
      <div class="row {r.sign === '+' ? 'plus' : r.sign === '-' ? 'minus' : r.sign === '…' ? 'gap' : ''}">
        <span class="no">{r.oldNo ?? ""}</span>
        <span class="no">{r.newNo ?? ""}</span>
        <span class="sign">{r.sign === "…" ? "" : r.sign}</span>
        <span class="code">{r.text}</span>
      </div>
    {/each}
  </div>
</div>

<style>
  .diff {
    border: 1px solid var(--muted);
    background: var(--darker-background);
    font-size: 12px;
  }
  .head {
    display: flex;
    gap: 12px;
    padding: 4px 10px;
    border-bottom: 1px solid var(--muted);
    background: var(--dark-background);
  }
  .path {
    flex: 1;
    color: var(--light-foreground);
  }
  .new {
    color: var(--yellow);
  }
  .add {
    color: var(--green);
  }
  .del {
    color: var(--red);
  }
  .body {
    overflow-x: auto;
    max-height: 480px;
    overflow-y: auto;
  }
  .row {
    display: grid;
    grid-template-columns: 4ch 4ch 2ch 1fr;
    white-space: pre;
    line-height: 1.55;
  }
  .no {
    color: var(--dark-foreground);
    text-align: right;
    padding-right: 1ch;
    user-select: none;
    opacity: 0.7;
  }
  .sign {
    user-select: none;
    padding-left: 0.5ch;
  }
  .plus {
    background: color-mix(in srgb, var(--green) 12%, transparent);
  }
  .plus .sign,
  .plus .code {
    color: color-mix(in srgb, var(--green) 70%, var(--foreground));
  }
  .minus {
    background: color-mix(in srgb, var(--red) 12%, transparent);
  }
  .minus .sign,
  .minus .code {
    color: color-mix(in srgb, var(--red) 70%, var(--foreground));
  }
  .gap {
    color: var(--dark-foreground);
    background: var(--dark-background);
    font-style: italic;
  }
</style>
