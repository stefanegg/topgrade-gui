<script lang="ts">
  import { onMount } from "svelte";
  import { invoke, Channel } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";
  import "../app.css";
  import Logo from "$lib/Logo.svelte";
  import Home from "$lib/Home.svelte";
  import RunProgress from "$lib/RunProgress.svelte";
  import Results from "$lib/Results.svelte";
  import HistoryList from "$lib/HistoryList.svelte";
  import SettingsPanel from "$lib/SettingsPanel.svelte";
  import WindowControls from "$lib/WindowControls.svelte";
  import { isMac } from "$lib/platform";
  import { applyTheme } from "$lib/theme";
  import {
    formatDate,
    isDryRun,
    isFinished,
    runTotals,
    formatDuration,
    type PromptKind,
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

  type View = "overview" | "history" | "settings";
  let view = $state<View>("overview");

  let running = $state(false);
  let stopping = $state(false);
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
  let runIsDry = $state(false);
  let runStartedAt = $state(0);
  let runEndedAt = $state<number | null>(null);
  /** Why the live run didn't end cleanly; null while running or when it did. */
  let problem = $state<string | null>(null);
  /** The question topgrade is waiting on, if any. */
  let prompt = $state<PromptKind | null>(null);
  /** Errors outside a run (couldn't start, couldn't save settings, …). */
  let notice = $state<string | null>(null);

  let history = $state<RunSummary[]>([]);
  /** Saved run being replayed read-only; `null` for live runs. It is a
   *  sub-page of `view` (History, or Overview's "See what changed"). */
  let viewing = $state<RunSummary | null>(null);
  let lastRun = $derived(history.find((r) => !isDryRun(r)));
  let lastUpdate = $derived(history.find((r) => !isDryRun(r) && isFinished(r)));

  let settings = $state<Settings | null>(null);
  let version = $state("");

  let update = $state<UpdateInfo | null>(null);
  let updateDismissed = $state(false);
  let updateMessage = $state("");
  let checkingUpdate = $state(false);
  let installing = $state(false);
  let downloadPercent = $state<number | null>(null);

  const TERMINAL_OPEN_KEY = "terminalOpen";
  let terminalOpen = $state(readTerminalOpen());

  type Phase = "home" | "running" | "results";
  let phase = $derived<Phase>(
    viewing ? "results" : running ? "running" : steps.length > 0 || runEndedAt !== null ? "results" : "home",
  );

  /** Step count of the last run of the same kind, for the progress estimate. */
  let expectedSteps = $derived(history.find((r) => isDryRun(r) === runIsDry)?.steps.length ?? null);

  function readTerminalOpen(): boolean {
    try {
      return localStorage.getItem(TERMINAL_OPEN_KEY) === "true";
    } catch {
      return false;
    }
  }

  function setTerminalOpen(open: boolean) {
    terminalOpen = open;
    try {
      localStorage.setItem(TERMINAL_OPEN_KEY, String(open));
    } catch {
      // Not remembered; fine.
    }
    if (open) {
      term.scrollToBottom();
      term.focus();
    } else {
      term.blur();
    }
  }

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
      notice = updateMessage;
    } catch (err) {
      installing = false;
      updateMessage = `Update failed: ${err}`;
      notice = updateMessage;
    }
  }

  async function loadSettings() {
    settings = await invoke<Settings>("get_settings").catch(() => null);
    if (settings) {
      cleanup = settings.cleanup;
      assumeYes = settings.assumeYes;
      setFontSize(settings.fontSize, false);
      applyTheme(settings.theme);
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
    if (patch.theme) applyTheme(patch.theme);
    invoke("set_settings", { settings }).catch((err) => (notice = `Couldn't save settings: ${err}`));
  }

  function setOptions(patch: { cleanup?: boolean; assumeYes?: boolean }) {
    if (patch.cleanup !== undefined) cleanup = patch.cleanup;
    if (patch.assumeYes !== undefined) assumeYes = patch.assumeYes;
    updateSettings(patch);
  }

  /** Switches page; leaving a saved run closes it. */
  function navigate(to: View) {
    if (viewing) {
      if (running) return; // still loading it
      closeRun();
    }
    view = to;
  }

  /** Back from a saved run to the page it was opened from. */
  function goBack() {
    if (viewing && !running) closeRun();
  }

  /** Mouse back button, Alt+Left, and Cmd+[ on macOS. */
  function onBackGesture(e: MouseEvent | KeyboardEvent) {
    const back =
      e instanceof MouseEvent
        ? e.button === 3
        : (e.altKey && e.key === "ArrowLeft") || (isMac && e.metaKey && e.key === "[");
    if (back && viewing) {
      e.preventDefault();
      goBack();
    }
  }

  /** Tray "Run now": start with the saved flags unless a run is active. */
  function runFromTray() {
    if (running) return;
    navigate("overview");
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
        if (!viewing) {
          prompt = e.prompt;
          waitingForInput = true;
          invoke("set_waiting_input", { waiting: true, password: e.prompt === "password" }).catch(() => {});
          // The password card takes focus itself; other questions are answered in the terminal.
          if (terminalOpen && e.prompt !== "password") term.focus();
        }
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
          break;
        }
        runEndedAt = Date.now();
        if (e.kind === "error") problem = `The run failed: ${e.message}`;
        else if (stopping) problem = "You stopped the run before it finished.";
        else if (e.code !== 0) problem = `The run ended with exit code ${e.code}.`;
        stopping = false;
        writeFooter(e);
        loadHistory();
        break;
    }
  }

  /** A coloured line at the end of the terminal output saying how the run went. */
  function writeFooter(e: StreamEvent & { kind: "exited" | "error" }) {
    const t = runTotals(steps);
    const took = formatDuration(Date.now() - runStartedAt);
    const failed = t.failed > 0 ? ` · ${t.failed} failed` : "";
    const updated = t.updated > 0 ? `${t.updated} updated` : "nothing updated";
    let text: string;
    if (e.kind === "error") text = `Run failed: ${e.message}`;
    else if (e.code !== 0) text = `Run ended with exit code ${e.code} · ${took}${failed}`;
    else text = `Run finished · ${updated}${failed} · ${took}`;
    const ok = e.kind === "exited" && e.code === 0 && t.failed === 0;
    term.write(`\r\n\x1b[1;${ok ? "32" : "31"}m━━ ${text} ━━\x1b[0m\r\n`);
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
    prompt = null;
    if (waitingForInput) {
      waitingForInput = false;
      invoke("set_waiting_input", { waiting: false, password: false }).catch(() => {});
    }
    for (const step of steps) step.needsInput = false;
  }

  function sendPassword(password: string) {
    clearInputRequests();
    invoke("write_input", { data: `${password}\r` }).catch(() => {});
  }

  async function run() {
    viewing = null;
    runIsDry = dryRun;
    await start("start_run", { flags: flags(), cols: term.cols, rows: term.rows });
  }

  function runFromHome(preview: boolean) {
    dryRun = preview;
    run();
  }

  async function replay(name: string) {
    viewing = null;
    runIsDry = name.includes("dry-run");
    view = "overview";
    await start("replay_fixture", { name });
  }

  async function openRun(saved: RunSummary, from: View) {
    view = from;
    viewing = saved;
    runIsDry = isDryRun(saved);
    await start("replay_run", { id: saved.id });
  }

  function resetRun() {
    term.reset();
    steps = [];
    unmatched = [];
    degraded = false;
    logPath = null;
    runEndedAt = null;
    problem = null;
  }

  function closeRun() {
    viewing = null;
    resetRun();
  }

  async function start(command: string, args: Record<string, unknown>) {
    resetRun();
    notice = null;
    fit.fit();
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
      if (terminalOpen) term.focus();
    } catch (err) {
      notice = `Couldn't start topgrade: ${err}`;
    }
  }

  async function stop() {
    stopping = true;
    await invoke("stop_run").catch(() => {});
  }

  function jumpTo(step: Step) {
    if (!terminalOpen) setTerminalOpen(true);
    if (step.marker && step.marker.line >= 0) term.scrollToLine(step.marker.line);
  }

  onMount(() => {
    term = new Terminal({
      convertEol: false,
      cursorBlink: true,
      fontFamily: "Menlo, 'DejaVu Sans Mono', monospace",
      fontSize: 13,
      scrollback: 50000,
      theme: { background: "#14181c" },
    });
    fit = new FitAddon();
    term.loadAddon(fit);
    term.open(termEl);
    fit.fit();
    termEl.addEventListener("wheel", onTermWheel, { capture: true, passive: false });
    window.addEventListener("mouseup", onBackGesture);
    window.addEventListener("keydown", onBackGesture);

    term.onData((data) => {
      if (!running || viewing) return;
      clearInputRequests();
      invoke("write_input", { data }).catch(() => {});
    });
    term.onResize(({ cols, rows }) => {
      invoke("resize_pty", { cols, rows }).catch(() => {});
    });

    // The terminal keeps its size while the drawer is closed, so the PTY
    // (and topgrade's line wrapping) doesn't change when it is toggled.
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
        await checkForUpdate();
        if (!update) navigate("settings");
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
      window.removeEventListener("mouseup", onBackGesture);
      window.removeEventListener("keydown", onBackGesture);
      clearTimeout(saveFontTimer);
      term.dispose();
    };
  });
</script>

<div class="app">
  <!-- Doubles as the title bar: drag to move, double-click to maximize. -->
  <header class="topbar" class:mac={isMac} data-tauri-drag-region>
    <div class="brand" data-tauri-drag-region>
      <Logo size={26} />
      <span data-tauri-drag-region>Topgrade GUI</span>
    </div>
    <nav aria-label="Main">
      <button aria-current={view === "overview" ? "page" : undefined} onclick={() => navigate("overview")}>
        Overview
        {#if running && view !== "overview"}<span class="dot" title="A run is in progress"></span>{/if}
      </button>
      <button aria-current={view === "history" ? "page" : undefined} onclick={() => navigate("history")}>
        History
      </button>
      <button
        aria-current={view === "settings" ? "page" : undefined}
        onclick={() => navigate("settings")}
        disabled={!settings}
      >
        Settings
      </button>
    </nav>
    {#if import.meta.env.DEV}
      <select
        class="dev"
        disabled={running}
        aria-label="Replay fixture"
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
    {#if !isMac}
      <WindowControls />
    {/if}
  </header>

  {#if update && !updateDismissed}
    <div class="banner info">
      <span>Topgrade GUI {update.version} is available.</span>
      {#if installing}
        <span>{downloadPercent === null ? "Installing…" : `Downloading… ${downloadPercent}%`}</span>
      {:else}
        <span class="banner-actions">
          <button class="btn primary" onclick={installUpdate} disabled={running} title={running ? "Wait for the run to finish" : ""}>
            Install and restart
          </button>
          <button class="btn" onclick={() => (updateDismissed = true)}>Later</button>
        </span>
      {/if}
    </div>
  {/if}
  {#if notice}
    <div class="banner warn">
      <span>{notice}</span>
      <span class="banner-actions"><button class="btn" onclick={() => (notice = null)}>Dismiss</button></span>
    </div>
  {/if}

  <main class="content">
    {#if view === "history" && !viewing}
      <HistoryList runs={history} {now} disabled={running} onopen={(run) => openRun(run, "history")} />
    {:else if view === "settings" && settings}
      <SettingsPanel
        {settings}
        {version}
        {update}
        {updateMessage}
        {checkingUpdate}
        oncheckupdate={checkForUpdate}
        oninstallupdate={installUpdate}
        {installing}
        onchange={updateSettings}
      />
    {:else if phase === "home"}
      <Home
        {lastRun}
        {lastUpdate}
        remindAfterDays={settings?.remindAfterDays ?? 7}
        {now}
        {cleanup}
        {assumeYes}
        onoptions={setOptions}
        onrun={runFromHome}
        onopenrun={(run) => openRun(run, "overview")}
      />
    {:else if phase === "running"}
      <RunProgress
        {steps}
        {now}
        startedAt={runStartedAt}
        dryRun={runIsDry}
        {expectedSteps}
        {prompt}
        {stopping}
        onstop={stop}
        onpassword={sendPassword}
        onshowterminal={() => setTerminalOpen(true)}
        onselect={jumpTo}
      />
    {:else}
      <Results
        {steps}
        {unmatched}
        {now}
        startedAt={viewing ? viewing.startedAt : new Date(runStartedAt).toISOString()}
        durationMs={viewing
          ? viewing.durationSecs * 1000 || null
          : runEndedAt !== null
            ? runEndedAt - runStartedAt
            : null}
        dryRun={runIsDry}
        saved={viewing !== null}
        loading={viewing !== null && running}
        problem={viewing
          ? !isFinished(viewing)
            ? "This run didn't finish: it was stopped before topgrade's summary, so some steps didn't run."
            : viewing.exitCode !== null && viewing.exitCode !== 0
              ? `The run ended with exit code ${viewing.exitCode}.`
              : null
          : problem}
        {degraded}
        {logPath}
        breadcrumb={viewing
          ? {
              parent: view === "history" ? "History" : "Overview",
              current: view === "history" ? formatDate(viewing.startedAt) : "Last run",
            }
          : null}
        ondone={closeRun}
        onshowlog={() => logPath && revealItemInDir(logPath)}
        onshowterminal={() => setTerminalOpen(true)}
        onselect={jumpTo}
      />
    {/if}
  </main>

  <section class="drawer" class:hidden={view !== "overview" && !viewing} aria-label="Terminal">
    <div class="drawer-bar">
      <button class="drawer-toggle" aria-expanded={terminalOpen} onclick={() => setTerminalOpen(!terminalOpen)}>
        <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true" class:open={terminalOpen}><path d="M18 15l-6-6-6 6" /></svg>
        Terminal output
      </button>
      <span class="muted drawer-hint">
        {#if viewing}
          Saved output, read-only
        {:else if running && terminalOpen}
          You can type here too · Shift + scroll to zoom
        {/if}
      </span>
      {#if logPath && phase !== "results"}
        <button class="link-button" onclick={() => revealItemInDir(logPath!)} title={logPath}>Show log file</button>
      {/if}
    </div>
    <div class="drawer-body" class:open={terminalOpen}>
      <div class="terminal" bind:this={termEl}></div>
    </div>
  </section>
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  .topbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 16px;
    padding: 10px 12px 10px 24px;
    user-select: none;
    -webkit-user-select: none;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    font-weight: 600;
    font-size: 15px;
  }

  /* Room for the macOS traffic lights (overlay title bar). Their vertical
     position (trafficLightPosition in tauri.macos.conf.json) is tuned to
     this header's height; change both together. */
  .topbar.mac {
    padding-left: 96px;
    padding-right: 24px;
  }

  nav {
    display: flex;
    gap: 4px;
    margin-right: auto;
  }

  nav button {
    position: relative;
    min-height: 36px;
    padding: 0 14px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--muted);
    font: inherit;
    cursor: pointer;
  }

  nav button:hover:not(:disabled) {
    color: var(--text);
    background: var(--surface-2);
  }

  nav button[aria-current="page"] {
    background: var(--accent-soft);
    color: var(--link);
    font-weight: 600;
  }

  nav button:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .dot {
    position: absolute;
    top: 6px;
    right: 4px;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent);
  }

  .dev {
    min-height: 32px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    font: inherit;
  }

  .banner {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    padding: 8px 24px;
    border-bottom: 1px solid;
  }

  .banner.info {
    background: var(--accent-soft);
    border-color: var(--border);
  }

  .banner.warn {
    background: var(--warn-soft);
    border-color: var(--warn-border);
    color: var(--warn-text);
  }

  .banner-actions {
    margin-left: auto;
    display: flex;
    gap: 8px;
  }

  .content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .drawer {
    flex-shrink: 0;
    border-top: 1px solid var(--border);
    background: var(--surface);
  }

  /* Collapsed rather than display: none, so xterm keeps its size (and
     the PTY its columns) on the History and Settings pages. */
  .drawer.hidden {
    height: 0;
    overflow: hidden;
    border: 0;
  }

  .drawer-bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 24px;
  }

  .drawer-toggle {
    min-height: 40px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }

  .drawer-toggle svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
    transition: transform 0.15s;
  }

  .drawer-toggle svg.open {
    transform: rotate(180deg);
  }

  .drawer-hint {
    flex: 1;
    font-size: 12px;
  }

  .drawer-body {
    height: 0;
    overflow: hidden;
    background: var(--term-bg);
  }

  .drawer-body.open {
    height: var(--drawer-height);
  }

  .terminal {
    height: var(--drawer-height);
    box-sizing: border-box;
    padding: 8px 12px;
  }

  .app {
    --drawer-height: min(380px, 45vh);
  }
</style>
