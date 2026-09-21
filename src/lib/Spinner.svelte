<script lang="ts">
  import { onMount } from "svelte";

  const frames = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏";
  let i = $state(0);
  onMount(() => {
    const t = setInterval(() => (i = (i + 1) % frames.length), 80);
    return () => clearInterval(t);
  });
</script>

<span class="spin">{frames[i]}</span>

<style>
  .spin {
    color: var(--yellow);
    display: inline-block;
    width: 1ch;
  }
</style>
