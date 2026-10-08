import type { IMarker } from "@xterm/xterm";

export type StepStatus = "ok" | "failed" | "ignored" | "skipped";
export type PromptKind = "retry" | "yesNo" | "password";

/** Events interleaved with output bytes on the run stream (see runner.rs / parser.rs). */
export type StreamEvent =
  | { kind: "started"; logPath: string | null }
  | { kind: "exited"; code: number | null }
  | { kind: "error"; message: string }
  | { kind: "stepStarted"; index: number; name: string }
  | { kind: "summaryStarted" }
  | {
      kind: "stepResult";
      stepIndex: number | null;
      name: string;
      status: StepStatus;
      reason: string | null;
    }
  | { kind: "prompt"; prompt: PromptKind };

export interface Step {
  name: string;
  /** "running" until the next step starts; "done" if no summary line matched. */
  state: "running" | "done" | StepStatus;
  reason: string | null;
  startedAt: number;
  endedAt: number | null;
  needsInput: boolean;
  marker: IMarker | null;
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
