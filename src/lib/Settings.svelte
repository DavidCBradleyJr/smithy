<script lang="ts">
  import { onMount } from "svelte";
  import { open as pick } from "@tauri-apps/plugin-dialog";
  import { openPath } from "@tauri-apps/plugin-opener";
  import { api, type Settings } from "./api";
  import { local, gb, folderLabel } from "./local.svelte";

  let saved = $state<Settings | null>(null);
  let form = $state<Settings | null>(null);
  let message = $state("");
  let error = $state("");
  let restarting = $state(false);

  const dirty = $derived(!!form && !!saved && JSON.stringify(form) !== JSON.stringify(saved));
  let needsRestart = $state(false);

  onMount(async () => {
    local.start();
    saved = await api.settings();
    form = { ...saved, model_dirs: [...saved.model_dirs] };
  });

  async function addDir() {
    const d = await pick({ directory: true, title: "Add a models folder" });
    if (typeof d === "string" && form && !form.model_dirs.includes(d)) form.model_dirs = [...form.model_dirs, d];
  }

  function removeDir(i: number) {
    if (form) form.model_dirs = form.model_dirs.filter((_, j) => j !== i);
  }

  function moveUp(i: number) {
    if (!form || i === 0) return;
    const d = [...form.model_dirs];
    [d[i - 1], d[i]] = [d[i], d[i - 1]];
    form.model_dirs = d;
  }

  async function browseBin() {
    const f = await pick({ directory: false, title: "llama-server binary", defaultPath: form?.llama_server });
    if (typeof f === "string" && form) form.llama_server = f;
  }

  async function save() {
    if (!form) return;
    error = "";
    message = "";
    try {
      const before = saved;
      saved = await api.saveSettings({ ...form, port: Number(form.port) });
      form = { ...saved, model_dirs: [...saved.model_dirs] };
      // Folder changes are picked up live; the binary and port need a restart.
      needsRestart = !!local.state?.router && (before?.llama_server !== saved.llama_server || before?.port !== saved.port);
      message = "Saved.";
      await local.refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function restart() {
    restarting = true;
    error = "";
    try {
      const was = local.state?.loaded?.id;
      await api.stop("local").catch(() => {});
      await new Promise((r) => setTimeout(r, 800));
      if (was && local.state?.models.some((m) => m.id === was)) await local.load(was);
      else await api.start("local");
      needsRestart = false;
      message = "Local server restarted.";
    } catch (e) {
      error = String(e);
    } finally {
      restarting = false;
      await local.refresh();
    }
  }
</script>

<section class="settings">
  <div class="k">settings</div>
  <h1>Local models</h1>

  {#if form}
    <div class="field">
      <div class="label">model folders</div>
      <ul class="dirs">
        {#each form.model_dirs as d, i (d)}
          {@const missing = local.state?.missing.includes(d)}
          {@const count = local.state?.models.filter((m) => m.folder === d).length ?? 0}
          <li>
            <span class="name">{folderLabel(d)}</span>
            <span class="path" title={d}>{d}</span>
            <span class="count" class:warn={missing}>{missing ? "not mounted" : `${count} model${count === 1 ? "" : "s"}`}</span>
            <button class="mini" disabled={i === 0} title="Search this folder first" onclick={() => moveUp(i)}>↑</button>
            <button class="mini" disabled={form.model_dirs.length === 1} title="Remove folder (files are untouched)" onclick={() => removeDir(i)}>×</button>
          </li>
        {/each}
      </ul>
      <button onclick={addDir}>add folder…</button>
      <p class="help">
        A model is a <code>.gguf</code> file, or a folder of weights plus an optional <code>mmproj*.gguf</code> for
        vision. Drop one into any of these folders and it shows up in the model picker. Keep models you switch to
        often on a fast internal drive; folders on removable drives are skipped while unplugged. If two folders hold a
        model with the same name, the higher one wins.
      </p>
      {#if local.state}
        <ul class="found">
          {#each local.state.models as m}
            <li>
              <span>{m.id}</span>
              <span class="dim">{gb(m.size)}{m.vision ? " · vision" : ""} · {folderLabel(m.folder)}</span>
            </li>
          {:else}
            <li class="dim">no models found yet</li>
          {/each}
        </ul>
      {/if}
    </div>

    <div class="field">
      <label for="bin">llama-server</label>
      <div class="row">
        <input id="bin" bind:value={form.llama_server} spellcheck="false" />
        <button onclick={browseBin}>browse…</button>
      </div>
      <p class="help">
        One server runs every model. PrismML's fork is a superset of upstream llama.cpp, so it serves Bonsai's
        ternary weights and ordinary GGUFs alike.
      </p>
    </div>

    <div class="field">
      <label for="port">port</label>
      <div class="row"><input id="port" class="short" type="number" bind:value={form.port} /></div>
      <p class="help">Agents and other apps (Open WebUI) reach local models at <code>http://127.0.0.1:{form.port}/v1</code>.</p>
    </div>

    <div class="field">
      <label for="presets">per-model settings</label>
      <div class="row">
        <code id="presets" class="path">{local.state?.presets ?? "~/.config/smithy/models.ini"}</code>
        <button onclick={() => local.state && openPath(local.state.presets)}>open</button>
      </div>
      <p class="help">Context size, sampling and GPU flags per model, in llama-server's preset format.</p>
    </div>

    <div class="actions">
      <button class="primary" disabled={!dirty} onclick={save}>save</button>
      {#if needsRestart || local.state?.router}
        <button disabled={restarting} onclick={restart}>{restarting ? "restarting…" : "restart local server"}</button>
      {/if}
      {#if message}<span class="msg">{message}{needsRestart ? " Restart the local server to apply." : ""}</span>{/if}
    </div>
    {#if error}<div class="error">{error}</div>{/if}
  {/if}
</section>

<style>
  .settings {
    padding: 48px;
    max-width: 760px;
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
    margin: 0 0 28px;
  }
  .field {
    margin-bottom: 26px;
  }
  label,
  .label {
    display: block;
    color: var(--light-foreground);
    margin-bottom: 6px;
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  input {
    flex: 1;
    font: inherit;
    color: var(--foreground);
    background: var(--dark-background);
    border: 1px solid var(--muted);
    padding: 5px 10px;
    outline: 0;
  }
  input:focus {
    border-color: var(--accent);
  }
  input.short {
    flex: 0 0 10ch;
  }
  .path {
    flex: 1;
    padding: 5px 0;
  }
  .help {
    margin: 6px 0 0;
    color: var(--dark-foreground);
    font-size: 12px;
  }
  code {
    color: var(--accent);
  }
  .dirs {
    list-style: none;
    margin: 0 0 10px;
    padding: 0;
    border-top: 1px solid var(--muted);
  }
  .dirs li {
    display: flex;
    align-items: baseline;
    gap: 1.5ch;
    padding: 6px 0;
    border-bottom: 1px solid var(--muted);
  }
  .dirs .name {
    width: 14ch;
    flex: none;
    font-weight: 700;
  }
  .dirs .path {
    flex: 1;
    min-width: 0;
    color: var(--dark-foreground);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    color: var(--light-foreground);
    font-size: 12px;
  }
  .count.warn {
    color: var(--yellow);
  }
  .mini {
    border: 0;
    padding: 0 4px;
    color: var(--dark-foreground);
  }
  .found {
    list-style: none;
    margin: 10px 0 0;
    padding: 0;
    border-top: 1px solid var(--muted);
    font-size: 12px;
  }
  .found li {
    display: flex;
    justify-content: space-between;
    padding: 4px 0;
    border-bottom: 1px solid var(--muted);
  }
  .dim {
    color: var(--dark-foreground);
  }
  .actions {
    display: flex;
    gap: 12px;
    align-items: center;
  }
  .msg {
    color: var(--light-foreground);
  }
  .error {
    margin-top: 12px;
    color: var(--red);
  }
</style>
