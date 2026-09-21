<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import { syncTheme } from "$lib/theme";

  let { children } = $props();

  onMount(() => {
    syncTheme();
  });
</script>

{@render children()}
