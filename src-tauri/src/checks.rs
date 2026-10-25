//! Which updates are waiting, per topgrade step: the read-only "outdated"
//! command of each supported tool, cached as `checks.json` in the app data
//! dir. Steps without a check here are simply absent from the results, so
//! the UI can tell "not checked" from "up to date".
//!
//! Checks refresh package metadata, so they need the network and can take
//! many seconds. They run without a terminal and must never ask for a
//! password: stdin is closed and none of the commands used needs root.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::extractors::Update;

const FILE: &str = "checks.json";

/// dnf can take 10-30 s on a slow mirror; longer than this is stuck.
const TIMEOUT: Duration = Duration::from_secs(120);

/// What a check found for one step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Outcome {
    UpToDate,
    Waiting {
        items: Vec<Update>,
    },
    /// The tool ran but couldn't tell (offline, repo error, timeout).
    Failed {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Checks {
    /// RFC 3339.
    pub checked_at: String,
    /// By topgrade step name.
    pub results: BTreeMap<String, Outcome>,
}

impl Checks {
    /// Number of waiting updates over all steps.
    pub fn waiting(&self) -> usize {
        self.results
            .values()
            .map(|o| match o {
                Outcome::Waiting { items } => items.len(),
                _ => 0,
            })
            .sum()
    }
}

pub fn load(dir: &Path) -> Option<Checks> {
    serde_json::from_slice(&fs::read(dir.join(FILE)).ok()?).ok()
}

pub fn save(dir: &Path, checks: &Checks) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(dir.join(FILE), serde_json::to_vec_pretty(checks)?)
}

/// What a command printed and how it ended.
struct Output {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

type Parse = fn(&Output) -> Result<Vec<Update>, String>;

struct Spec {
    /// Topgrade step names this check answers for.
    steps: &'static [&'static str],
    /// Program and arguments; no quoting needed, so keep them plain.
    command: &'static str,
    parse: Parse,
}

const SPECS: &[Spec] = &[
    Spec {
        steps: &["System update"],
        command: "dnf check-update -q",
        parse: dnf,
    },
    Spec {
        // Without --user/--system it covers every installation.
        steps: &[
            "Flatpak",
            "Flatpak User Packages",
            "Flatpak System Packages",
        ],
        command: "flatpak remote-ls --updates --columns=application,name,version",
        parse: flatpak,
    },
    Spec {
        steps: &["Brew"],
        command: "brew outdated --formula --json=v2",
        parse: brew_formulae,
    },
    Spec {
        steps: &["Brew - Cask"],
        command: "brew outdated --cask --json=v2",
        parse: brew_casks,
    },
    Spec {
        steps: &["Node Package Manager"],
        command: "npm outdated -g --json",
        parse: npm,
    },
    Spec {
        steps: &["rustup"],
        command: "rustup check",
        parse: rustup,
    },
];

/// Runs the checks for those of `steps` that have one, in parallel. Blocks
/// until the slowest one is done. A tool that isn't installed is skipped.
pub fn run(steps: &[String]) -> Checks {
    let mut handles = Vec::new();
    for spec in SPECS {
        let wanted: Vec<&String> = steps
            .iter()
            .filter(|s| spec.steps.contains(&s.as_str()))
            .collect();
        if wanted.is_empty() {
            continue;
        }
        let names: Vec<String> = wanted.into_iter().cloned().collect();
        handles.push(thread::spawn(move || {
            let outcome = execute(spec);
            names
                .into_iter()
                .filter_map(|n| outcome.clone().map(|o| (n, o)))
                .collect::<Vec<_>>()
        }));
    }
    let mut results = BTreeMap::new();
    for handle in handles {
        if let Ok(found) = handle.join() {
            results.extend(found);
        }
    }
    Checks {
        checked_at: chrono::Local::now().to_rfc3339(),
        results,
    }
}

/// `None` when the tool isn't installed.
fn execute(spec: &Spec) -> Option<Outcome> {
    let output = match spawn(spec.command) {
        Ok(output) => output,
        Err(message) => return Some(Outcome::Failed { message }),
    };
    if output.code == Some(127) {
        return None;
    }
    Some(match (spec.parse)(&output) {
        Ok(items) if items.is_empty() => Outcome::UpToDate,
        Ok(items) => Outcome::Waiting { items },
        Err(message) => Outcome::Failed { message },
    })
}

/// Runs `command` through the login shell, like topgrade itself (so PATH
/// includes Homebrew, ~/.cargo/bin and so on), in English, without a terminal.
fn spawn(command: &str) -> Result<Output, String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let script = format!("export LANG=C LC_ALL=C NO_COLOR=1; exec {command}");
    let mut cmd = Command::new(shell);
    cmd.args(["-l", "-c", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(home) = std::env::var_os("HOME") {
        cmd.current_dir(home);
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let mut stdout = child.stdout.take().ok_or("no output")?;
    let mut stderr = child.stderr.take().ok_or("no output")?;
    let (tx, rx) = mpsc::channel();
    let out_tx = tx.clone();
    thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        let _ = out_tx.send((true, s));
    });
    thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        let _ = tx.send((false, s));
    });
    let (mut out, mut err) = (None, None);
    let deadline = std::time::Instant::now() + TIMEOUT;
    while out.is_none() || err.is_none() {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(left) {
            Ok((true, s)) => out = Some(s),
            Ok((false, s)) => err = Some(s),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("took too long".into());
            }
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    Ok(Output {
        code: status.code(),
        stdout: out.unwrap_or_default(),
        stderr: err.unwrap_or_default(),
    })
}

/// The last meaningful line of stderr, for "couldn't check" messages.
fn failure(output: &Output) -> String {
    let last_line = |text: &str| {
        text.lines()
            .map(str::trim)
            .rfind(|l| !l.is_empty())
            .map(str::to_string)
    };
    let line = last_line(&output.stderr)
        .or_else(|| last_line(&output.stdout))
        .unwrap_or_else(|| "the command failed".into());
    let line = line.strip_prefix("error: ").unwrap_or(&line);
    match output.code {
        Some(code) => format!("{line} (exit {code})"),
        None => line.to_string(),
    }
}

/// `dnf check-update`: exit 100 with a table of `name.arch version repo`
/// when updates exist, 0 when none. The table follows an "Upgrades" header
/// (dnf5) or starts right away (dnf4); obsoletes come after a second header.
fn dnf(output: &Output) -> Result<Vec<Update>, String> {
    match output.code {
        Some(0) => return Ok(Vec::new()),
        Some(100) => {}
        _ => return Err(failure(output)),
    }
    let mut items = Vec::new();
    for line in output.stdout.lines() {
        let line = line.trim();
        if line.to_lowercase().starts_with("obsoleting") {
            break;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if let [package, version, _repo] = fields[..] {
            let name = package
                .rsplit_once('.')
                .map_or(package, |(name, _arch)| name);
            items.push(Update::new(name, None, Some(version)));
        }
    }
    Ok(items)
}

/// `flatpak remote-ls --updates --columns=application,name,version`: one
/// tab-separated row per update; the same app can appear per installation.
fn flatpak(output: &Output) -> Result<Vec<Update>, String> {
    if output.code != Some(0) {
        return Err(failure(output));
    }
    let mut items: Vec<Update> = Vec::new();
    for line in output.stdout.lines() {
        let mut cols = line.split('\t').map(str::trim);
        let (Some(id), name, version) = (
            cols.next().filter(|c| !c.is_empty()),
            cols.next(),
            cols.next(),
        ) else {
            continue;
        };
        let name = name.filter(|n| !n.is_empty()).unwrap_or(id);
        if items.iter().any(|i| i.name == name) {
            continue;
        }
        items.push(Update::new(name, None, version.filter(|v| !v.is_empty())));
    }
    Ok(items)
}

fn brew_formulae(output: &Output) -> Result<Vec<Update>, String> {
    brew(output, "formulae")
}

fn brew_casks(output: &Output) -> Result<Vec<Update>, String> {
    brew(output, "casks")
}

/// `brew outdated --json=v2`: `installed_versions` is a list for formulae
/// and a string or list for casks.
fn brew(output: &Output, key: &str) -> Result<Vec<Update>, String> {
    if output.code != Some(0) {
        return Err(failure(output));
    }
    let json: serde_json::Value =
        serde_json::from_str(&output.stdout).map_err(|_| "unexpected brew output".to_string())?;
    let rows = json
        .get(key)
        .and_then(|v| v.as_array())
        .ok_or("unexpected brew output")?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let name = row.get("name")?.as_str()?;
            let from = match row.get("installed_versions")? {
                serde_json::Value::String(s) => Some(s.as_str()),
                serde_json::Value::Array(a) => a.last().and_then(|v| v.as_str()),
                _ => None,
            };
            Some(Update::new(
                name,
                from,
                row.get("current_version")?.as_str(),
            ))
        })
        .collect())
}

/// `npm outdated -g --json`: exit 1 with `{"pkg": {"current", "latest"}}`
/// when something is outdated, exit 0 with `{}` (or nothing) otherwise.
fn npm(output: &Output) -> Result<Vec<Update>, String> {
    if !matches!(output.code, Some(0 | 1)) {
        return Err(failure(output));
    }
    if output.stdout.trim().is_empty() {
        return if output.code == Some(0) {
            Ok(Vec::new())
        } else {
            Err(failure(output))
        };
    }
    let json: serde_json::Value =
        serde_json::from_str(&output.stdout).map_err(|_| "unexpected npm output".to_string())?;
    let packages = json.as_object().ok_or("unexpected npm output")?;
    Ok(packages
        .iter()
        .map(|(name, info)| {
            let text = |key| info.get(key).and_then(|v| v.as_str());
            Update::new(name, text("current"), text("latest"))
        })
        .collect())
}

/// `rustup check`: a line per toolchain, `<name> - Update available : old
/// (hash date) -> new (hash date)` or `<name> - up to date : version`.
fn rustup(output: &Output) -> Result<Vec<Update>, String> {
    if !matches!(output.code, Some(0 | 100)) {
        return Err(failure(output));
    }
    let mut items = Vec::new();
    for line in output.stdout.lines() {
        let Some((name, rest)) = line.split_once(" - Update available :") else {
            continue;
        };
        let version = |s: &str| s.split_whitespace().next().map(str::to_string);
        let (from, to) = match rest.split_once("->") {
            Some((from, to)) => (version(from), version(to)),
            None => (None, version(rest)),
        };
        items.push(Update::new(name.trim(), from.as_deref(), to.as_deref()));
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(code: i32, stdout: &str, stderr: &str) -> Output {
        Output {
            code: Some(code),
            stdout: stdout.into(),
            stderr: stderr.into(),
        }
    }

    fn names(items: &[Update]) -> Vec<&str> {
        items.iter().map(|i| i.name.as_str()).collect()
    }

    #[test]
    fn dnf_lists_upgrades() {
        let text = include_str!("../tests/fixtures/checks/dnf.txt");
        let items = dnf(&out(100, text, "")).unwrap();
        assert_eq!(items.len(), 21);
        assert_eq!(
            items[0],
            Update::new("cockpit-bridge", None, Some("370-1.fc44"))
        );
        // The architecture is dropped, dashes and dots in names are kept.
        assert!(names(&items).contains(&"pipewire-jack-audio-connection-kit-libs"));
    }

    #[test]
    fn dnf_stops_at_obsoletes_and_handles_dnf4() {
        let text = "\nfoo.x86_64   1.2   updates\nbar.noarch   3.4   fedora\n\nObsoleting Packages\nbaz.x86_64   9   updates\n    old.x86_64   1   @system\n";
        let items = dnf(&out(100, text, "")).unwrap();
        assert_eq!(names(&items), ["foo", "bar"]);
    }

    #[test]
    fn dnf_nothing_waiting_and_errors() {
        assert!(dnf(&out(0, "", "")).unwrap().is_empty());
        let err = dnf(&out(
            1,
            "",
            "Failed to download metadata for repo 'updates'\n",
        ))
        .unwrap_err();
        assert_eq!(
            err,
            "Failed to download metadata for repo 'updates' (exit 1)"
        );
    }

    #[test]
    fn flatpak_rows_without_duplicates() {
        let text = include_str!("../tests/fixtures/checks/flatpak-updates.tsv");
        let items = flatpak(&out(0, text, "")).unwrap();
        assert_eq!(
            items,
            [
                Update::new("Firefox", None, Some("150.0.2")),
                Update::new("Mesa", None, Some("26.1.3")),
            ]
        );
        assert!(flatpak(&out(0, "", "")).unwrap().is_empty());
    }

    #[test]
    fn flatpak_offline_is_an_error() {
        let err = flatpak(&out(1, "", "error: Unable to load summary from remote fedora: Failure when receiving data from the peer\n")).unwrap_err();
        assert!(err.starts_with("Unable to load summary from remote fedora"));
    }

    #[test]
    fn brew_formulae_use_the_newest_installed_version() {
        let text = include_str!("../tests/fixtures/checks/brew-formulae.json");
        let items = brew_formulae(&out(0, text, "")).unwrap();
        assert_eq!(
            items,
            [
                Update::new("git", Some("2.50.0"), Some("2.51.1")),
                Update::new("node", Some("23.9.0"), Some("24.2.0")),
            ]
        );
        assert!(brew_casks(&out(0, text, "")).unwrap().is_empty());
    }

    #[test]
    fn brew_casks_accept_string_and_list_versions() {
        let text = include_str!("../tests/fixtures/checks/brew-casks.json");
        let items = brew_casks(&out(0, text, "")).unwrap();
        assert_eq!(
            items,
            [
                Update::new("firefox", Some("150.0.1"), Some("150.0.2")),
                Update::new("iterm2", Some("3.5.0"), Some("3.5.1")),
            ]
        );
        assert!(brew_casks(&out(0, "not json", "")).is_err());
    }

    #[test]
    fn npm_outdated_exits_with_one() {
        let text = include_str!("../tests/fixtures/checks/npm-outdated.json");
        let items = npm(&out(1, text, "")).unwrap();
        assert_eq!(items, [Update::new("left-pad", None, Some("1.3.0"))]);
        assert!(npm(&out(0, "{}", "")).unwrap().is_empty());
        assert!(npm(&out(0, "", "")).unwrap().is_empty());
        assert!(npm(&out(1, "", "npm ERR! network")).is_err());
        assert!(npm(&out(2, "{}", "")).is_err());
    }

    #[test]
    fn rustup_check() {
        let text = include_str!("../tests/fixtures/checks/rustup-up-to-date.txt");
        assert!(rustup(&out(0, text, "")).unwrap().is_empty());
        let text = include_str!("../tests/fixtures/checks/rustup-updates.txt");
        let items = rustup(&out(100, text, "")).unwrap();
        assert_eq!(
            items,
            [
                Update::new(
                    "stable-x86_64-unknown-linux-gnu",
                    Some("1.99.1"),
                    Some("1.100.0")
                ),
                Update::new("rustup", Some("1.29.2"), Some("1.30.0")),
            ]
        );
    }

    #[test]
    fn waiting_counts_all_steps() {
        let mut checks = Checks::default();
        checks.results.insert("a".into(), Outcome::UpToDate);
        checks.results.insert(
            "b".into(),
            Outcome::Failed {
                message: "x".into(),
            },
        );
        checks.results.insert(
            "c".into(),
            Outcome::Waiting {
                items: vec![Update::new("p", None, None), Update::new("q", None, None)],
            },
        );
        assert_eq!(checks.waiting(), 2);
    }

    #[test]
    fn serializes_with_a_state_tag() {
        let json = serde_json::to_value(Outcome::Waiting { items: vec![] }).unwrap();
        assert_eq!(json["state"], "waiting");
        let json = serde_json::to_value(Outcome::Failed {
            message: "m".into(),
        })
        .unwrap();
        assert_eq!(
            (json["state"].as_str(), json["message"].as_str()),
            (Some("failed"), Some("m"))
        );
    }

    #[test]
    fn cache_round_trip() {
        let dir = std::env::temp_dir().join(format!("topgrade-gui-checks-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(load(&dir), None);
        let mut checks = Checks {
            checked_at: "2026-10-25T12:00:00+02:00".into(),
            ..Default::default()
        };
        checks.results.insert("Brew".into(), Outcome::UpToDate);
        save(&dir, &checks).unwrap();
        assert_eq!(load(&dir), Some(checks));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_tool_is_not_reported() {
        let spec = Spec {
            steps: &["x"],
            command: "definitely-not-installed-xyz",
            parse: npm,
        };
        assert_eq!(execute(&spec), None);
    }
}
