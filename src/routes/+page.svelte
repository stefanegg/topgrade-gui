<script lang="ts">
  import { onMount } from "svelte";
  import { invoke, Channel } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";
  import Sidebar from "$lib/Sidebar.svelte";
  import HistoryList from "$lib/HistoryList.svelte";
  import SettingsPanel from "$lib/SettingsPanel.svelte";
  import {
    formatAgo,
    formatDate,
    isDryRun,
    runTotals,
    formatDuration,
    type RunSummary,
    type Settings,
    type Step,
    type StreamEvent,
    type UpdateInfo,
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
  let runStartedAt = 0;
  /** How the live run ended; shown as a banner and a footer in the terminal. */
  let outcome = $state<{ ok: boolean; text: string } | null>(null);

  let history = $state<RunSummary[]>([]);
  let showHistory = $state(false);
  /** Saved run being replayed read-only; `null` for live runs. */
  let viewing = $state<RunSummary | null>(null);
  let lastRun = $derived(history.find((r) => !isDryRun(r)));

  let settings = $state<Settings | null>(null);
  let showSettings = $state(false);
  let version = $state("");

  let update = $state<UpdateInfo | null>(null);
  let updateDismissed = $state(false);
  let updateMessage = $state("");
  let checkingUpdate = $state(false);
  let installing = $state(false);
  let downloadPercent = $state<number | null>(null);

  async function checkForUpdate() {
    checkingUpdate = true;
    updateMessage = "Checking…";
    try {
      update = await invoke<UpdateInfo | null>("check_for_update");
      updateDismissed = false;
      updateMessage = update ? `Version ${update.version} is available.` : "You're on the latest version.";
    } catch (err) {
      updateMessage = `Update check failed: ${err}`;
    } finally {
      checkingUpdate = false;
    }
  }

  async function installUpdate() {
    installing = true;
    downloadPercent = null;
    try {
      await invoke("install_update"); // restarts the app on success
      // Returns only if a run started during the download.
      installing = false;
      updateMessage = "Update installed. It takes effect when the app restarts after this run.";
      status = updateMessage;
    } catch (err) {
      installing = false;
      updateMessage = `Update failed: ${err}`;
      status = updateMessage;
    }
  }

  async function loadSettings() {
    settings = await invoke<Settings>("get_settings").catch(() => null);
    if (settings) {
      cleanup = settings.cleanup;
      assumeYes = settings.assumeYes;
      setFontSize(settings.fontSize, false);
    }
  }

  const MIN_FONT = 8;
  const MAX_FONT = 32;
  let saveFontTimer: ReturnType<typeof setTimeout> | undefined;

  function setFontSize(size: number, save = true) {
    size = Math.min(MAX_FONT, Math.max(MIN_FONT, Math.round(size)));
    if (term.options.fontSize !== size) {
      term.options.fontSize = size;
      fit.fit();
    }
    if (!save) return;
    clearTimeout(saveFontTimer);
    saveFontTimer = setTimeout(() => updateSettings({ fontSize: size }), 300);
  }

  /** Shift + wheel zooms the terminal font (like VS Code's Ctrl + wheel). */
  function onTermWheel(e: WheelEvent) {
    if (!e.shiftKey) return;
    e.preventDefault();
    e.stopPropagation();
    // WebKit may turn shift + wheel into horizontal scrolling.
    const delta = e.deltaY || e.deltaX;
    if (delta) setFontSize((term.options.fontSize ?? 13) + (delta < 0 ? 1 : -1));
  }

  function updateSettings(patch: Partial<Settings>) {
    if (!settings) return;
    settings = { ...settings, ...patch };
    invoke("set_settings", { settings }).catch((err) => (status = `Couldn't save settings: ${err}`));
  }

  /** Tray "Run now": start with the saved flags unless a run is active. */
  function runFromTray() {
    if (running) return;
    showHistory = false;
    showSettings = false;
    dryRun = false;
    run();
  }

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
        runStartedAt = Date.now();
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
        if (!viewing) {
          waitingForInput = true;
          invoke("set_waiting_input", { waiting: true, password: e.prompt === "password" }).catch(() => {});
        }
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
        showOutcome(e);
        loadHistory();
        break;
    }
  }

  function showOutcome(e: StreamEvent & { kind: "exited" | "error" }) {
    const t = runTotals(steps);
    const took = formatDuration(Date.now() - runStartedAt);
    const failed = t.failed > 0 ? ` · ${t.failed} failed` : "";
    const updated = t.updated > 0 ? `${t.updated} updated` : "nothing updated";
    let text: string;
    if (e.kind === "error") text = `Run failed: ${e.message}`;
    else if (e.code !== 0) text = `Run ended with exit code ${e.code} · ${took}${failed}`;
    else text = `Run finished · ${updated}${failed} · ${took}`;
    const ok = e.kind === "exited" && e.code === 0 && t.failed === 0;
    outcome = { ok, text };
    const color = ok ? "32" : "31";
    term.write(`\r\n\x1b[1;${color}m━━ ${text} ━━\x1b[0m\r\n`);
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

  let waitingForInput = false;

  function clearInputRequests() {
    if (waitingForInput) {
      waitingForInput = false;
      invoke("set_waiting_input", { waiting: false, password: false }).catch(() => {});
    }
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
    outcome = null;
    term.reset();
    steps = [];
    unmatched = [];
    degraded = false;
    logPath = null;
    status = "Idle";
  }

  async function start(command: string, args: Record<string, unknown>) {
    outcome = null;
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
    termEl.addEventListener("wheel", onTermWheel, { capture: true, passive: false });

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
    loadSettings();
    invoke<string>("app_version").then((v) => (version = v)).catch(() => {});
    const unlisten = [
      listen("run-requested", runFromTray),
      listen<UpdateInfo>("update-available", (e) => {
        update = e.payload;
        updateDismissed = false;
      }),
      listen("update-requested", async () => {
        showHistory = false;
        await checkForUpdate();
        if (!update) showSettings = true;
      }),
      listen<{ downloaded: number; total: number | null }>("update-progress", (e) => {
        const { downloaded, total } = e.payload;
        downloadPercent = total ? Math.round((downloaded / total) * 100) : null;
      }),
    ];
    return () => {
      unlisten.forEach((u) => u.then((stop) => stop()));
      clearInterval(clock);
      observer.disconnect();
      termEl.removeEventListener("wheel", onTermWheel, { capture: true });
      clearTimeout(saveFontTimer);
      term.dispose();
    };
  });
</script>

<main>
  <header>
    <button class="primary" onclick={run} disabled={running}>Run</button>
    <button onclick={stop} disabled={!running || viewing !== null}>Stop</button>
    <label title="Also delete temporary files and old leftovers after updating (topgrade --cleanup)">
      <input
        type="checkbox"
        bind:checked={cleanup}
        disabled={running}
        onchange={(e) => updateSettings({ cleanup: e.currentTarget.checked })}
      /> Clean up afterwards
    </label>
    <label title="Answer yes to confirmation questions so the run doesn't wait for you (topgrade -y)">
      <input
        type="checkbox"
        bind:checked={assumeYes}
        disabled={running}
        onchange={(e) => updateSettings({ assumeYes: e.currentTarget.checked })}
      /> Don't ask for confirmation
    </label>
    <label title="Only show what would be updated, without changing anything (topgrade --dry-run)">
      <input type="checkbox" bind:checked={dryRun} disabled={running} /> Preview only (dry run)
    </label>
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
    <button
      onclick={() => {
        showHistory = !showHistory;
        showSettings = false;
      }}
      disabled={running}
      class:active={showHistory}
    >
      History
    </button>
    <button
      onclick={() => {
        showSettings = !showSettings;
        showHistory = false;
      }}
      class:active={showSettings}
      disabled={!settings}
    >
      Settings
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
  {#if update && !updateDismissed}
    <div class="update">
      Topgrade GUI {update.version} is available.
      {#if installing}
        {downloadPercent === null ? "Installing…" : `Downloading… ${downloadPercent}%`}
      {:else}
        <span class="actions">
          <button class="primary" onclick={installUpdate} disabled={running} title={running ? "Wait for the run to finish" : ""}>
            Install and restart
          </button>
          <button onclick={() => (updateDismissed = true)}>Later</button>
        </span>
      {/if}
    </div>
  {/if}
  {#if viewing}
    <div class="viewing">
      Viewing run from {formatDate(viewing.startedAt)}
      {#if isDryRun(viewing)}(dry run){/if} · read-only
      <button onclick={closeRun}>Close</button>
    </div>
  {/if}
  {#if outcome}
    <div class="outcome" class:ok={outcome.ok} class:bad={!outcome.ok}>
      <span>{outcome.ok ? "✓" : "✗"} {outcome.text}</span>
      <button onclick={() => (outcome = null)}>Dismiss</button>
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
    {#if showSettings && settings}
      <SettingsPanel
        {settings}
        {version}
        {update}
        {updateMessage}
        {checkingUpdate}
        oncheckupdate={checkForUpdate}
        oninstallupdate={installUpdate}
        {installing}
        onchange={updateSettings} onclose={() => (showSettings = false)} />
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

  .outcome {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    font-weight: bold;
    border-bottom: 1px solid;
  }

  .outcome button {
    margin-left: auto;
    font-weight: normal;
  }

  .outcome.ok {
    background: #1d3524;
    color: #7fe09a;
    border-color: #2f6140;
  }

  .outcome.bad {
    background: #3a2222;
    color: #f08a84;
    border-color: #6b3030;
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

  .update {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: #1d3524;
    color: #9fdcb0;
    border-bottom: 1px solid #2f6140;
  }

  .update .actions {
    margin-left: auto;
    display: flex;
    gap: 8px;
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
