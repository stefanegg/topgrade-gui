<script lang="ts">
  import { formatAgo, formatDate, isFinished, summaryTotals, type RunSummary } from "./run";
  import { collectChanges, highlightNames, joinNames } from "./changes";

  interface Props {
    /** Newest real (non-dry) run, finished or not. */
    lastRun: RunSummary | undefined;
    /** Newest real run that finished; the reminder counts from this one. */
    lastUpdate: RunSummary | undefined;
    remindAfterDays: number;
    now: number;
    cleanup: boolean;
    assumeYes: boolean;
    onoptions: (patch: { cleanup?: boolean; assumeYes?: boolean }) => void;
    onrun: (dryRun: boolean) => void;
    onopenrun: (run: RunSummary) => void;
  }

  let { lastRun, lastUpdate, remindAfterDays, now, cleanup, assumeYes, onoptions, onrun, onopenrun }: Props =
    $props();

  const DAY = 24 * 60 * 60 * 1000;

  let due = $derived(
    !lastUpdate || now - new Date(lastUpdate.startedAt).getTime() >= remindAfterDays * DAY,
  );
  /** The last run was stopped (or crashed) before topgrade's summary. */
  let unfinished = $derived(lastRun !== undefined && !isFinished(lastRun));

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
    if (names.length === 0) {
      return unfinished
        ? "It stopped before anything was reported as updated."
        : "Nothing needed updating.";
    }
    const rest = lastChanges.length - names.length;
    const list = rest > 0 ? `${names.join(", ")} and ${rest} more` : joinNames(names);
    return unfinished ? `Updated before it stopped: ${list}` : list;
  });
</script>

<div class="home">
  <section class="main-col">
    <div class="card hero">
      <div class="status">
        <div class="status-icon" class:due={due || unfinished}>
          {#if unfinished}
            <svg width="28" height="28" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 7v6M12 17v.5" /><circle cx="12" cy="12" r="9" /></svg>
          {:else if due}
            <svg width="28" height="28" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 2" /></svg>
          {:else}
            <svg width="28" height="28" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12l5 5 9-10" /></svg>
          {/if}
        </div>
        <div>
          {#if unfinished && lastRun}
            <h1>Your last update didn't finish</h1>
            <p class="muted">
              It was stopped on {formatDate(lastRun.startedAt)}, so some steps didn't run.
              {#if lastUpdate}
                The last complete update was {formatAgo(lastUpdate.startedAt, now)}.
              {:else}
                No update has finished yet.
              {/if}
            </p>
          {:else if !lastUpdate}
            <h1>Ready for your first update</h1>
            <p class="muted">Topgrade GUI runs topgrade, which updates everything it finds on this computer, one source after another.</p>
          {:else}
            <h1>{due ? "Time for an update" : "Up to date"}</h1>
            <p class="muted">
              Last updated {formatAgo(lastUpdate.startedAt, now)} ({formatDate(lastUpdate.startedAt)}).
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
            {#if unfinished}
              Last run: <span class="failed">didn't finish</span>
            {:else}
              Last run: {lastTotals.updated} {lastTotals.updated === 1 ? "update" : "updates"}
              {#if lastTotals.failed > 0}<span class="failed">· {lastTotals.failed} failed</span>{/if}
            {/if}
          </span>
          <span class="muted">{lastSummary}</span>
        </div>
        <button class="btn" onclick={() => onopenrun(lastRun)}>
          {unfinished ? "See details" : "See what changed"}
        </button>
      </div>
    {/if}
  </section>
</div>

<style>
  .home {
    width: 100%;
    max-width: 760px;
    margin: 0 auto;
    box-sizing: border-box;
    padding: 32px 24px;
  }

  .main-col {
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

</style>
