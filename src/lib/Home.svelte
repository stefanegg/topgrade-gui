<script lang="ts">
  import { formatAgo, formatDate, summaryTotals, type RunSummary, type StepStatus } from "./run";
  import { collectChanges, highlightNames, joinNames, sourceName } from "./changes";

  interface Props {
    lastRun: RunSummary | undefined;
    remindAfterDays: number;
    now: number;
    cleanup: boolean;
    assumeYes: boolean;
    onoptions: (patch: { cleanup?: boolean; assumeYes?: boolean }) => void;
    onrun: (dryRun: boolean) => void;
    onopenrun: (run: RunSummary) => void;
  }

  let { lastRun, remindAfterDays, now, cleanup, assumeYes, onoptions, onrun, onopenrun }: Props =
    $props();

  const DAY = 24 * 60 * 60 * 1000;

  let due = $derived(
    !lastRun || now - new Date(lastRun.startedAt).getTime() >= remindAfterDays * DAY,
  );

  let lastChanges = $derived(
    lastRun
      ? collectChanges(
          lastRun.steps.map((s) => ({ name: s.name, group: s.group, items: s.updates?.items ?? [] })),
        )
      : [],
  );
  let lastTotals = $derived(lastRun ? summaryTotals(lastRun) : null);

  let lastSummary = $derived.by(() => {
    const names = highlightNames(lastChanges);
    if (names.length === 0) return "Nothing needed updating.";
    const rest = lastChanges.length - names.length;
    return rest > 0 ? `${names.join(", ")} and ${rest} more` : joinNames(names);
  });

  /** Top-level steps of the last run; a nested run (e.g. inside Toolbx)
   *  counts toward the top-level step it ran in, the one before it. */
  let sources = $derived.by(() => {
    if (!lastRun) return [];
    const rows: { name: string; count: number; status: StepStatus | null }[] = [];
    for (const s of lastRun.steps) {
      const count = s.updates?.items.length ?? 0;
      if (s.group === null) rows.push({ name: s.name, count, status: s.status });
      else if (rows.length > 0) rows[rows.length - 1].count += count;
    }
    return rows.sort((a, b) => b.count - a.count);
  });

  const SHOWN_SOURCES = 8;
</script>

<div class="home">
  <section class="main-col">
    <div class="card hero">
      <div class="status">
        <div class="status-icon" class:due>
          {#if due}
            <svg width="28" height="28" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 2" /></svg>
          {:else}
            <svg width="28" height="28" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12l5 5 9-10" /></svg>
          {/if}
        </div>
        <div>
          {#if !lastRun}
            <h1>Ready for your first update</h1>
            <p class="muted">Topgrade GUI runs topgrade, which updates everything it finds on this computer, one source after another.</p>
          {:else}
            <h1>{due ? "Time for an update" : "Up to date"}</h1>
            <p class="muted">
              Last updated {formatAgo(lastRun.startedAt, now)} ({formatDate(lastRun.startedAt)}).
            </p>
          {/if}
        </div>
      </div>
      <div class="actions">
        <button class="btn primary large" onclick={() => onrun(false)}>Update everything</button>
        <button class="btn large" onclick={() => onrun(true)}>Check what would change</button>
      </div>
      <fieldset>
        <legend>Options</legend>
        <label>
          <input
            type="checkbox"
            checked={assumeYes}
            onchange={(e) => onoptions({ assumeYes: e.currentTarget.checked })}
          />
          <span>
            <span class="opt">Answer “yes” to confirmations</span>
            <span class="muted small">Package managers won't stop to ask. You may still be asked for your password.</span>
          </span>
        </label>
        <label>
          <input
            type="checkbox"
            checked={cleanup}
            onchange={(e) => onoptions({ cleanup: e.currentTarget.checked })}
          />
          <span>
            <span class="opt">Clean up afterwards</span>
            <span class="muted small">Remove old versions and caches to free disk space.</span>
          </span>
        </label>
      </fieldset>
    </div>

    {#if lastRun && lastTotals}
      <div class="card last">
        <div class="last-text">
          <span class="strong">
            Last run: {lastTotals.updated} {lastTotals.updated === 1 ? "update" : "updates"}
            {#if lastTotals.failed > 0}<span class="failed">· {lastTotals.failed} failed</span>{/if}
          </span>
          <span class="muted">{lastSummary}</span>
        </div>
        <button class="btn" onclick={() => onopenrun(lastRun)}>See what changed</button>
      </div>
    {/if}
  </section>

  {#if sources.length > 0}
    <aside class="card sources">
      <h2>What topgrade updates</h2>
      <ul>
        {#each sources.slice(0, SHOWN_SOURCES) as source}
          <li>
            <span class="abbr" aria-hidden="true">{sourceName(source.name).slice(0, 2)}</span>
            <span class="source-text">
              <span>{sourceName(source.name)}</span>
              <span class="muted small">
                {#if source.status === "failed"}
                  <span class="failed">Failed last time</span>
                {:else if source.count > 0}
                  {source.count} updated last time
                {:else}
                  No package changes listed
                {/if}
              </span>
            </span>
          </li>
        {/each}
      </ul>
      {#if sources.length > SHOWN_SOURCES}
        <p class="muted small more">and {sources.length - SHOWN_SOURCES} more</p>
      {/if}
    </aside>
  {/if}
</div>

<style>
  .home {
    width: 100%;
    max-width: 1120px;
    margin: 0 auto;
    box-sizing: border-box;
    padding: 32px 24px;
    display: flex;
    flex-wrap: wrap;
    gap: 24px;
    align-items: flex-start;
  }

  .main-col {
    flex: 999 1 520px;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  .hero {
    padding: 32px;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .status {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .status-icon {
    width: 56px;
    height: 56px;
    border-radius: 28px;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--ok-soft);
    color: var(--ok);
  }

  .status-icon.due {
    background: var(--warn-soft);
    color: var(--warn);
  }

  .status-icon svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  h1 {
    margin: 0 0 4px;
    font-size: 24px;
    font-weight: 600;
  }

  p {
    margin: 0;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }

  fieldset {
    border: 0;
    margin: 0;
    padding: 16px 0 0;
    border-top: 1px solid var(--border-soft);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  legend {
    padding: 16px 0 4px;
    font-weight: 600;
  }

  label {
    display: flex;
    gap: 10px;
    align-items: flex-start;
  }

  label input {
    margin-top: 3px;
  }

  .opt {
    display: block;
  }

  .small {
    font-size: 13px;
  }

  .last {
    padding: 20px 24px;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 16px;
  }

  .last-text {
    flex: 1;
    min-width: 200px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .strong {
    font-weight: 600;
  }

  .failed {
    color: var(--danger);
  }

  .sources {
    flex: 1 1 300px;
    min-width: 0;
    padding: 20px 0;
  }

  h2 {
    margin: 0 24px 12px;
    font-size: 15px;
    font-weight: 600;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 24px;
    border-top: 1px solid var(--border-soft);
  }

  .abbr {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--muted);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    font-weight: 700;
    text-transform: uppercase;
    flex-shrink: 0;
  }

  .source-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .more {
    margin: 12px 24px 0;
  }
</style>
