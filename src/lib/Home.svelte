<script lang="ts">
  import { open as pickFolder } from "@tauri-apps/plugin-dialog";
  import { app } from "./app.svelte";

  async function add() {
    const dir = await pickFolder({ directory: true, title: "Add project folder" });
    if (typeof dir === "string") await app.addProject(dir);
  }
</script>

<section class="home">
  <pre class="mark">
 ┌─┐┌┬┐┬┌┬┐┬ ┬┬ ┬
 └─┐│││ │ ├─┤└┬┘
 └─┘┴ ┴┴ ┴ ┴ ┴ ┴ </pre>
  {#if app.projects.length === 0}
    <p>Add a project folder, then start a thread with an agent.</p>
    <p class="dim">Local models run through Pi. Subscriptions run through their own agents.</p>
    <button class="primary" onclick={add}>add project…</button>
  {:else}
    <p class="dim">Pick a thread on the left, or press <b>+</b> next to a project.</p>
  {/if}
</section>

<style>
  .home {
    padding: 72px 48px;
    max-width: 640px;
  }
  .mark {
    color: var(--accent);
    margin: 0 0 24px;
    line-height: 1.2;
  }
  p {
    margin: 0 0 6px;
  }
  .dim {
    color: var(--dark-foreground);
  }
  button {
    margin-top: 20px;
  }
</style>
