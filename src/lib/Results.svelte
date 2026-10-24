<script lang="ts">
  import { formatDate, formatDuration, type Step, type UnmatchedResult } from "./run";
  import {
    CATEGORIES,
    changesAsText,
    collectChanges,
    highlightNames,
    joinNames,
    sourceName,
    type Category,
  } from "./changes";
  import StepList from "./StepList.svelte";

  interface Props {
    steps: Step[];
    unmatched: UnmatchedResult[];
    now: number;
    /** ISO date of the run's start. */
    startedAt: string;
    durationMs: number | null;
    dryRun: boolean;
    /** Opened from History rather than just finished. */
    saved: boolean;
    /** Still replaying a saved run. */
    loading: boolean;
    /** Exit problem ("Run ended with exit code 1"); null when it exited cleanly. */
    problem: string | null;
    degraded: boolean;
    logPath: string | null;
    ondone: () => void;
    onshowlog: () => void;
    onshowterminal: () => void;
    onselect: (step: Step) => void;
  }

  let {
    steps,
    unmatched,
    now,
    startedAt,
    durationMs,
    dryRun,
    saved,
    loading,
    problem,
    degraded,
    logPath,
    ondone,
    onshowlog,
    onshowterminal,
    onselect,
  }: Props = $props();

  let changes = $derived(
    collectChanges(steps.map((s) => ({ name: s.name, group: s.group, items: s.updates }))),
  );
  let failed = $derived(steps.filter((s) => s.state === "failed"));
  let unlisted = $derived(steps.filter((s) => s.unlistedCount !== null && s.updates.length === 0));
  let restart = $derived(changes.filter((c) => c.needsRestart));
  let counts = $derived(
    Object.fromEntries(
      CATEGORIES.map(({ key }) => [key, changes.filter((c) => c.category === key).length]),
    ) as Record<Category, number>,
  );

  let filter = $state<Category | "all">("all");
  let query = $state("");
  let expandedGroups = $state(new Set<Category>());
  let showSteps = $state(false);
  let copied = $state(false);

  const PAGE = 8;

  let groups = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return CATEGORIES.filter(({ key }) => filter === "all" || filter === key)
      .map((cat) => {
        const items = changes.filter(
          (c) =>
            c.category === cat.key &&
            (!q || [c.name, c.id ?? "", c.source].some((t) => t.toLowerCase().includes(q))),
        );
        const open = expandedGroups.has(cat.key) || q !== "";
        return { ...cat, items, shown: open ? items : items.slice(0, PAGE) };
      })
      .filter((g) => g.items.length > 0);
  });

  let headline = $derived.by(() => {
    if (loading) return "Loading run…";
    if (dryRun) return "Preview finished. Nothing was changed.";
    const n = changes.length;
    const took = durationMs ? ` in ${formatDuration(durationMs)}` : "";
    if (n === 0) return problem ? "The run didn't finish cleanly" : "Everything was already up to date";
    return `${n} ${n === 1 ? "update" : "updates"} installed${took}`;
  });

  let story = $derived.by(() => {
    if (loading || changes.length === 0) return "";
    const apps = changes.filter((c) => c.category === "apps");
    const parts: string[] = [];
    if (apps.length > 0) {
      const names = highlightNames(apps);
      const rest = apps.length - names.length;
      parts.push(
        rest > 0
          ? `${names.join(", ")} and ${rest} more ${rest === 1 ? "app" : "apps"} were updated.`
          : `${joinNames(names)} ${names.length === 1 ? "was" : "were"} updated.`,
      );
    }
    const other =
      counts.system > 0 && counts.firmware > 0
        ? "system packages, tools and firmware"
        : counts.system > 0
          ? "system packages and tools"
          : counts.firmware > 0
            ? "firmware"
            : "";
    if (other) parts.push(apps.length > 0 ? `The rest were ${other}.` : `They were ${other}.`);
    return parts.join(" ");
  });

  function toggleGroup(key: Category) {
    const next = new Set(expandedGroups);
    if (!next.delete(key)) next.add(key);
    expandedGroups = next;
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(changesAsText(changes));
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch {
      // Clipboard denied: nothing sensible to fall back to.
    }
  }

  $effect(() => {
    // Problems are easiest to understand from the step list.
    if (failed.length > 0) showSteps = true;
  });
</script>

<div class="results">
  <div class="head">
    <div class="status-icon" class:bad={problem || failed.length > 0} aria-hidden="true">
      {#if problem || failed.length > 0}
        <svg width="26" height="26" viewBox="0 0 24 24"><path d="M12 7v6M12 17v.5" /></svg>
      {:else}
        <svg width="26" height="26" viewBox="0 0 24 24"><path d="M5 12l5 5 9-10" /></svg>
      {/if}
    </div>
    <div class="title">
      <h1>{headline}</h1>
      <p class="muted">
        {saved ? "Saved run from" : "Finished"} {formatDate(startedAt)}
        · {steps.length} {steps.length === 1 ? "step" : "steps"}{failed.length > 0
          ? `, ${failed.length} failed`
          : ", none failed"}
        {#if dryRun}<span class="tag">preview</span>{/if}
      </p>
    </div>
    <div class="head-actions">
      {#if changes.length > 0}
        <button class="btn" onclick={copy}>{copied ? "Copied" : "Copy list"}</button>
      {/if}
      {#if logPath}
        <button class="btn" onclick={onshowlog} title={logPath}>Show log file</button>
      {/if}
      <button class="btn primary" onclick={ondone}>{saved ? "Close" : "Done"}</button>
    </div>
  </div>

  {#if story}
    <p class="story">{story}</p>
  {/if}

  {#if problem}
    <div class="note bad">{problem}</div>
  {/if}
  {#if failed.length > 0}
    <div class="note bad">
      <strong>{failed.length === 1 ? "1 step failed" : `${failed.length} steps failed`}:</strong>
      {joinNames(failed.map((s) => sourceName(s.name)))}. Their output is in the terminal below.
    </div>
  {/if}
  {#if degraded}
    <div class="note warn">
      Couldn't recognise topgrade's steps in the output, so there's no summary.
      <button class="link-button" onclick={onshowterminal}>Show the terminal output</button>
    </div>
  {/if}
  {#if restart.length > 0 && !dryRun}
    <div class="note warn">
      <svg width="20" height="20" viewBox="0 0 24 24" aria-hidden="true"><path d="M21 12a9 9 0 1 1-3-6.7" /><path d="M21 4v5h-5" /></svg>
      <span>
        <strong>Restart when convenient.</strong>
        {restart.length === 1 ? "1 update" : `${restart.length} updates`} (firmware or system core) only take effect after a restart.
      </span>
    </div>
  {/if}

  {#if changes.length > 0}
    <div class="filters">
      <div class="chips" role="group" aria-label="Show">
        <button class="chip" aria-pressed={filter === "all"} onclick={() => (filter = "all")}>
          Everything <span>{changes.length}</span>
        </button>
        {#each CATEGORIES as cat}
          {#if counts[cat.key] > 0}
            <button class="chip" aria-pressed={filter === cat.key} onclick={() => (filter = cat.key)}>
              {cat.title} <span>{counts[cat.key]}</span>
            </button>
          {/if}
        {/each}
      </div>
      <label class="search">
        <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="7" /><path d="M20 20l-4-4" /></svg>
        <span class="visually-hidden">Find a package</span>
        <input type="search" placeholder="Find a package" bind:value={query} />
      </label>
    </div>

    {#each groups as group (group.key)}
      <section class="card group">
        <div class="group-head">
          <h2>{group.title}</h2>
          <span class="muted">{group.items.length}</span>
          <span class="muted hint">{group.hint}</span>
        </div>
        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Name</th>
                <th class="source-col">Installed via</th>
                <th class="version-col">Version</th>
              </tr>
            </thead>
            <tbody>
              {#each group.shown as change}
                <tr>
                  <td>
                    <span class="pkg">{change.name}</span>
                    {#if change.id}<span class="id">{change.id}</span>{/if}
                  </td>
                  <td><span class="badge">{change.source}</span></td>
                  <td class="version">
                    {#if change.from}<span class="muted">{change.from}</span> <span class="muted" aria-label="to">→</span>{/if}
                    {#if change.to}<strong>{change.to}</strong>{:else if !change.from}<span class="muted">updated</span>{/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
        {#if group.items.length > PAGE && query.trim() === ""}
          <button class="more" onclick={() => toggleGroup(group.key)}>
            {expandedGroups.has(group.key) ? "Show fewer" : `Show ${group.items.length - PAGE} more`}
          </button>
        {/if}
      </section>
    {:else}
      <p class="muted">Nothing matches “{query}”.</p>
    {/each}
  {/if}

  {#if !loading && steps.length > 0}
    <p class="unlisted">
      {#each unlisted as step}
        <strong>{sourceName(step.name)}</strong> changed about {step.unlistedCount?.toLocaleString()} packages, including dependencies.
      {/each}
      Some tools don't report package names; their details are only in the
      <button class="link-button" onclick={onshowterminal}>terminal output</button>.
    </p>

    <section class="card steps">
      <button class="steps-toggle" aria-expanded={showSteps} onclick={() => (showSteps = !showSteps)}>
        <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true" class:open={showSteps}><path d="M9 6l6 6-6 6" /></svg>
        All steps ({steps.length + unmatched.length})
      </button>
      {#if showSteps}
        <StepList {steps} {unmatched} {now} {onselect} />
      {/if}
    </section>
  {/if}
</div>

<style>
  .results {
    width: 100%;
    max-width: 1120px;
    margin: 0 auto;
    box-sizing: border-box;
    padding: 28px 24px 40px;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 16px;
  }

  .status-icon {
    width: 48px;
    height: 48px;
    border-radius: 24px;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--ok-soft);
    color: var(--ok);
  }

  .status-icon.bad {
    background: var(--danger-soft);
    color: var(--danger);
  }

  svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 2.5;
    stroke-linecap: round;
    stroke-linejoin: round;
    flex-shrink: 0;
  }

  .title {
    flex: 1;
    min-width: 240px;
  }

  h1 {
    margin: 0 0 2px;
    font-size: 24px;
    font-weight: 600;
  }

  p {
    margin: 0;
  }

  .tag {
    margin-left: 6px;
    padding: 1px 8px;
    border-radius: 10px;
    background: var(--surface-2);
    font-size: 12px;
  }

  .head-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .story {
    font-size: 17px;
    line-height: 1.5;
    max-width: 760px;
  }

  .note {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 14px 18px;
    border-radius: 12px;
    border: 1px solid;
  }

  .note > svg {
    stroke-width: 2;
    margin-top: 1px;
  }

  .note.warn {
    background: var(--warn-soft);
    border-color: var(--warn-border);
    color: var(--warn-text);
  }

  .note.bad {
    display: block;
    background: var(--danger-soft);
    border-color: var(--danger-border);
    color: var(--text);
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    align-items: center;
    justify-content: space-between;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .chip {
    min-height: 36px;
    padding: 0 14px;
    border-radius: 18px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-weight: 500;
    cursor: pointer;
  }

  .chip span {
    opacity: 0.75;
    margin-left: 2px;
  }

  .chip[aria-pressed="true"] {
    background: var(--chip-on-bg);
    border-color: var(--chip-on-bg);
    color: var(--chip-on-fg);
  }

  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 36px;
    min-width: 220px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    color: var(--muted);
  }

  .search svg {
    stroke-width: 2;
  }

  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: none;
    color: var(--text);
    font: inherit;
  }

  .group {
    overflow: hidden;
  }

  .group-head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 10px;
    padding: 14px 20px;
    border-bottom: 1px solid var(--border-soft);
  }

  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }

  .hint {
    flex: 1;
    text-align: right;
    font-size: 13px;
  }

  .table-wrap {
    overflow-x: auto;
  }

  table {
    width: 100%;
    min-width: 600px;
    border-collapse: collapse;
  }

  th {
    text-align: left;
    padding: 8px 20px;
    font-size: 12px;
    font-weight: 500;
    color: var(--muted);
  }

  .source-col {
    width: 200px;
  }

  .version-col {
    width: 280px;
  }

  td {
    padding: 10px 20px;
    border-top: 1px solid var(--border-soft);
    vertical-align: top;
  }

  .pkg {
    display: block;
    font-weight: 500;
  }

  .id,
  .version {
    font-family: ui-monospace, Menlo, "DejaVu Sans Mono", monospace;
    font-size: 12px;
  }

  .id {
    color: var(--muted);
  }

  .version {
    font-size: 13px;
    word-break: break-all;
  }

  .badge {
    display: inline-block;
    padding: 2px 8px;
    border-radius: 10px;
    background: var(--surface-2);
    color: var(--muted);
    font-size: 12px;
  }

  .more {
    width: 100%;
    min-height: 44px;
    border: 0;
    border-top: 1px solid var(--border-soft);
    background: none;
    color: var(--link);
    font: inherit;
    cursor: pointer;
  }

  .unlisted {
    padding: 14px 20px;
    border: 1px dashed var(--border);
    border-radius: 12px;
    color: var(--muted);
  }

  .unlisted strong {
    color: var(--text);
  }

  .steps {
    overflow: hidden;
  }

  .steps-toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-height: 48px;
    padding: 0 20px;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
    text-align: left;
  }

  .steps-toggle svg {
    stroke-width: 2;
    color: var(--muted);
    transition: transform 0.15s;
  }

  .steps-toggle svg.open {
    transform: rotate(90deg);
  }
</style>
