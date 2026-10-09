<script lang="ts">
  import {
    formatAgo,
    formatDate,
    formatDuration,
    isDryRun,
    summaryTotals,
    type RunSummary,
  } from "./run";

  interface Props {
    runs: RunSummary[];
    now: number;
    onopen: (run: RunSummary) => void;
    onclose: () => void;
  }

  let { runs, now, onopen, onclose }: Props = $props();
</script>

<section>
  <header>
    <h2>History</h2>
    <span class="hint">Last {runs.length} {runs.length === 1 ? "run" : "runs"}</span>
    <button onclick={onclose}>Close</button>
  </header>
  {#if runs.length === 0}
    <p class="empty">No saved runs yet. Each run is saved here when it finishes.</p>
  {:else}
    <ul>
      {#each runs as run (run.id)}
        {@const totals = summaryTotals(run)}
        <li>
          <button onclick={() => onopen(run)}>
            <span class="date">{formatDate(run.startedAt)}</span>
            <span class="ago">{formatAgo(run.startedAt, now)}</span>
            <span class="result">
              {#if totals.updated > 0}
                <strong>{totals.updated}</strong> updated
              {:else}
                Nothing updated
              {/if}
              {#if totals.failed > 0}
                · <span class="failed">{totals.failed} failed</span>
              {/if}
              {#if run.exitCode !== null && run.exitCode !== 0}
                · <span class="failed">exit {run.exitCode}</span>
              {/if}
            </span>
            <span class="tags">
              {#if isDryRun(run)}<span class="tag">dry run</span>{/if}
            </span>
            <span class="duration">
              {run.durationSecs > 0 ? formatDuration(run.durationSecs * 1000) : ""}
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
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
    gap: 12px;
    padding: 10px 16px;
    border-bottom: 1px solid #3a3a3a;
  }

  h2 {
    margin: 0;
    font-size: 15px;
  }

  .hint {
    color: #888;
    flex: 1;
  }

  header button {
    padding: 4px 14px;
    border-radius: 6px;
    border: 1px solid #555;
    background: #3a3a3a;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .empty {
    color: #888;
    padding: 16px;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  li button {
    display: grid;
    grid-template-columns: 170px 70px 1fr auto 70px;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 10px 16px;
    border: none;
    border-bottom: 1px solid #2a2a2a;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  li button:hover {
    background: #2a2a2a;
  }

  .ago,
  .duration {
    color: #888;
    font-variant-numeric: tabular-nums;
  }

  .duration {
    text-align: right;
  }

  .result strong {
    color: #4caf50;
  }

  .failed {
    color: #e5534b;
    font-weight: bold;
  }

  .tag {
    font-size: 11px;
    padding: 1px 6px;
    border-radius: 8px;
    background: #333;
    color: #aaa;
  }
</style>
