<script lang="ts">
  import { marked } from "marked";
  import DOMPurify from "dompurify";
  import { openUrl } from "@tauri-apps/plugin-opener";

  let { text }: { text: string } = $props();

  const html = $derived(DOMPurify.sanitize(marked.parse(text, { async: false, gfm: true, breaks: false }) as string));

  // Links leave the app instead of navigating the webview.
  function onclick(e: MouseEvent) {
    const a = (e.target as HTMLElement).closest("a");
    if (!a?.href) return;
    e.preventDefault();
    openUrl(a.href);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="md" {onclick}>{@html html}</div>

<style>
  .md {
    line-height: 1.65;
    overflow-wrap: anywhere;
  }
  .md :global(p) {
    margin: 0 0 0.8em;
  }
  .md :global(p:last-child) {
    margin-bottom: 0;
  }
  .md :global(h1),
  .md :global(h2),
  .md :global(h3),
  .md :global(h4) {
    font-size: 1em;
    font-weight: 700;
    margin: 1.2em 0 0.5em;
    color: var(--bright-foreground, var(--foreground));
  }
  .md :global(h1)::before {
    content: "# ";
    color: var(--accent);
  }
  .md :global(h2)::before {
    content: "## ";
    color: var(--accent);
  }
  .md :global(h3)::before {
    content: "### ";
    color: var(--accent);
  }
  .md :global(code) {
    background: var(--dark-background);
    padding: 0.1em 0.35em;
    color: var(--light-foreground);
  }
  .md :global(pre) {
    background: var(--darker-background);
    border-left: 2px solid var(--muted);
    padding: 10px 14px;
    overflow-x: auto;
    margin: 0 0 0.8em;
  }
  .md :global(pre code) {
    background: none;
    padding: 0;
    color: var(--foreground);
  }
  .md :global(ul),
  .md :global(ol) {
    margin: 0 0 0.8em;
    padding-left: 2ch;
  }
  .md :global(li) {
    margin: 0.15em 0;
  }
  .md :global(ul li::marker) {
    content: "- ";
    color: var(--dark-foreground);
  }
  .md :global(a) {
    color: var(--accent);
    text-decoration: underline;
    text-decoration-color: var(--muted);
    text-underline-offset: 3px;
  }
  .md :global(blockquote) {
    margin: 0 0 0.8em;
    padding-left: 12px;
    border-left: 2px solid var(--muted);
    color: var(--light-foreground);
  }
  .md :global(table) {
    border-collapse: collapse;
    margin: 0 0 0.8em;
  }
  .md :global(th),
  .md :global(td) {
    border: 1px solid var(--muted);
    padding: 3px 10px;
    text-align: left;
  }
  .md :global(hr) {
    border: 0;
    border-top: 1px solid var(--muted);
  }
  .md :global(strong) {
    color: var(--bright-foreground, var(--foreground));
  }
</style>
