<script lang="ts">
  import { onMount } from "svelte";
  import { invoke, Channel } from "@tauri-apps/api/core";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";

  type RunEvent =
    | { kind: "started" }
    | { kind: "exited"; code: number | null }
    | { kind: "error"; message: string };

  let termEl: HTMLDivElement;
  let term: Terminal;
  let fit: FitAddon;

  let running = $state(false);
  let status = $state("Idle");
  let cleanup = $state(false);
  let assumeYes = $state(false);
  let dryRun = $state(false);

  function flags(): string[] {
    const f: string[] = [];
    if (cleanup) f.push("--cleanup");
    if (assumeYes) f.push("-y");
    if (dryRun) f.push("--dry-run");
    return f;
  }

  async function run() {
    term.reset();
    fit.fit();

    const onOutput = new Channel<ArrayBuffer>();
    onOutput.onmessage = (data) => term.write(new Uint8Array(data));

    const onEvent = new Channel<RunEvent>();
    onEvent.onmessage = (e) => {
      if (e.kind === "started") {
        running = true;
        status = "Running…";
      } else if (e.kind === "exited") {
        running = false;
        status = e.code === 0 ? "Finished" : `Exited with code ${e.code}`;
      } else {
        running = false;
        status = `Error: ${e.message}`;
      }
    };

    try {
      await invoke("start_run", {
        flags: flags(),
        cols: term.cols,
        rows: term.rows,
        onOutput,
        onEvent,
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
      if (running) invoke("write_input", { data }).catch(() => {});
    });
    term.onResize(({ cols, rows }) => {
      invoke("resize_pty", { cols, rows }).catch(() => {});
    });

    const observer = new ResizeObserver(() => fit.fit());
    observer.observe(termEl);
    return () => {
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
  </header>
  <div class="terminal" bind:this={termEl}></div>
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

  .terminal {
    flex: 1;
    min-height: 0;
    padding: 6px;
  }
</style>
