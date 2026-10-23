<script lang="ts">
  import changelogSource from "../../CHANGELOG.md?raw";
  import type { Settings, UpdateInfo } from "./run";

  interface Props {
    settings: Settings;
    version: string;
    update: UpdateInfo | null;
    updateMessage: string;
    checkingUpdate: boolean;
    installing: boolean;
    oncheckupdate: () => void;
    oninstallupdate: () => void;
    onchange: (patch: Partial<Settings>) => void;
    onclose: () => void;
  }

  let {
    settings,
    version,
    update,
    updateMessage,
    checkingUpdate,
    installing,
    oncheckupdate,
    oninstallupdate,
    onchange,
    onclose,
  }: Props = $props();

  interface Release {
    title: string;
    groups: { name: string; items: string[] }[];
  }

  /** Minimal Keep a Changelog reader: `## release`, `### group`, `- item`. */
  function parseChangelog(source: string): Release[] {
    const releases: Release[] = [];
    for (const raw of source.split("\n")) {
      const line = raw.trim();
      if (line.startsWith("## ")) {
        releases.push({ title: line.slice(3), groups: [] });
      } else if (line.startsWith("### ") && releases.length) {
        releases.at(-1)!.groups.push({ name: line.slice(4), items: [] });
      } else if (line.startsWith("- ") && releases.length) {
        const release = releases.at(-1)!;
        if (!release.groups.length) release.groups.push({ name: "", items: [] });
        release.groups.at(-1)!.items.push(line.slice(2));
      }
    }
    return releases;
  }

  const releases = parseChangelog(changelogSource);
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
  <div class="field">
    <h3>About</h3>
    <p class="version">Version {version || "…"}</p>
    <div class="inline">
      <button onclick={oncheckupdate} disabled={checkingUpdate || installing}>Check for updates</button>
      {#if update}
        <button onclick={oninstallupdate} disabled={installing}>Install {update.version} and restart</button>
      {/if}
    </div>
    {#if updateMessage}<p class="hint">{updateMessage}</p>{/if}
    <label class="check">
      <input
        type="checkbox"
        checked={settings.autoCheckUpdates}
        onchange={(e) => onchange({ autoCheckUpdates: e.currentTarget.checked })}
      />
      Check for updates automatically
    </label>
  </div>
  <div class="field changelog">
    <h3>Changelog</h3>
    {#each releases as release}
      <h4>{release.title}</h4>
      {#each release.groups as group}
        {#if group.name}<h5>{group.name}</h5>{/if}
        <ul>
          {#each group.items as item}<li>{item}</li>{/each}
        </ul>
      {/each}
    {/each}
  </div>
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

  h3 {
    margin: 12px 0 0;
    font-size: 13px;
  }

  h4 {
    margin: 12px 0 0;
    font-size: 13px;
  }

  h5 {
    margin: 6px 0 0;
    font-size: 12px;
    color: #aaa;
  }

  .version {
    margin: 6px 0 0;
    font-family: Menlo, monospace;
  }

  .changelog {
    padding-bottom: 16px;
  }

  ul {
    margin: 4px 0 0;
    padding-left: 20px;
  }

  .hint {
    color: #888;
    font-size: 12px;
    margin: 6px 0 0;
  }
</style>
