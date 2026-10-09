<script lang="ts">
  import type { Settings } from "./run";

  interface Props {
    settings: Settings;
    onchange: (patch: Partial<Settings>) => void;
    onclose: () => void;
  }

  let { settings, onchange, onclose }: Props = $props();
</script>

<section>
  <header>
    <h2>Settings</h2>
    <button onclick={onclose}>Close</button>
  </header>
  <div class="field">
    <label for="remind">Remind me when the last update is older than</label>
    <div class="inline">
      <input
        id="remind"
        type="number"
        min="0"
        max="365"
        value={settings.remindAfterDays}
        onchange={(e) => {
          const days = Math.max(0, Math.min(365, Math.round(Number(e.currentTarget.value) || 0)));
          e.currentTarget.value = String(days);
          onchange({ remindAfterDays: days });
        }}
      />
      days
    </div>
    <p class="hint">Checked hourly while the app runs in the tray. Dry runs don't count.</p>
  </div>
  <div class="field">
    <label class="check">
      <input
        type="checkbox"
        checked={settings.autostart}
        onchange={(e) => onchange({ autostart: e.currentTarget.checked })}
      />
      Start at login (in the tray)
    </label>
    {#if import.meta.env.DEV}
      <p class="hint">Dev build: saved, but the login item is only registered by release builds.</p>
    {/if}
  </div>
  <p class="hint">
    The --cleanup and -y toggles in the toolbar are saved too, and are used by Run now in the tray.
  </p>
</section>

<style>
  section {
    position: absolute;
    inset: 0;
    z-index: 10;
    overflow-y: auto;
    background: #1e1e1e;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px;
    border-bottom: 1px solid #3a3a3a;
  }

  h2 {
    margin: 0;
    font-size: 15px;
  }

  button {
    padding: 4px 14px;
    border-radius: 6px;
    border: 1px solid #555;
    background: #3a3a3a;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .field,
  section > .hint {
    padding: 12px 16px 0;
    max-width: 520px;
  }

  .inline {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 6px;
  }

  input[type="number"] {
    width: 64px;
    padding: 4px 6px;
    border-radius: 6px;
    border: 1px solid #555;
    background: #2a2a2a;
    color: inherit;
    font: inherit;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .hint {
    color: #888;
    font-size: 12px;
    margin: 6px 0 0;
  }
</style>
