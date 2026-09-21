<script lang="ts">
  import { onMount, tick } from "svelte";
  import { app } from "./app.svelte";
  import { api, type Thread } from "./api";
  import Markdown from "./Markdown.svelte";
  import ToolCall from "./ToolCall.svelte";
  import Permission from "./Permission.svelte";
  import Composer from "./Composer.svelte";
  import Spinner from "./Spinner.svelte";
  import { duration } from "./format";

  let { thread }: { thread: Thread } = $props();

  const view = $derived(app.viewOf(thread.id));
  const busy = $derived(!!app.inflight[thread.id]);
  const agent = $derived(app.agents.find((a) => a.id === thread.agent));
  const project = $derived(app.projects.find((p) => p.path === thread.project));
  const branch = $derived(app.branches[thread.project]);
  const lastPending = $derived(
    view.items.findLast((i) => i.kind === "permission" && i.chosen === undefined) as { key: string } | undefined,
  );
  const starting = $derived(view.items.length === 0 && !view.config.length && !view.items.some((i) => i.kind === "error"));

  let now = $state(Date.now());
  let turnStarted = $derived.by(() => {
    const u = view.items.findLast((i) => i.kind === "user");
    return u && u.kind === "user" ? u.at : now;
  });
  onMount(() => {
    const t = setInterval(() => (now = Date.now()), 250);
    return () => clearInterval(t);
  });

  // Follow the output while the user is at the bottom; stop if they scroll up.
  let scroller: HTMLDivElement;
  let pinned = true;
  function onscroll() {
    pinned = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 60;
  }
  $effect(() => {
    view.items.length;
    (view.items.at(-1) as any)?.text?.length;
    (view.items.at(-1) as any)?.output?.length;
    if (pinned) tick().then(() => scroller && (scroller.scrollTop = scroller.scrollHeight));
  });
  $effect(() => {
    thread.id;
    pinned = true;
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && busy) {
      e.preventDefault();
      api.cancel(thread.id);
    }
  }

  let openThoughts = $state<Record<number, boolean>>({});
</script>

<svelte:window {onkeydown} />

<section class="pane">
  <header>
    <div class="title">{thread.title || "New thread"}</div>
    <div class="where">
      <span>{project?.name ?? thread.project}</span>
      {#if branch}<span class="branch"> {branch}</span>{/if}
      <span class="sep">·</span>
      <span>{view.agentName ?? agent?.name ?? thread.agent}</span>
    </div>
  </header>

  <div class="scroll" bind:this={scroller} {onscroll}>
    <div class="log">
      {#if starting}
        <div class="starting"><Spinner /> starting {agent?.name ?? thread.agent}…</div>
      {/if}
      {#each view.items as item, i}
        {#if item.kind === "user"}
          <div class="user"><span class="caret">›</span><div class="utext">{item.text}</div></div>
        {:else if item.kind === "text"}
          <div class="text"><Markdown text={item.text} /></div>
        {:else if item.kind === "thought"}
          {@const running = item.ended == null && busy}
          <div class="thought">
            <button class="tline" onclick={() => (openThoughts[i] = !openThoughts[i])}>
              <span class="glyph">∴</span>
              {#if running}<span>thinking</span> <Spinner />{:else}<span
                  >thought{item.ended ? ` for ${duration(item.ended - item.started)}` : ""}</span
                >{/if}
            </button>
            {#if openThoughts[i] || running}<div class="ttext">{item.text}</div>{/if}
          </div>
        {:else if item.kind === "tool"}
          <ToolCall tool={item} root={thread.project} {now} />
        {:else if item.kind === "permission"}
          <Permission {...item} active={lastPending?.key === item.key} />
        {:else if item.kind === "plan"}
          <div class="plan">
            {#each item.entries as e}
              <div class="step {e.status}">
                <span class="box">{e.status === "completed" ? "[x]" : e.status === "in_progress" ? "[~]" : "[ ]"}</span>
                {e.content}
              </div>
            {/each}
          </div>
        {:else if item.kind === "stop"}
          <div class="stop">
            {item.reason === "end_turn" ? "done" : item.reason.replace("_", " ")}{item.took ? ` · ${duration(item.took)}` : ""}
          </div>
        {:else if item.kind === "error"}
          <div class="error">{item.message}</div>
        {:else if item.kind === "note"}
          <div class="note">{item.text}</div>
        {/if}
      {/each}
      {#if busy && !view.waiting}
        <div class="working"><Spinner /> working <span class="dim">{duration(now - turnStarted)}</span></div>
      {/if}
      {#if !starting && view.items.length === 0 && view.config.length}
        <div class="empty">
          <p>{agent?.name} is ready in <b>{project?.name ?? thread.project}</b>.</p>
          <p class="dim">It can read and edit files and run commands in this folder.</p>
        </div>
      {/if}
    </div>
  </div>

  <div class="dock">
    <Composer threadId={thread.id} config={view.config} {busy} onsend={(t) => app.send(thread.id, t)} />
  </div>
</section>

<style>
  .pane {
    display: grid;
    grid-template-rows: auto 1fr auto;
    height: 100%;
    min-width: 0;
  }
  header {
    display: flex;
    align-items: baseline;
    gap: 2ch;
    padding: 10px 24px;
    border-bottom: 1px solid var(--muted);
    min-width: 0;
  }
  .title {
    font-weight: 700;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .where {
    color: var(--dark-foreground);
    font-size: 12px;
    white-space: nowrap;
    margin-left: auto;
  }
  .branch {
    color: var(--light-foreground);
  }
  .sep {
    margin: 0 0.5ch;
  }
  .scroll {
    overflow-y: auto;
    min-height: 0;
  }
  .log {
    max-width: 900px;
    padding: 20px 24px 32px;
  }
  .user {
    display: flex;
    gap: 1ch;
    margin: 20px 0 12px;
    padding: 8px 12px;
    background: var(--lighter-background);
    border-left: 2px solid var(--accent);
  }
  .user:first-child {
    margin-top: 0;
  }
  .caret {
    color: var(--accent);
  }
  .utext {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .text {
    margin: 10px 0;
    padding-left: 3ch;
  }
  .thought {
    margin: 4px 0;
  }
  .tline {
    all: unset;
    cursor: pointer;
    display: flex;
    gap: 1ch;
    padding: 2px 6px;
    color: var(--dark-foreground);
  }
  .tline:hover {
    color: var(--light-foreground);
  }
  .glyph {
    width: 2ch;
    text-align: center;
  }
  .ttext {
    margin: 2px 0 8px 3ch;
    padding-left: 12px;
    border-left: 1px solid var(--muted);
    color: var(--dark-foreground);
    white-space: pre-wrap;
    font-size: 12px;
    max-height: 240px;
    overflow-y: auto;
  }
  .plan {
    margin: 8px 0 8px 3ch;
    font-size: 12px;
  }
  .step {
    color: var(--light-foreground);
  }
  .step.completed {
    color: var(--dark-foreground);
    text-decoration: line-through;
  }
  .step.in_progress .box {
    color: var(--yellow);
  }
  .box {
    color: var(--dark-foreground);
  }
  .stop {
    margin: 12px 0 0 3ch;
    color: var(--dark-foreground);
    font-size: 11px;
  }
  .error {
    margin: 10px 0;
    padding: 8px 12px;
    border-left: 2px solid var(--red);
    color: var(--red);
    background: var(--dark-background);
    white-space: pre-wrap;
  }
  .note {
    margin: 10px 0;
    color: var(--dark-foreground);
    font-style: italic;
  }
  .working,
  .starting {
    margin: 8px 0 0;
    padding-left: 6px;
    color: var(--light-foreground);
  }
  .dim {
    color: var(--dark-foreground);
  }
  .empty {
    color: var(--light-foreground);
  }
  .empty p {
    margin: 0 0 4px;
  }
  .dock {
    padding: 0 24px 16px;
    max-width: 948px;
  }
</style>
