//! What topgrade would update on this machine: the steps of a background
//! `topgrade --dry-run`, cached as `sources.json` in the app data dir. A dry
//! run changes nothing and asks for no password; it only detects the tools.

use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use portable_pty::{native_pty_system, PtySize};
use serde::{Deserialize, Serialize};

use crate::parser::{ParseEvent, Parser};
use crate::runner;

const FILE: &str = "sources.json";

/// A dry run takes seconds; anything longer is stuck (e.g. an unexpected prompt).
const TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sources {
    /// RFC 3339.
    pub detected_at: String,
    /// Top-level step names in topgrade's order.
    pub names: Vec<String>,
}

pub fn load(dir: &Path) -> Option<Sources> {
    serde_json::from_slice(&fs::read(dir.join(FILE)).ok()?).ok()
}

pub fn save(dir: &Path, sources: &Sources) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(dir.join(FILE), serde_json::to_vec_pretty(sources)?)
}

/// Runs `topgrade --dry-run` in a PTY (like a real run, so detection matches)
/// and returns the steps it would run. Blocks for the length of the run.
pub fn detect() -> Result<Sources, String> {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 40,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;
    let mut child = pair
        .slave
        .spawn_command(runner::build_command(&["--dry-run".to_string()]))
        .map_err(|e| e.to_string())?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut output = Vec::new();
        // Linux reports EIO at the end of a PTY; what was read is kept.
        let _ = reader.read_to_end(&mut output);
        let _ = tx.send(output);
    });
    let output = match rx.recv_timeout(TIMEOUT) {
        Ok(output) => output,
        Err(_) => {
            let _ = child.kill();
            return Err("topgrade --dry-run took too long".into());
        }
    };
    let _ = child.wait();
    drop(pair.master);

    let names = step_names(&output);
    if names.is_empty() {
        return Err("topgrade --dry-run listed no steps".into());
    }
    Ok(Sources {
        detected_at: chrono::Local::now().to_rfc3339(),
        names,
    })
}

/// Top-level steps in `output`, in order, without repeats. Nested runs
/// (inside Toolbx) belong to their top-level step.
fn step_names(output: &[u8]) -> Vec<String> {
    let mut parser = Parser::default();
    let mut events: Vec<ParseEvent> = parser.feed(output).into_iter().map(|(_, e)| e).collect();
    events.extend(parser.finish());
    let mut names: Vec<String> = Vec::new();
    for event in events {
        if let ParseEvent::StepStarted {
            name, group: None, ..
        } = event
        {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_of_macos_dry_run() {
        let names = step_names(include_bytes!("../tests/fixtures/macos-dry-run.log"));
        assert_eq!(
            names.first().map(String::as_str),
            Some("macOS system update")
        );
        assert!(names.iter().any(|n| n == "Brew - Cask"));
        assert!(names.iter().any(|n| n == "Claude Code"));
        assert!(!names.iter().any(|n| n == "Summary"));
    }

    #[test]
    fn nested_steps_are_left_out() {
        let names = step_names(include_bytes!("../tests/fixtures/fedora-run.log"));
        // The Toolbx run's own "System update" doesn't appear twice.
        assert_eq!(names.iter().filter(|n| *n == "System update").count(), 1);
        assert!(names.iter().any(|n| n == "Toolbx"));
    }

    #[test]
    fn cache_round_trip() {
        let dir = std::env::temp_dir().join(format!("topgrade-gui-sources-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(load(&dir), None);
        let sources = Sources {
            detected_at: "2026-10-24T12:00:00+02:00".into(),
            names: vec!["Flatpak".into()],
        };
        save(&dir, &sources).unwrap();
        assert_eq!(load(&dir), Some(sources));
        fs::remove_dir_all(&dir).unwrap();
    }
}
