<script lang="ts">
  import changelogSource from "../../CHANGELOG.md?raw";
  import type { Settings, UpdateInfo } from "./run";
  import type { Theme } from "./theme";

  const themes: { value: Theme; label: string }[] = [
    { value: "system", label: "System" },
    { value: "light", label: "Light" },
    { value: "dark", label: "Dark" },
  ];

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

<div class="settings">
  <h1>Settings</h1>

  <section class="card">
    <h2>Appearance</h2>
    <fieldset class="field">
      <legend>Theme</legend>
      <div class="segmented">
        {#each themes as theme}
          <label class:selected={settings.theme === theme.value}>
            <input
              type="radio"
              name="theme"
              value={theme.value}
              checked={settings.theme === theme.value}
              onchange={() => onchange({ theme: theme.value })}
            />
            {theme.label}
          </label>
        {/each}
      </div>
      <p class="hint">System follows your desktop's light or dark setting. The terminal stays dark.</p>
    </fieldset>
  </section>

  <section class="card">
    <h2>Reminders and startup</h2>
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
      <p class="hint">Checked hourly while the app runs in the tray. Previews don't count.</p>
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
    <p class="hint field">
      The options on the Overview page are saved too, and are used by Run now in the tray.
    </p>
  </section>

  <section class="card">
    <h2>About</h2>
    <div class="field">
      <p class="version">Version {version || "…"}</p>
      <div class="inline">
        <button class="btn" onclick={oncheckupdate} disabled={checkingUpdate || installing}>Check for updates</button>
        {#if update}
          <button class="btn primary" onclick={oninstallupdate} disabled={installing}>
            Install {update.version} and restart
          </button>
        {/if}
      </div>
      {#if updateMessage}<p class="hint">{updateMessage}</p>{/if}
    </div>
    <div class="field">
      <label class="check">
        <input
          type="checkbox"
          checked={settings.autoCheckUpdates}
          onchange={(e) => onchange({ autoCheckUpdates: e.currentTarget.checked })}
        />
        Check for updates automatically
      </label>
    </div>
  </section>

  <section class="card changelog">
    <h2>Changelog</h2>
    <div class="field">
      {#each releases as release}
        <h3>{release.title}</h3>
        {#each release.groups as group}
          {#if group.name}<h4>{group.name}</h4>{/if}
          <ul>
            {#each group.items as item}<li>{item}</li>{/each}
          </ul>
        {/each}
      {/each}
    </div>
  </section>
</div>

<style>
  .settings {
    width: 100%;
    max-width: 760px;
    margin: 0 auto;
    box-sizing: border-box;
    padding: 28px 24px 40px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  h1 {
    margin: 0;
    font-size: 22px;
    font-weight: 600;
  }

  section {
    padding: 20px 24px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }

  .field {
    margin: 0;
    padding: 0;
    border: 0;
  }

  legend {
    padding: 0;
    margin-bottom: 8px;
  }

  .segmented {
    display: inline-flex;
    padding: 3px;
    gap: 2px;
    border-radius: 10px;
    background: var(--surface-2);
    border: 1px solid var(--border);
  }

  .segmented label {
    min-height: 34px;
    padding: 0 18px;
    border-radius: 7px;
    display: flex;
    align-items: center;
    cursor: pointer;
    color: var(--muted);
  }

  .segmented label.selected {
    background: var(--surface);
    color: var(--text);
    font-weight: 600;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.15);
  }

  .segmented label:focus-within {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .segmented input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }

  .inline {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
  }

  input[type="number"] {
    width: 72px;
    min-height: 36px;
    box-sizing: border-box;
    padding: 0 8px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: inherit;
    font: inherit;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  h3 {
    margin: 16px 0 0;
    font-size: 14px;
  }

  h3:first-child {
    margin-top: 0;
  }

  h4 {
    margin: 6px 0 0;
    font-size: 13px;
    color: var(--muted);
  }

  .version {
    margin: 0;
    font-family: ui-monospace, Menlo, monospace;
  }

  ul {
    margin: 4px 0 0;
    padding-left: 20px;
  }

  .hint {
    color: var(--muted);
    font-size: 13px;
    margin: 8px 0 0;
  }

  section > .hint {
    margin: 0;
  }
</style>
