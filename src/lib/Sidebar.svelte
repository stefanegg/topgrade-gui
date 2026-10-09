<script lang="ts">
  import { formatDuration, type Step, type UnmatchedResult } from "./run";

  interface Props {
    steps: Step[];
    unmatched: UnmatchedResult[];
    now: number;
    onselect: (step: Step) => void;
  }

  let { steps, unmatched, now, onselect }: Props = $props();

  const icons: Record<Step["state"], string> = {
    running: "",
    done: "•",
    ok: "✓",
    failed: "✗",
    ignored: "!",
    skipped: "–",
  };
</script>

<aside>
  {#if steps.length === 0}
    <p class="empty">Steps appear here once a run starts.</p>
  {/if}
  <ul>
    {#each steps as step}
      <li>
        <button
          class="step {step.state}"
          class:needs-input={step.needsInput}
          class:nested={step.group !== null}
          title={[step.group, step.name, step.reason].filter(Boolean).join(" · ")}
          onclick={() => onselect(step)}
        >
          <span class="icon">
            {#if step.state === "running"}<span class="spinner"></span>{:else}{icons[step.state]}{/if}
          </span>
          <span class="name">{step.name}</span>
          {#if step.needsInput}
            <span class="badge">input</span>
          {:else}
            <span class="time">{formatDuration((step.endedAt ?? now) - step.startedAt)}</span>
          {/if}
        </button>
        {#if step.reason}
          <div class="reason">{step.reason}</div>
        {/if}
      </li>
    {/each}
  </ul>
  {#if unmatched.length > 0}
    <h3>Other results</h3>
    <ul>
      {#each unmatched as result}
        <li class="step {result.status}" title={result.reason ?? result.name}>
          <span class="icon">{icons[result.status]}</span>
          <span class="name">{result.name}</span>
        </li>
      {/each}
    </ul>
  {/if}
</aside>

<style>
  aside {
    width: 260px;
    flex-shrink: 0;
    overflow-y: auto;
    background: #252525;
    border-right: 1px solid #3a3a3a;
    padding: 6px 0;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .empty {
    color: #777;
    padding: 8px 12px;
    margin: 0;
  }

  h3 {
    font-size: 11px;
    text-transform: uppercase;
    color: #888;
    margin: 12px 12px 4px;
  }

  .step {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 4px 12px;
    border: none;
    border-left: 3px solid transparent;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
  }

  button.step {
    cursor: pointer;
  }

  button.step:hover {
    background: #2f2f2f;
  }

  .step.nested {
    padding-left: 30px;
  }

  .step.needs-input {
    border-left-color: #e5b93b;
    background: #3a3320;
  }

  .icon {
    width: 14px;
    text-align: center;
    font-weight: bold;
  }

  .ok .icon {
    color: #4caf50;
  }
  .failed .icon {
    color: #e5534b;
  }
  .ignored .icon {
    color: #e5b93b;
  }
  .skipped .icon,
  .done .icon {
    color: #888;
  }

  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .time {
    color: #888;
    font-variant-numeric: tabular-nums;
  }

  .badge {
    font-size: 11px;
    padding: 0 6px;
    border-radius: 8px;
    background: #e5b93b;
    color: #1e1e1e;
  }

  .reason {
    padding: 0 12px 4px 37px;
    color: #888;
    font-size: 12px;
  }

  .spinner {
    display: inline-block;
    width: 9px;
    height: 9px;
    border: 2px solid #2f6fd6;
    border-right-color: transparent;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
