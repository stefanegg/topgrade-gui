<script lang="ts">
  import { formatDuration, type PromptKind, type Step } from "./run";
  import { sourceName } from "./changes";
  import StepList from "./StepList.svelte";

  interface Props {
    steps: Step[];
    now: number;
    startedAt: number;
    dryRun: boolean;
    /** Step count of the last comparable run, to estimate progress. */
    expectedSteps: number | null;
    prompt: PromptKind | null;
    stopping: boolean;
    onstop: () => void;
    onpassword: (password: string) => void;
    onshowterminal: () => void;
    onselect: (step: Step) => void;
  }

  let {
    steps,
    now,
    startedAt,
    dryRun,
    expectedSteps,
    prompt,
    stopping,
    onstop,
    onpassword,
    onshowterminal,
    onselect,
  }: Props = $props();

  let password = $state("");

  let current = $derived(steps.findLast((s) => s.state === "running"));
  let updatedSoFar = $derived(steps.reduce((n, s) => n + s.updates.length, 0));
  let total = $derived(
    expectedSteps && expectedSteps >= steps.length ? expectedSteps : null,
  );
  // Never reach 100% on an estimate; the run ends when topgrade says so.
  let progress = $derived(total ? Math.min(0.95, steps.length / total) : null);

  function submit(e: SubmitEvent) {
    e.preventDefault();
    onpassword(password);
    password = "";
  }
</script>

<div class="running">
  <div class="head">
    <div class="title">
      <h1>
        {#if prompt}
          Waiting for you
        {:else if current}
          {dryRun ? "Checking" : "Updating"} {sourceName(current.name)}…
        {:else}
          Starting…
        {/if}
      </h1>
      <p class="muted">
        {#if steps.length > 0}
          Step {steps.length}{#if total}&nbsp;of about {total}{/if} ·
        {/if}
        started {formatDuration(now - startedAt)} ago
        {#if !dryRun}· {updatedSoFar === 0 ? "nothing installed yet" : `${updatedSoFar} updated so far`}{/if}
      </p>
    </div>
    <button class="btn large" onclick={onstop} disabled={stopping}>{stopping ? "Stopping…" : "Stop"}</button>
  </div>

  <div
    class="track"
    class:indeterminate={progress === null}
    role="progressbar"
    aria-label="Overall progress"
    aria-valuemin="0"
    aria-valuemax="100"
    aria-valuenow={progress === null ? undefined : Math.round(progress * 100)}
  >
    <div class="bar" style:width={progress === null ? undefined : `${progress * 100}%`}></div>
  </div>

  {#if prompt === "password"}
    <form class="prompt" onsubmit={submit} aria-live="polite">
      <svg width="28" height="28" viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="11" width="16" height="10" rx="2" /><path d="M8 11V7a4 4 0 0 1 8 0v4" /></svg>
      <div class="prompt-text">
        <label for="password" class="strong">{current ? sourceName(current.name) : "This step"} needs your password</label>
        <span>It goes straight to the terminal (sudo) and isn't stored.</span>
      </div>
      <div class="prompt-input">
        <!-- svelte-ignore a11y_autofocus -->
        <input id="password" type="password" autocomplete="off" bind:value={password} autofocus />
        <button class="btn primary large" type="submit">Continue</button>
      </div>
    </form>
  {:else if prompt}
    <div class="prompt" aria-live="polite">
      <svg width="28" height="28" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" /><path d="M12 8v5M12 16.5v.5" /></svg>
      <div class="prompt-text">
        <span class="strong">
          {prompt === "retry" ? "A step failed. Retry it?" : "A question from topgrade"}
        </span>
        <span>Read the question and answer it in the terminal.</span>
      </div>
      <button class="btn primary large" onclick={onshowterminal}>Answer in terminal</button>
    </div>
  {/if}

  <section class="card">
    <StepList {steps} unmatched={[]} {now} {onselect} />
  </section>
</div>

<style>
  .running {
    width: 100%;
    max-width: 1120px;
    margin: 0 auto;
    box-sizing: border-box;
    padding: 28px 24px;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 16px;
  }

  .title {
    flex: 1;
    min-width: 0;
  }

  h1 {
    margin: 0 0 4px;
    font-size: 22px;
    font-weight: 600;
  }

  p {
    margin: 0;
  }

  .track {
    height: 8px;
    border-radius: 4px;
    background: var(--track);
    overflow: hidden;
  }

  .bar {
    height: 100%;
    background: var(--accent);
    transition: width 0.4s ease;
  }

  .indeterminate .bar {
    width: 30%;
    animation: slide 1.6s ease-in-out infinite;
  }

  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }

  .prompt {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 16px;
    padding: 20px 24px;
    border-radius: 12px;
    background: var(--warn-soft);
    border: 1px solid var(--warn-border);
    color: var(--warn-text);
  }

  .prompt svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
    flex-shrink: 0;
  }

  .prompt-text {
    flex: 1 1 260px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .strong {
    font-weight: 600;
    color: var(--text);
  }

  .prompt-input {
    flex: 1 1 320px;
    display: flex;
    gap: 8px;
  }

  .prompt-input input {
    flex: 1;
    min-width: 0;
    min-height: 44px;
    box-sizing: border-box;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    color: var(--text);
    font: inherit;
  }

  section {
    overflow: hidden;
  }
</style>
