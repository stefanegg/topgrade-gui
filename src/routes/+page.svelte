<script lang="ts">
  import { onMount } from "svelte";
  import { invoke, Channel } from "@tauri-apps/api/core";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";
  import Sidebar from "$lib/Sidebar.svelte";
  import HistoryList from "$lib/HistoryList.svelte";
  import {
    formatAgo,
    formatDate,
    isDryRun,
    runTotals,
    type RunSummary,
    type Step,
    type StreamEvent,
    type UnmatchedResult,
  } from "$lib/run";

  let termEl: HTMLDivElement;
  let term: Terminal;
  let fit: FitAddon;

  let running = $state(false);
  let status = $state("Idle");
  let cleanup = $state(false);
  let assumeYes = $state(false);
  let dryRun = $state(false);

  let steps = $state<Step[]>([]);
  let unmatched = $state<UnmatchedResult[]>([]);
  let degraded = $state(false);
  let now = $state(Date.now());
  let totals = $derived(runTotals(steps));
  let logPath = $state<string | null>(null);
  let receivedOutput = false;
  let summarySeen = false;
  let runIsDry = false;

  let history = $state<RunSummary[]>([]);
  let showHistory = $state(false);
  /** Saved run being replayed read-only; `null` for live runs. */
  let viewing = $state<RunSummary | null>(null);
  let lastRun = $derived(history.find((r) => !isDryRun(r)));

  async function loadHistory() {
    history = await invoke<RunSummary[]>("list_runs").catch(() => []);
  }

  function flags(): string[] {
    const f: string[] = [];
    if (cleanup) f.push("--cleanup");
    if (assumeYes) f.push("-y");
    if (dryRun) f.push("--dry-run");
    return f;
  }

  /** Ends running steps of `group`; `null` (a top-level boundary) ends all,
   *  since a nested run lives inside its top-level step (e.g. Toolbx). */
  function finishRunningSteps(group: string | null) {
    for (const step of steps) {
      if (step.state === "running" && (group === null || step.group === group)) {
        step.state = "done";
        step.endedAt = Date.now();
      }
    }
  }

  function handleEvent(e: StreamEvent) {
    switch (e.kind) {
      case "started":
        logPath = e.logPath;
        running = true;
        status = viewing ? "Loading…" : "Running…";
        break;
      case "stepStarted": {
        finishRunningSteps(e.group);
        steps.push({
          name: e.name,
          group: e.group,
          state: "running",
          reason: null,
          startedAt: Date.now(),
          endedAt: null,
          needsInput: false,
          marker: term.registerMarker(0) ?? null,
          updates: [],
          unlistedCount: null,
        });
        break;
      }
      case "summaryStarted":
        finishRunningSteps(e.group);
        summarySeen = true;
        break;
      case "stepResult": {
        const step = e.stepIndex === null ? undefined : steps[e.stepIndex];
        if (step) {
          step.state = e.status;
          step.reason = e.reason;
        } else {
          unmatched.push({ name: e.name, status: e.status, reason: e.reason });
        }
        break;
      }
      case "stepUpdates": {
        const step = steps[e.index];
        if (step) {
          step.updates = e.items;
          step.unlistedCount = e.unlistedCount;
        }
        break;
      }
      case "prompt": {
        const step = steps.at(-1);
        if (step) step.needsInput = true;
        status = e.prompt === "password" ? "Waiting for password…" : "Waiting for input…";
        term.focus();
        break;
      }
      case "exited":
      case "error":
        finishRunningSteps(null);
        clearInputRequests();
        if (summarySeen) explainUnreported();
        running = false;
        degraded = receivedOutput && steps.length === 0;
        if (viewing) {
          applySavedDurations(viewing);
          status = "";
          break;
        }
        status =
          e.kind === "error"
            ? `Error: ${e.message}`
            : e.code === 0
              ? "Finished"
              : `Exited with code ${e.code}`;
        loadHistory();
        break;
    }
  }

  // topgrade omits a step from its summary when the step aborts during a dry
  // run (it can't continue without real command output) or is skipped
  // without `show_skipped`.
  function explainUnreported() {
    for (const step of steps) {
      if (step.state === "done" && !step.reason) {
        step.reason = runIsDry
          ? "Not reported by topgrade (step can't complete in a dry run)"
          : "Not reported in topgrade's summary";
      }
    }
  }

  /** Replays render instantly, so step times come from the saved summary. */
  function applySavedDurations(run: RunSummary) {
    steps.forEach((step, i) => {
      const saved = run.steps[i];
      // Logs saved before history existed have no timings.
      if (saved?.name !== step.name || saved.durationSecs === 0) return;
      step.startedAt = 0;
      step.endedAt = saved.durationSecs * 1000;
    });
  }

  function clearInputRequests() {
    for (const step of steps) step.needsInput = false;
  }

  async function run() {
    viewing = null;
    runIsDry = dryRun;
    await start("start_run", { flags: flags(), cols: term.cols, rows: term.rows });
  }

  async function replay(name: string) {
    viewing = null;
    runIsDry = name.includes("dry-run");
    await start("replay_fixture", { name });
  }

  async function openRun(saved: RunSummary) {
    showHistory = false;
    viewing = saved;
    runIsDry = isDryRun(saved);
    await start("replay_run", { id: saved.id });
  }

  function closeRun() {
    viewing = null;
    term.reset();
    steps = [];
    unmatched = [];
    degraded = false;
    logPath = null;
    status = "Idle";
  }

  async function start(command: string, args: Record<string, unknown>) {
    term.reset();
    fit.fit();
    steps = [];
    unmatched = [];
    degraded = false;
    logPath = null;
    receivedOutput = false;
    summarySeen = false;

    // Bytes and events share one channel to keep their order; xterm parses
    // writes asynchronously, so events are queued behind preceding output
    // (markers must land on the line the step header was written to).
    const onStream = new Channel<ArrayBuffer | StreamEvent>();
    onStream.onmessage = (msg) => {
      if (msg instanceof ArrayBuffer) {
        receivedOutput = true;
        term.write(new Uint8Array(msg));
      } else {
        term.write("", () => handleEvent(msg));
      }
    };

    try {
      await invoke(command, { ...args, onStream });
      term.focus();
    } catch (err) {
      status = `Failed to start: ${err}`;
    }
  }

  async function stop() {
    status = "Stopping…";
    await invoke("stop_run").catch(() => {});
  }

  function jumpTo(step: Step) {
    if (step.marker && step.marker.line >= 0) term.scrollToLine(step.marker.line);
    term.focus();
  }

  onMount(() => {
    term = new Terminal({
      convertEol: false,
      cursorBlink: true,
      fontFamily: "Menlo, 'DejaVu Sans Mono', monospace",
      fontSize: 13,
      scrollback: 50000,
      theme: { background: "#1e1e1e" },
    });
    fit = new FitAddon();
    term.loadAddon(fit);
    term.open(termEl);
    fit.fit();

    term.onData((data) => {
      if (!running || viewing) return;
      clearInputRequests();
      if (status.startsWith("Waiting")) status = "Running…";
      invoke("write_input", { data }).catch(() => {});
    });
    term.onResize(({ cols, rows }) => {
      invoke("resize_pty", { cols, rows }).catch(() => {});
    });

    const observer = new ResizeObserver(() => fit.fit());
    observer.observe(termEl);
    const clock = setInterval(() => (now = Date.now()), 1000);
    loadHistory();
    return () => {
      clearInterval(clock);
      observer.disconnect();
      term.dispose();
    };
  });
</script>

<main>
  <header>
    <button class="primary" onclick={run} disabled={running}>Run</button>
    <button onclick={stop} disabled={!running || viewing !== null}>Stop</button>
    <label><input type="checkbox" bind:checked={cleanup} disabled={running} /> --cleanup</label>
    <label><input type="checkbox" bind:checked={assumeYes} disabled={running} /> -y</label>
    <label><input type="checkbox" bind:checked={dryRun} disabled={running} /> --dry-run</label>
    {#if import.meta.env.DEV}
      <select
        disabled={running}
        onchange={(e) => {
          const select = e.currentTarget;
          if (select.value) replay(select.value);
          select.value = "";
        }}
      >
        <option value="">Replay fixture…</option>
        <option>macos-run.log</option>
        <option>fedora-run.log</option>
        <option>macos-dry-run.log</option>
      </select>
    {/if}
    <button onclick={() => (showHistory = !showHistory)} disabled={running} class:active={showHistory}>
      History
    </button>
    <span class="status">
      {#if status === "Idle" && lastRun}
        Last run: {formatAgo(lastRun.startedAt, now)}
      {:else}
        {status}
      {/if}
    </span>
    {#if logPath}
      <button onclick={() => revealItemInDir(logPath!)} title={logPath}>Show log</button>
    {/if}
  </header>
  {#if viewing}
    <div class="viewing">
      Viewing run from {formatDate(viewing.startedAt)}
      {#if isDryRun(viewing)}(dry run){/if} · read-only
      <button onclick={closeRun}>Close</button>
    </div>
  {/if}
  {#if steps.length > 0}
    <div class="totals" class:has-failed={totals.failed > 0}>
      {#if totals.updated > 0}
        <strong>{totals.updated}</strong> updated across {totals.stepsWithUpdates}
        {totals.stepsWithUpdates === 1 ? "step" : "steps"}
      {:else if running}
        Nothing updated yet
      {:else}
        Nothing updated
      {/if}
      {#if totals.failed > 0}
        · <span class="failed">{totals.failed} failed</span>
      {/if}
    </div>
  {/if}
  {#if degraded}
    <div class="banner">
      Couldn't detect topgrade steps in the output — parsing degraded, showing raw output only.
    </div>
  {/if}
  <div class="body">
    {#if showHistory}
      <HistoryList runs={history} {now} onopen={openRun} onclose={() => (showHistory = false)} />
    {/if}
    <Sidebar {steps} {unmatched} {now} onselect={jumpTo} />
    <div class="terminal" bind:this={termEl}></div>
  </div>
</main>

<style>
  :global(html, body) {
    margin: 0;
    height: 100%;
    background: #1e1e1e;
    color: #ddd;
    font-family: -apple-system, "Segoe UI", "Noto Sans", sans-serif;
    font-size: 13px;
  }

  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    background: #2a2a2a;
    border-bottom: 1px solid #3a3a3a;
  }

  label {
    display: flex;
    align-items: center;
    gap: 4px;
    font-family: Menlo, monospace;
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

  button.primary {
    background: #2f6fd6;
    border-color: #2f6fd6;
  }

  button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .status {
    margin-left: auto;
    color: #999;
  }

  .totals {
    padding: 6px 12px;
    background: #232a23;
    border-bottom: 1px solid #3a3a3a;
    color: #bbb;
  }

  .totals strong {
    color: #4caf50;
  }

  .totals.has-failed {
    background: #2e2222;
  }

  .totals .failed {
    color: #e5534b;
    font-weight: bold;
  }

  .viewing {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: #1f2a3a;
    color: #9ec2f0;
    border-bottom: 1px solid #2f4466;
  }

  .viewing button {
    margin-left: auto;
  }

  button.active {
    background: #555;
  }

  .banner {
    padding: 6px 12px;
    background: #4a3b12;
    color: #f0d68a;
    border-bottom: 1px solid #6b5520;
  }

  .body {
    position: relative;
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .terminal {
    flex: 1;
    min-width: 0;
    min-height: 0;
    padding: 6px;
  }
</style>
