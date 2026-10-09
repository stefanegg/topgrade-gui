//! Run history: a JSON summary saved next to each run's raw log
//! (`<stamp>-<os>[-dry-run].log` + `.json`), pruned to the newest runs.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::extractors::Extracted;
use crate::parser::{ParseEvent, Parser, StepStatus};

/// Runs kept on disk; older logs and summaries are deleted.
pub const KEEP_RUNS: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    /// Log file stem; identifies the run.
    pub id: String,
    pub started_at: String,
    pub duration_secs: f64,
    pub exit_code: Option<u32>,
    pub flags: Vec<String>,
    pub os: String,
    pub topgrade_version: Option<String>,
    pub steps: Vec<StepSummary>,
    /// Summary lines no step header matched (e.g. `config-update`).
    pub unmatched: Vec<UnmatchedResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepSummary {
    pub name: String,
    pub group: Option<String>,
    /// `None`: topgrade's summary had no line for this step.
    pub status: Option<StepStatus>,
    pub reason: Option<String>,
    pub duration_secs: f64,
    pub updates: Option<Extracted>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnmatchedResult {
    pub name: String,
    pub status: StepStatus,
    pub reason: Option<String>,
}

/// Builds a `RunSummary` from parser events as they stream.
pub struct Recorder {
    summary: RunSummary,
    started: Instant,
    /// Start time of each step still running.
    running: Vec<(usize, Instant)>,
}

impl Recorder {
    pub fn new(id: String, flags: Vec<String>) -> Self {
        Self {
            summary: RunSummary {
                id,
                started_at: chrono::Local::now().to_rfc3339(),
                duration_secs: 0.0,
                exit_code: None,
                flags,
                os: std::env::consts::OS.into(),
                topgrade_version: None,
                steps: Vec::new(),
                unmatched: Vec::new(),
            },
            started: Instant::now(),
            running: Vec::new(),
        }
    }

    pub fn on_event(&mut self, event: &ParseEvent) {
        match event {
            ParseEvent::StepStarted { index, name, group } => {
                self.end_steps(group.as_deref());
                debug_assert_eq!(*index, self.summary.steps.len());
                self.summary.steps.push(StepSummary {
                    name: name.clone(),
                    group: group.clone(),
                    status: None,
                    reason: None,
                    duration_secs: 0.0,
                    updates: None,
                });
                self.running.push((*index, Instant::now()));
            }
            ParseEvent::SummaryStarted { group } => self.end_steps(group.as_deref()),
            ParseEvent::StepResult {
                step_index,
                name,
                status,
                reason,
            } => match step_index.and_then(|i| self.summary.steps.get_mut(i)) {
                Some(step) => {
                    step.status = Some(*status);
                    step.reason = reason.clone();
                }
                None => self.summary.unmatched.push(UnmatchedResult {
                    name: name.clone(),
                    status: *status,
                    reason: reason.clone(),
                }),
            },
            ParseEvent::StepUpdates { index, updates } => {
                if let Some(step) = self.summary.steps.get_mut(*index) {
                    step.updates = Some(updates.clone());
                }
            }
            ParseEvent::Prompt { .. } => {}
        }
    }

    /// Ends running steps of `group`; a top-level boundary (`None`) ends all,
    /// since a nested run lives inside its top-level step (e.g. Toolbx).
    fn end_steps(&mut self, group: Option<&str>) {
        let steps = &mut self.summary.steps;
        self.running.retain(|&(i, started)| {
            let ends = group.is_none() || steps[i].group.as_deref() == group;
            if ends {
                steps[i].duration_secs = started.elapsed().as_secs_f64();
            }
            !ends
        });
    }

    pub fn finish(
        mut self,
        exit_code: Option<u32>,
        topgrade_version: Option<String>,
    ) -> RunSummary {
        self.end_steps(None);
        self.summary.duration_secs = self.started.elapsed().as_secs_f64();
        self.summary.exit_code = exit_code;
        self.summary.topgrade_version = topgrade_version;
        self.summary
    }
}

pub fn summary_path(log_path: &Path) -> PathBuf {
    log_path.with_extension("json")
}

pub fn save(log_path: &Path, summary: &RunSummary) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(summary)?;
    fs::write(summary_path(log_path), json)
}

/// Deletes all but the newest `keep` runs. Log names start with a sortable
/// timestamp, so name order is age order.
pub fn prune(dir: &Path, keep: usize) {
    let mut logs = log_files(dir);
    logs.sort();
    let excess = logs.len().saturating_sub(keep);
    for log in &logs[..excess] {
        let _ = fs::remove_file(log);
        let _ = fs::remove_file(summary_path(log));
    }
}

/// Saved runs, newest first. Logs without a summary (from before history
/// existed, or the app quit mid-run) are summarized from the log itself,
/// without timing or exit code.
pub fn list(dir: &Path) -> Vec<RunSummary> {
    let mut runs: Vec<RunSummary> = log_files(dir)
        .iter()
        .filter_map(|log| match fs::read(summary_path(log)) {
            Ok(json) => serde_json::from_slice(&json).ok(),
            Err(_) => summarize_log(log),
        })
        .collect();
    runs.sort_by(|a, b| b.id.cmp(&a.id));
    runs
}

fn summarize_log(log: &Path) -> Option<RunSummary> {
    let id = log.file_stem()?.to_string_lossy().into_owned();
    // `2026-10-09T13-42-58-macos[-dry-run]`
    let stamp = chrono::NaiveDateTime::parse_from_str(id.get(..19)?, "%Y-%m-%dT%H-%M-%S").ok()?;
    let started_at = stamp
        .and_local_timezone(chrono::Local)
        .single()?
        .to_rfc3339();
    let flags = if id.ends_with("-dry-run") {
        vec!["--dry-run".to_string()]
    } else {
        vec![]
    };

    let mut parser = Parser::default();
    let mut recorder = Recorder::new(id.clone(), flags);
    for (_, event) in parser.feed(&fs::read(log).ok()?) {
        recorder.on_event(&event);
    }
    for event in parser.finish() {
        recorder.on_event(&event);
    }
    let mut summary = recorder.finish(None, None);
    summary.started_at = started_at;
    summary.duration_secs = 0.0;
    for step in &mut summary.steps {
        step.duration_secs = 0.0;
    }
    summary.os = id.split('-').nth(5).unwrap_or_default().to_string();
    Some(summary)
}

/// Log path for a run id, if it is a plain file name that exists in `dir`.
pub fn log_path(dir: &Path, id: &str) -> Option<PathBuf> {
    if id.is_empty() || id.contains(['/', '\\']) || id.starts_with('.') {
        return None;
    }
    let path = dir.join(format!("{id}.log"));
    path.is_file().then_some(path)
}

fn log_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext == "log"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("topgrade-gui-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn record(input: &[u8]) -> RunSummary {
        let mut parser = Parser::default();
        let mut recorder = Recorder::new("run".into(), vec![]);
        for (_, event) in parser.feed(input) {
            recorder.on_event(&event);
        }
        for event in parser.finish() {
            recorder.on_event(&event);
        }
        recorder.finish(Some(0), Some("17.12.3".into()))
    }

    #[test]
    fn records_fedora_run() {
        let summary = record(include_bytes!("../tests/fixtures/fedora-run.log"));
        assert_eq!(summary.steps.len(), 20);
        let toolbx_dnf = summary
            .steps
            .iter()
            .find(|s| s.group.is_some() && s.name == "System update")
            .unwrap();
        assert_eq!(toolbx_dnf.status, Some(StepStatus::Ok));
        assert_eq!(toolbx_dnf.updates.as_ref().unwrap().items.len(), 4);
        assert_eq!(summary.unmatched.len(), 1);
        assert_eq!(summary.exit_code, Some(0));
    }

    #[test]
    fn save_list_prune_round_trip() {
        let dir = temp_dir("history");
        for i in 0..15 {
            let id = format!("2026-10-{:02}T10-00-00-macos", i + 1);
            let log = dir.join(format!("{id}.log"));
            fs::write(&log, b"x").unwrap();
            let mut summary = record(b"");
            summary.id = id;
            save(&log, &summary).unwrap();
        }
        // Log without summary (older run / app quit mid-run): summarized
        // from the log itself.
        fs::copy(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/macos-run.log"),
            dir.join("2026-10-20T10-00-00-macos.log"),
        )
        .unwrap();

        prune(&dir, KEEP_RUNS);
        assert_eq!(log_files(&dir).len(), KEEP_RUNS);
        let runs = list(&dir);
        assert_eq!(runs.len(), KEEP_RUNS);
        assert_eq!(runs[0].id, "2026-10-20T10-00-00-macos");
        assert_eq!(runs[0].steps.len(), 22);
        assert_eq!(runs[0].os, "macos");
        assert_eq!(runs[0].exit_code, None);
        assert!(runs[0].started_at.starts_with("2026-10-20T10:00:00"));
        assert_eq!(runs[1].id, "2026-10-15T10-00-00-macos");
        assert_eq!(runs.last().unwrap().id, "2026-10-05T10-00-00-macos");
        assert!(!dir.join("2026-10-04T10-00-00-macos.json").exists());

        assert!(log_path(&dir, "2026-10-15T10-00-00-macos").is_some());
        assert!(log_path(&dir, "../etc/passwd").is_none());
        assert!(log_path(&dir, "2026-10-01T10-00-00-macos").is_none());
        fs::remove_dir_all(&dir).unwrap();
    }
}
