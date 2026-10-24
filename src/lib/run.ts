import type { IMarker } from "@xterm/xterm";
import type { Theme } from "./theme";

export type StepStatus = "ok" | "failed" | "ignored" | "skipped";
export type PromptKind = "retry" | "yesNo" | "password";

/** Events interleaved with output bytes on the run stream (see runner.rs / parser.rs). */
export type StreamEvent =
  | { kind: "started"; logPath: string | null }
  | { kind: "exited"; code: number | null }
  | { kind: "error"; message: string }
  | { kind: "stepStarted"; index: number; name: string; group: string | null }
  | { kind: "summaryStarted"; group: string | null }
  | {
      kind: "stepResult";
      stepIndex: number | null;
      name: string;
      status: StepStatus;
      reason: string | null;
    }
  | { kind: "prompt"; prompt: PromptKind }
  | {
      kind: "stepUpdates";
      index: number;
      items: UpdateItem[];
      unlistedCount: number | null;
    };

export interface UpdateItem {
  name: string;
  from: string | null;
  to: string | null;
}

export interface Step {
  name: string;
  /** Nested topgrade run (e.g. inside Toolbx) this step belongs to. */
  group: string | null;
  /** "running" until the next step starts; "done" if no summary line matched. */
  state: "running" | "done" | StepStatus;
  reason: string | null;
  startedAt: number;
  endedAt: number | null;
  needsInput: boolean;
  marker: IMarker | null;
  updates: UpdateItem[];
  /** Count from tools that don't list names (npm, incl. dependencies). */
  unlistedCount: number | null;
}

export interface RunTotals {
  updated: number;
  stepsWithUpdates: number;
  failed: number;
}

/** Totals count listed items only; unlisted counts (npm's "changed 1122
 *  packages", mostly dependencies) would drown them out. */
export function runTotals(steps: Step[]): RunTotals {
  const withUpdates = steps.filter((s) => s.updates.length > 0);
  return {
    updated: withUpdates.reduce((n, s) => n + s.updates.length, 0),
    stepsWithUpdates: withUpdates.length,
    failed: steps.filter((s) => s.state === "failed").length,
  };
}

export interface UnmatchedResult {
  name: string;
  status: StepStatus;
  reason: string | null;
}

export function formatDuration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return `${s}s`;
  return `${Math.floor(s / 60)}m ${String(s % 60).padStart(2, "0")}s`;
}

/** Saved run (history.rs `RunSummary`). */
export interface RunSummary {
  id: string;
  startedAt: string;
  durationSecs: number;
  exitCode: number | null;
  flags: string[];
  os: string;
  topgradeVersion: string | null;
  steps: {
    name: string;
    group: string | null;
    status: StepStatus | null;
    reason: string | null;
    durationSecs: number;
    updates: { items: UpdateItem[]; unlistedCount: number | null } | null;
  }[];
  unmatched: UnmatchedResult[];
}

export function summaryTotals(run: RunSummary): RunTotals {
  const withUpdates = run.steps.filter((s) => (s.updates?.items.length ?? 0) > 0);
  return {
    updated: withUpdates.reduce((n, s) => n + (s.updates?.items.length ?? 0), 0),
    stepsWithUpdates: withUpdates.length,
    failed: run.steps.filter((s) => s.status === "failed").length,
  };
}

export function isDryRun(run: RunSummary): boolean {
  return run.flags.includes("--dry-run") || run.flags.includes("-n");
}

/** Whether topgrade got as far as its closing summary; a stopped or crashed
 *  run has no step results. Mirrors `RunSummary::is_finished` in history.rs. */
export function isFinished(run: RunSummary): boolean {
  return run.steps.some((s) => s.status !== null) || run.unmatched.length > 0;
}

export function formatDate(iso: string): string {
  return new Date(iso).toLocaleString(undefined, {
    weekday: "short",
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function formatAgo(iso: string, now: number): string {
  const minutes = Math.floor((now - new Date(iso).getTime()) / 60000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

/** settings.rs `Settings`. */
export interface Settings {
  remindAfterDays: number;
  autostart: boolean;
  cleanup: boolean;
  assumeYes: boolean;
  fontSize: number;
  autoCheckUpdates: boolean;
  theme: Theme;
}

/** updater.rs `UpdateInfo`. */
export interface UpdateInfo {
  version: string;
  notes: string | null;
}
