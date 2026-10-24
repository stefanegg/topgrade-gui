<script lang="ts">
  import { formatDuration, type Step, type UnmatchedResult } from "./run";
  import { joinNames, sourceName } from "./changes";

  interface Props {
    steps: Step[];
    unmatched: UnmatchedResult[];
    now: number;
    /** Shows the step's output in the terminal. */
    onselect: (step: Step) => void;
  }

  let { steps, unmatched, now, onselect }: Props = $props();

  const labels: Record<Step["state"], string> = {
    running: "Running",
    done: "Finished",
    ok: "Succeeded",
    failed: "Failed",
    ignored: "Failed, ignored",
    skipped: "Skipped",
  };

  function note(step: Step): string {
    if (step.needsInput) return "Waiting for your input";
    const n = step.updates.length;
    if (n > 0) {
      const names = step.updates.slice(0, 3).map((u) => u.name);
      if (n > 3) names.push(`${n - 3} more`);
      return `${n} updated: ${joinNames(names)}`;
    }
    if (step.unlistedCount !== null) {
      return `About ${step.unlistedCount.toLocaleString()} packages changed, including dependencies`;
    }
    if (step.reason) return step.reason;
    return step.state === "running" ? "Running…" : "";
  }
</script>

<ul class="steps">
  {#each steps as step}
    <li class:nested={step.group !== null}>
      <button class="row" onclick={() => onselect(step)} title="Show this step's output">
        <span class="icon {step.state}" class:input={step.needsInput} aria-label={labels[step.state]}>
          {#if step.state === "running"}
            <span class="spinner"></span>
          {:else if step.state === "ok" || step.state === "done"}
            <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12l5 5 9-10" /></svg>
          {:else if step.state === "failed"}
            <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18" /></svg>
          {:else if step.state === "ignored"}
            <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 6v8M12 18v.5" /></svg>
          {:else}
            <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 12h10" /></svg>
          {/if}
        </span>
        <span class="text">
          <span class="name">{sourceName(step.name)}</span>
          {#if note(step)}<span class="note">{note(step)}</span>{/if}
        </span>
        <span class="time">{formatDuration((step.endedAt ?? now) - step.startedAt)}</span>
      </button>
    </li>
  {/each}
  {#each unmatched as result}
    <li>
      <div class="row">
        <span class="icon {result.status}" aria-label={labels[result.status]}>
          <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 12h10" /></svg>
        </span>
        <span class="text">
          <span class="name">{result.name}</span>
          <span class="note">{result.reason ?? "Reported in topgrade's summary"}</span>
        </span>
      </div>
    </li>
  {/each}
</ul>

<style>
  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  li + li .row {
    border-top: 1px solid var(--border-soft);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 14px;
    width: 100%;
    min-height: 44px;
    box-sizing: border-box;
    padding: 8px 20px;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
  }

  button.row {
    cursor: pointer;
  }

  button.row:hover {
    background: var(--surface-2);
  }

  .nested .row {
    padding-left: 48px;
  }

  .icon {
    width: 20px;
    height: 20px;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .icon svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 2.5;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .icon.ok,
  .icon.done {
    color: var(--ok);
  }

  .icon.failed {
    color: var(--danger);
  }

  .icon.ignored,
  .icon.input {
    color: var(--warn);
  }

  .icon.skipped {
    color: var(--muted);
  }

  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .note {
    color: var(--muted);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .time {
    flex-shrink: 0;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .spinner {
    width: 12px;
    height: 12px;
    border: 2px solid var(--track);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
