<script lang="ts">
  import { api, workdir, type Changes, type FileChange, type FileDiff, type Thread } from "./api";
  import { app, reviewPrompt, type ReviewComment } from "./app.svelte";
  import Diff from "./Diff.svelte";
  import Spinner from "./Spinner.svelte";

  let { thread, busy }: { thread: Thread; busy: boolean } = $props();

  const cwd = $derived(workdir(thread));
  let changes = $state<Changes | null>(null);
  let open = $state<Record<string, boolean>>({});
  let diffs = $state<Record<string, FileDiff | "loading">>({});
  let message = $state("");
  let working = $state<"" | "commit" | "push" | "merge">("");
  let note = $state("");
  let error = $state("");

  const comments = $derived(app.comments[thread.id] ?? []);
  const totals = $derived(
    (changes?.files ?? []).reduce((t, f) => ({ add: t.add + (f.additions ?? 0), del: t.del + (f.deletions ?? 0) }), { add: 0, del: 0 }),
  );

  async function refresh() {
    const next = await api.changes(cwd);
    // Re-fetch open diffs whose numbers moved, so the panel follows the agent.
    const before = new Map((changes?.files ?? []).map((f) => [f.path, `${f.status}${f.additions}${f.deletions}`]));
    changes = next;
    for (const f of next.files) {
      if (open[f.path] && before.get(f.path) !== `${f.status}${f.additions}${f.deletions}`) loadDiff(f);
    }
  }

  async function loadDiff(f: FileChange) {
    if (!diffs[f.path]) diffs[f.path] = "loading";
    diffs[f.path] = await api.fileDiff(cwd, f.path, f.orig);
  }

  function toggle(f: FileChange) {
    open[f.path] = !open[f.path];
    if (open[f.path]) loadDiff(f);
  }

  // Poll while visible; faster while the agent is working.
  $effect(() => {
    cwd;
    refresh();
    const t = setInterval(refresh, busy ? 1500 : 4000);
    return () => clearInterval(t);
  });

  function addComment(c: ReviewComment) {
    app.comments[thread.id] = [...comments, c];
  }

  function removeComment(c: ReviewComment) {
    app.comments[thread.id] = comments.filter((x) => x !== c);
  }

  function sendComments() {
    if (!comments.length || busy) return;
    app.send(thread.id, reviewPrompt(comments));
    app.comments[thread.id] = [];
  }

  async function run(kind: typeof working, fn: () => Promise<string>, done: (r: string) => string) {
    working = kind;
    error = "";
    note = "";
    try {
      note = done(await fn());
    } catch (e) {
      error = String(e);
    } finally {
      working = "";
      await refresh();
    }
  }

  const commit = () =>
    run("commit", () => api.commit(cwd, message), (sha) => {
      message = "";
      return `committed ${sha}`;
    });
  const push = () => run("push", () => api.push(cwd), () => "pushed");
  const merge = () =>
    run("merge", () => api.merge(thread.project, thread.branch!), (into) => `merged ${thread.branch} into ${into}`);

  const statusName: Record<string, string> = { M: "modified", A: "added", D: "deleted", R: "renamed", "?": "new", U: "conflict" };

</script>

<aside class="review">
  <div class="top">
    <span class="k">changes</span>
    {#if changes?.repo}
      <span class="branch">{changes.branch ?? "detached"}</span>
      {#if changes.ahead}<span class="ab" title="Commits not pushed">↑{changes.ahead}</span>{/if}
      {#if changes.behind}<span class="ab" title="Commits to pull">↓{changes.behind}</span>{/if}
      <span class="grow"></span>
      {#if changes.files.length}<span class="tot"><span class="add">+{totals.add}</span> <span class="del">−{totals.del}</span></span>{/if}
    {/if}
    <button class="close" title="Close (Ctrl+Shift+G)" onclick={() => (app.panels.changes = false)}>×</button>
  </div>

  {#if !changes}
    <div class="empty"><Spinner /></div>
  {:else if !changes.repo}
    <div class="empty">This folder isn't a git repository, so there's nothing to review here.</div>
  {:else}
    <div class="files">
      {#each changes.files as f (f.path)}
        {@const d = diffs[f.path]}
        {@const n = comments.filter((c) => c.path === f.path).length}
        <div class="file">
          <button class="fline" class:open={open[f.path]} onclick={() => toggle(f)} title={statusName[f.status]}>
            <span class="chev">{open[f.path] ? "▾" : "▸"}</span>
            <span class="st st-{f.status === '?' ? 'n' : f.status}">{f.status === "?" ? "N" : f.status}</span>
            <span class="path">{f.orig ? `${f.orig} → ${f.path}` : f.path}</span>
            {#if n}<span class="cc" title="Comments">✎{n}</span>{/if}
            <span class="add">{f.additions != null ? `+${f.additions}` : ""}</span>
            <span class="del">{f.deletions ? `−${f.deletions}` : ""}</span>
          </button>
          {#if open[f.path]}
            {#if d === "loading" || !d}
              <div class="msg"><Spinner /></div>
            {:else if d.binary}
              <div class="msg">binary file</div>
            {:else if d.too_large}
              <div class="msg">too large to show</div>
            {:else}
              <Diff
                diff={{ path: f.path, oldText: d.old, newText: d.new ?? "" }}
                header={false}
                {comments}
                oncomment={addComment}
                onremove={removeComment}
              />
            {/if}
          {/if}
        </div>
      {:else}
        <div class="empty">No changes. The working tree matches the last commit.</div>
      {/each}
    </div>

    <div class="actions">
      {#if comments.length}
        <button class="primary send" disabled={busy} onclick={sendComments}>
          send {comments.length} comment{comments.length === 1 ? "" : "s"} to the agent
        </button>
      {:else if changes.files.length}
        <div class="hint">Hover a line and click + to leave a comment for the agent.</div>
      {/if}

      {#if changes.files.length}
        <textarea bind:value={message} rows="2" placeholder="Commit message" spellcheck="false"></textarea>
        <div class="row">
          <button disabled={!message.trim() || !!working || busy} onclick={commit}>
            {working === "commit" ? "committing…" : `commit all (${changes.files.length})`}
          </button>
        </div>
      {/if}
      <div class="row">
        <button disabled={!!working || (!changes.ahead && !!changes.upstream)} onclick={push}>
          {working === "push" ? "pushing…" : changes.upstream ? `push${changes.ahead ? ` ${changes.ahead}` : ""}` : "push (new branch)"}
        </button>
        {#if thread.branch}
          <button
            disabled={!!working || busy || changes.files.length > 0}
            title={changes.files.length ? "Commit first" : `Merge ${thread.branch} into the project's current branch`}
            onclick={merge}
          >
            {working === "merge" ? "merging…" : "merge into project"}
          </button>
        {/if}
      </div>
      {#if note}<div class="ok">{note}</div>{/if}
      {#if error}<pre class="err">{error}</pre>{/if}
    </div>
  {/if}
</aside>

<style>
  .review {
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
    border-left: 1px solid var(--muted);
    background: var(--background);
    font-size: 12px;
  }
  button {
    all: unset;
    box-sizing: border-box;
    cursor: pointer;
  }
  .top {
    display: flex;
    align-items: baseline;
    gap: 1.2ch;
    padding: 10px 14px;
    border-bottom: 1px solid var(--muted);
  }
  .k {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--dark-foreground);
  }
  .branch {
    color: var(--light-foreground);
  }
  .ab {
    color: var(--yellow);
  }
  .grow {
    flex: 1;
  }
  .close {
    color: var(--dark-foreground);
    padding: 0 2px;
  }
  .close:hover {
    color: var(--red);
  }
  .files {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
  }
  .file {
    border-bottom: 1px solid var(--muted);
  }
  .fline {
    display: flex;
    align-items: baseline;
    gap: 1ch;
    width: 100%;
    padding: 5px 14px 5px 8px;
  }
  .fline:hover,
  .fline.open {
    background: var(--dark-background);
  }
  .chev {
    color: var(--dark-foreground);
    width: 1ch;
  }
  .st {
    width: 1ch;
    font-weight: 700;
  }
  .st-M,
  .st-R {
    color: var(--yellow);
  }
  .st-A,
  .st-n {
    color: var(--green);
  }
  .st-D,
  .st-U {
    color: var(--red);
  }
  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }
  .cc {
    color: var(--accent);
  }
  .add {
    color: var(--green);
  }
  .del {
    color: var(--red);
  }
  .msg,
  .empty {
    padding: 12px 14px;
    color: var(--dark-foreground);
  }
  .actions {
    border-top: 1px solid var(--muted);
    padding: 10px 14px 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .actions button {
    border: 1px solid var(--muted);
    padding: 4px 12px;
    color: var(--foreground);
  }
  .actions button:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--accent);
  }
  .actions button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .actions .primary {
    border-color: var(--accent);
    color: var(--accent);
    text-align: center;
  }
  .row {
    display: flex;
    gap: 8px;
  }
  textarea {
    font: inherit;
    color: var(--foreground);
    background: var(--dark-background);
    border: 1px solid var(--muted);
    padding: 6px 8px;
    resize: vertical;
    outline: 0;
  }
  textarea:focus {
    border-color: var(--accent);
  }
  .hint {
    color: var(--dark-foreground);
  }
  .ok {
    color: var(--green);
  }
  .err {
    margin: 0;
    color: var(--red);
    white-space: pre-wrap;
    font: inherit;
  }
  .tot {
    white-space: nowrap;
  }
</style>
