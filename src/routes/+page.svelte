<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "$lib/app.svelte";
  import Rail from "$lib/Rail.svelte";
  import ThreadPane from "$lib/ThreadPane.svelte";
  import NewThread from "$lib/NewThread.svelte";
  import Models from "$lib/Models.svelte";
  import Home from "$lib/Home.svelte";
  import Settings from "$lib/Settings.svelte";

  let ready = $state(false);
  onMount(async () => {
    await app.init();
    ready = true;
  });

  // Ctrl+N: new thread in the current thread's project.
  function onkeydown(e: KeyboardEvent) {
    if (e.ctrlKey && e.key === "n") {
      e.preventDefault();
      const project = app.current?.project ?? app.projects[0]?.path;
      if (project) app.view = { name: "new", project };
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="shell">
  <Rail />
  <main>
    {#if !ready}
      <!-- loading state is instant enough not to need chrome -->
    {:else if app.view.name === "thread" && app.current}
      {#key app.current.id}<ThreadPane thread={app.current} />{/key}
    {:else if app.view.name === "new"}
      {#key app.view.project}<NewThread project={app.view.project} />{/key}
    {:else if app.view.name === "models"}
      <Models />
    {:else if app.view.name === "settings"}
      <Settings />
    {:else}
      <Home />
    {/if}
  </main>
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: 260px 1fr;
    height: 100vh;
  }
  main {
    min-width: 0;
    min-height: 0;
    overflow: auto;
  }
</style>
