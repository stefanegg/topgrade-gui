<script lang="ts">
  import {
    formatAgo,
    formatDate,
    formatDuration,
    isDryRun,
    summaryTotals,
    type RunSummary,
  } from "./run";
  import { collectChanges, highlightNames } from "./changes";

  interface Props {
    runs: RunSummary[];
    now: number;
    /** Opening a run replays it, which can't happen during a live run. */
    disabled: boolean;
    onopen: (run: RunSummary) => void;
  }

  let { runs, now, disabled, onopen }: Props = $props();

  function names(run: RunSummary): string {
    const changes = collectChanges(
      run.steps.map((s) => ({ name: s.name, group: s.group, items: s.updates?.items ?? [] })),
    );
    const shown = highlightNames(changes);
    if (shown.length === 0) return "";
    const rest = changes.length - shown.length;
    return rest > 0 ? `${shown.join(", ")} and ${rest} more` : shown.join(", ");
  }
</script>

<div class="history">
  <div class="head">
    <h1>History</h1>
    <span class="muted">Your last runs are kept here with their full output.</span>
  </div>
  {#if disabled}
    <p class="muted">Past runs can be opened once the current run has finished.</p>
  {/if}
  {#if runs.length === 0}
    <p class="muted">No saved runs yet. Each run is saved here when it finishes.</p>
  {:else}
    <ul class="card">
      {#each runs as run (run.id)}
        {@const totals = summaryTotals(run)}
        <li>
          <button onclick={() => onopen(run)} {disabled}>
            <span class="when">
              <span>{formatDate(run.startedAt)}</span>
              <span class="muted small">{formatAgo(run.startedAt, now)}</span>
            </span>
            <span class="what">
              <span>
                {#if isDryRun(run)}
                  Preview
                {:else if totals.updated > 0}
                  <strong>{totals.updated}</strong> {totals.updated === 1 ? "update" : "updates"}
                {:else}
                  Nothing updated
                {/if}
                {#if totals.failed > 0}
                  · <span class="failed">{totals.failed} failed</span>
                {/if}
                {#if run.exitCode !== null && run.exitCode !== 0}
                  · <span class="failed">exit code {run.exitCode}</span>
                {/if}
              </span>
              {#if names(run)}<span class="muted small">{names(run)}</span>{/if}
            </span>
            <span class="muted duration">
              {run.durationSecs > 0 ? formatDuration(run.durationSecs * 1000) : ""}
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .history {
    width: 100%;
    max-width: 1120px;
    margin: 0 auto;
    box-sizing: border-box;
    padding: 28px 24px 40px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 12px;
  }

  h1 {
    margin: 0;
    font-size: 22px;
    font-weight: 600;
  }

  p {
    margin: 0;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow: hidden;
  }

  li + li button {
    border-top: 1px solid var(--border-soft);
  }

  button {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 24px;
    width: 100%;
    min-height: 56px;
    padding: 12px 20px;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  button:hover:not(:disabled) {
    background: var(--surface-2);
  }

  button:disabled {
    cursor: default;
  }

  .when {
    width: 180px;
    display: flex;
    flex-direction: column;
  }

  .what {
    flex: 1;
    min-width: 200px;
    display: flex;
    flex-direction: column;
  }

  .small {
    font-size: 12px;
  }

  .duration {
    font-variant-numeric: tabular-nums;
  }

  .failed {
    color: var(--danger);
    font-weight: 600;
  }
</style>
