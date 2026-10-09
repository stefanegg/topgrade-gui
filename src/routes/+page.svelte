<script lang="ts">
  import { onMount } from "svelte";
  import { invoke, Channel } from "@tauri-apps/api/core";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";
  import Sidebar from "$lib/Sidebar.svelte";
  import type { Step, StreamEvent, UnmatchedResult } from "$lib/run";

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
  let logPath = $state<string | null>(null);
  let receivedOutput = false;
  let summarySeen = false;
  let runIsDry = false;

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
        status = "Running…";
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
        status =
          e.kind === "error"
            ? `Error: ${e.message}`
            : e.code === 0
              ? "Finished"
              : `Exited with code ${e.code}`;
        degraded = receivedOutput && steps.length === 0;
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

  function clearInputRequests() {
    for (const step of steps) step.needsInput = false;
  }

  async function run() {
    term.reset();
    fit.fit();
    steps = [];
    unmatched = [];
    degraded = false;
    logPath = null;
    receivedOutput = false;
    summarySeen = false;
    runIsDry = dryRun;

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
      await invoke("start_run", {
        flags: flags(),
        cols: term.cols,
        rows: term.rows,
        onStream,
      });
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
      if (!running) return;
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
    <button onclick={stop} disabled={!running}>Stop</button>
    <label><input type="checkbox" bind:checked={cleanup} disabled={running} /> --cleanup</label>
    <label><input type="checkbox" bind:checked={assumeYes} disabled={running} /> -y</label>
    <label><input type="checkbox" bind:checked={dryRun} disabled={running} /> --dry-run</label>
    <span class="status">{status}</span>
    {#if logPath}
      <button onclick={() => revealItemInDir(logPath!)} title={logPath}>Show log</button>
    {/if}
  </header>
  {#if degraded}
    <div class="banner">
      Couldn't detect topgrade steps in the output — parsing degraded, showing raw output only.
    </div>
  {/if}
  <div class="body">
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

  .banner {
    padding: 6px 12px;
    background: #4a3b12;
    color: #f0d68a;
    border-bottom: 1px solid #6b5520;
  }

  .body {
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
