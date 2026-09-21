<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";
  import { api } from "./api";
  import { app } from "./app.svelte";

  let { id, cwd }: { id: string; cwd: string } = $props();

  let host: HTMLDivElement;
  let term: Terminal;
  let exited = $state(false);

  /** xterm colors from the live Omarchy theme (CSS variables). */
  function theme() {
    const css = getComputedStyle(document.documentElement);
    const v = (name: string) => css.getPropertyValue(`--${name}`).trim() || undefined;
    return {
      background: v("darker-background"),
      foreground: v("foreground"),
      cursor: v("accent"),
      selectionBackground: v("selection"),
      black: v("dark-background"),
      red: v("red"),
      green: v("green"),
      yellow: v("yellow"),
      blue: v("blue"),
      magenta: v("magenta"),
      cyan: v("cyan"),
      white: v("light-foreground"),
      brightBlack: v("dark-foreground"),
      brightRed: v("bright-red") ?? v("red"),
      brightGreen: v("bright-green") ?? v("green"),
      brightYellow: v("bright-yellow") ?? v("yellow"),
      brightBlue: v("bright-blue") ?? v("blue"),
      brightMagenta: v("bright-magenta") ?? v("magenta"),
      brightCyan: v("bright-cyan") ?? v("cyan"),
      brightWhite: v("bright-foreground") ?? v("foreground"),
    };
  }

  onMount(() => {
    const font = getComputedStyle(document.documentElement).getPropertyValue("--font").trim();
    term = new Terminal({ fontFamily: font, fontSize: 13, theme: theme(), cursorBlink: true, scrollback: 5000 });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);
    fit.fit();

    let alive = true;
    const unlisten: Promise<() => void>[] = [
      listen<{ id: string; data: string }>("pty-data", (e) => e.payload.id === id && term.write(e.payload.data)),
      listen<{ id: string }>("pty-exit", (e) => {
        if (e.payload.id === id) exited = true;
      }),
      listen("theme-changed", () => (term.options.theme = theme())),
    ];

    api.ptyOpen(id, cwd, term.cols, term.rows).then((scrollback) => {
      if (alive && scrollback) term.write(scrollback);
      term.focus();
    });
    const input = term.onData((d) => api.ptyWrite(id, d).catch(() => (exited = true)));

    const ro = new ResizeObserver(() => {
      if (!host.clientWidth) return;
      fit.fit();
      api.ptyResize(id, term.cols, term.rows).catch(() => {});
    });
    ro.observe(host);

    return () => {
      alive = false;
      ro.disconnect();
      input.dispose();
      unlisten.forEach((u) => u.then((f) => f()));
      term.dispose();
      // The shell keeps running; reopening reattaches with scrollback.
    };
  });

  async function restart() {
    await api.ptyClose(id);
    term.reset();
    await api.ptyOpen(id, cwd, term.cols, term.rows);
    exited = false;
    term.focus();
  }
</script>

<div class="term">
  <div class="bar">
    <span class="k">terminal</span>
    <span class="cwd" title={cwd}>{cwd.replace(/^\/home\/[^/]+/, "~")}</span>
    <span class="grow"></span>
    {#if exited}<button onclick={restart}>shell exited · restart</button>{/if}
    <button class="close" title="Close (Ctrl+`). The shell keeps running." onclick={() => (app.panels.terminal = false)}>×</button>
  </div>
  <div class="host" bind:this={host}></div>
</div>

<style>
  .term {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    border-top: 1px solid var(--muted);
    background: var(--darker-background);
  }
  .bar {
    display: flex;
    align-items: baseline;
    gap: 1.5ch;
    padding: 4px 14px;
    font-size: 11px;
    color: var(--dark-foreground);
  }
  .k {
    text-transform: uppercase;
    letter-spacing: 0.12em;
    font-size: 10px;
  }
  .grow {
    flex: 1;
  }
  button {
    all: unset;
    cursor: pointer;
    color: var(--light-foreground);
  }
  button:hover {
    color: var(--accent);
  }
  .close:hover {
    color: var(--red);
  }
  .host {
    flex: 1;
    min-height: 0;
    padding: 2px 0 0 10px;
  }
  .host :global(.xterm-viewport) {
    background: transparent !important;
  }
</style>
