<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";

  /** Minimize, maximize and close for the undecorated Linux window. Close
   *  goes through CloseRequested, so it hides to the tray like before. */
  const appWindow = getCurrentWindow();
  let maximized = $state(false);

  onMount(() => {
    const sync = () => appWindow.isMaximized().then((m) => (maximized = m)).catch(() => {});
    sync();
    const unlisten = appWindow.onResized(sync);
    return () => {
      unlisten.then((stop) => stop()).catch(() => {});
    };
  });
</script>

<div class="controls">
  <button aria-label="Minimize" title="Minimize" onclick={() => appWindow.minimize()}>
    <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><path d="M3 7h8" /></svg>
  </button>
  <button
    aria-label={maximized ? "Restore" : "Maximize"}
    title={maximized ? "Restore" : "Maximize"}
    onclick={() => appWindow.toggleMaximize()}
  >
    {#if maximized}
      <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><rect x="3" y="5" width="6" height="6" /><path d="M5 5V3h6v6H9" /></svg>
    {:else}
      <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><rect x="3" y="3" width="8" height="8" /></svg>
    {/if}
  </button>
  <button class="close" aria-label="Close" title="Close (keeps running in the tray)" onclick={() => appWindow.close()}>
    <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><path d="M3.5 3.5l7 7M10.5 3.5l-7 7" /></svg>
  </button>
</div>

<style>
  .controls {
    display: flex;
    gap: 6px;
  }

  button {
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: 50%;
    background: var(--surface-2);
    color: var(--text);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
  }

  button:hover {
    background: var(--border);
  }

  button.close:hover {
    background: var(--danger);
    color: #ffffff;
  }

  svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
    stroke-linecap: round;
  }
</style>
