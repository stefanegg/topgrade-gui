//! Turns topgrade's raw terminal output into structured events.
//!
//! Step boundaries come from two signals: the OSC window title
//! (`Topgrade - <Step>`) and the separator line (`── HH:MM:SS - <Step> ───`).
//! Whichever arrives first starts the step; the other is deduplicated.
//!
//! Nested topgrade runs (e.g. inside Toolbx) prefix both with
//! `TOPGRADE_PREFIX` as `(<prefix>) `; such steps get a `group` and have their
//! own summary.

use serde::{Deserialize, Serialize};

use crate::extractors::{self, Extracted};

const TITLE_MARKER: &str = "Topgrade - ";
const SUMMARY: &str = "Summary";
/// Title topgrade sets while showing the retry prompt; not a step.
const AWAITING_USER: &str = "Awaiting user";
/// Cap on lines kept per step for extractors.
const MAX_STEP_LINES: usize = 50_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ParseEvent {
    StepStarted {
        index: usize,
        name: String,
        group: Option<String>,
    },
    SummaryStarted {
        group: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    StepResult {
        /// Step this summary line belongs to; `None` if no step header matched.
        step_index: Option<usize>,
        name: String,
        status: StepStatus,
        reason: Option<String>,
    },
    Prompt {
        prompt: PromptKind,
    },
    /// Emitted when a step's output ends, if its extractor found anything.
    StepUpdates {
        index: usize,
        #[serde(flatten)]
        updates: Extracted,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StepStatus {
    Ok,
    Failed,
    Ignored,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PromptKind {
    Retry,
    YesNo,
    Password,
}

pub struct Parser {
    vte: vte::Parser,
    state: State,
}

struct StepInfo {
    group: Option<String>,
    name: String,
    matched: bool,
}

/// Last matched summary line; may still cover further steps (see
/// `State::extend_last_match`).
struct LastMatch {
    index: usize,
    /// Exact matches name one specific step and never extend.
    exact: bool,
    name: String,
    status: StepStatus,
    reason: Option<String>,
}

#[derive(Default)]
struct State {
    line: String,
    /// A `\r` was seen; the next printed char overwrites the line.
    pending_cr: bool,
    /// Prompt already reported for the current line.
    prompt_reported: bool,
    /// Step started from the title whose separator line is still expected.
    awaiting_header: Option<(Option<String>, String)>,
    steps: Vec<StepInfo>,
    /// `Some(group)` while reading the summary of that group.
    summary: Option<Option<String>>,
    /// Summary lines are in step order; only search steps from here on.
    summary_cursor: usize,
    last_match: Option<LastMatch>,
    /// Output lines of the step currently printing (the last started one).
    step_output: Option<Vec<String>>,
    events: Vec<ParseEvent>,
}

impl Default for Parser {
    fn default() -> Self {
        Self {
            vte: vte::Parser::new(),
            state: State::default(),
        }
    }
}

impl Parser {
    /// Feeds raw output and returns events, each paired with the offset into
    /// `bytes` just after the byte that completed it.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<(usize, ParseEvent)> {
        let mut out = Vec::new();
        for (i, byte) in bytes.iter().enumerate() {
            self.vte
                .advance(&mut self.state, std::slice::from_ref(byte));
            out.extend(self.state.events.drain(..).map(|e| (i + 1, e)));
        }
        // Prompts wait for input without a trailing newline, so check the
        // unfinished line once the chunk is consumed.
        if let Some(prompt) = self.state.detect_prompt() {
            out.push((bytes.len(), ParseEvent::Prompt { prompt }));
        }
        out
    }

    /// Call at end of output: flushes results still pending on lookahead.
    pub fn finish(&mut self) -> Vec<ParseEvent> {
        self.state.end_step_output();
        self.state.end_summary();
        std::mem::take(&mut self.state.events)
    }
}

impl State {
    fn start_step(&mut self, group: Option<String>, name: String) {
        self.end_step_output();
        self.end_summary();
        if name == SUMMARY {
            self.summary = Some(group.clone());
            self.summary_cursor = 0;
            self.events.push(ParseEvent::SummaryStarted { group });
            return;
        }
        self.steps.push(StepInfo {
            group: group.clone(),
            name: name.clone(),
            matched: false,
        });
        self.step_output = Some(Vec::new());
        self.events.push(ParseEvent::StepStarted {
            index: self.steps.len() - 1,
            name,
            group,
        });
    }

    fn end_step_output(&mut self) {
        let Some(lines) = self.step_output.take() else {
            return;
        };
        let index = self.steps.len() - 1;
        if let Some(updates) = extractors::extract(&self.steps[index].name, &lines) {
            self.events.push(ParseEvent::StepUpdates { index, updates });
        }
    }

    fn end_summary(&mut self) {
        if self.summary.is_some() {
            self.extend_last_match(self.steps.len());
            self.summary = None;
        }
    }

    fn on_title(&mut self, title: &str) {
        let Some((prefix, name)) = title.split_once(TITLE_MARKER) else {
            return;
        };
        if name == AWAITING_USER {
            return;
        }
        let group = parse_group(prefix.trim());
        self.awaiting_header = Some((group.clone(), name.to_string()));
        self.start_step(group, name.to_string());
    }

    fn on_line(&mut self) {
        let raw = std::mem::take(&mut self.line);
        self.prompt_reported = false;
        self.pending_cr = false;
        let line = raw.trim();

        if let Some((group, name)) = parse_separator(line) {
            let header = (group, name.to_string());
            // Already started from the title.
            if self.awaiting_header.take().as_ref() != Some(&header) {
                self.start_step(header.0, header.1);
            }
            return;
        }
        if let Some(output) = self.step_output.as_mut() {
            if output.len() < MAX_STEP_LINES {
                output.push(raw.trim_end().to_string());
            }
        }
        if self.summary.is_some() {
            if let Some((name, status, reason)) = parse_result(line) {
                self.on_result(name, status, reason);
            }
        }
    }

    fn on_result(&mut self, name: &str, status: StepStatus, reason: Option<&str>) {
        let matched = self.match_summary(name);
        let step_index = matched.map(|(index, _)| index);
        if let Some((index, exact)) = matched {
            self.extend_last_match(index);
            self.last_match = Some(LastMatch {
                index,
                exact,
                name: name.to_string(),
                status,
                reason: reason.map(str::to_string),
            });
        }
        self.events.push(ParseEvent::StepResult {
            step_index,
            name: name.to_string(),
            status,
            reason: reason.map(str::to_string),
        });
    }

    /// Summary keys differ from separator names (`npm` vs
    /// `Node Package Manager`), and some steps have no summary line, so match
    /// fuzzily but in order: exact normalized match first, then fuzzy.
    fn match_summary(&mut self, name: &str) -> Option<(usize, bool)> {
        let group = self.summary.clone().flatten();
        let candidates: Vec<usize> = (self.summary_cursor..self.steps.len())
            .filter(|&i| !self.steps[i].matched && self.steps[i].group == group)
            .collect();
        let (found, exact) = candidates
            .iter()
            .find(|&&i| normalize(&self.steps[i].name) == normalize(name))
            .map(|&i| (i, true))
            .or_else(|| {
                candidates
                    .iter()
                    .find(|&&i| fuzzy_match(&self.steps[i].name, name))
                    .map(|&i| (i, false))
            })?;
        self.steps[found].matched = true;
        self.summary_cursor = found + 1;
        Some((found, exact))
    }

    /// One summary line can cover several consecutive steps (`Flatpak` →
    /// `Flatpak User Packages` + `Flatpak System Packages`). Once the next
    /// line's step (`until`) is known, give the previous result to similar
    /// unmatched steps in between. Only loose matches extend: `Claude Code`
    /// names exactly one step and must not claim `Claude Code Plugins`.
    fn extend_last_match(&mut self, until: usize) {
        let Some(last) = self.last_match.take().filter(|last| !last.exact) else {
            return;
        };
        let group = self.steps[last.index].group.clone();
        for i in last.index + 1..until {
            let step = &mut self.steps[i];
            if step.matched || step.group != group || !fuzzy_match(&step.name, &last.name) {
                continue;
            }
            step.matched = true;
            self.events.push(ParseEvent::StepResult {
                step_index: Some(i),
                name: last.name.clone(),
                status: last.status,
                reason: last.reason.clone(),
            });
        }
    }

    fn detect_prompt(&mut self) -> Option<PromptKind> {
        if self.prompt_reported {
            return None;
        }
        let line = self.line.trim_end();
        let prompt = if line.ends_with("(y)es/(N)o/(s)hell/(q)uit") {
            PromptKind::Retry
        } else if line.ends_with("(Y)es/(N)o") || is_yes_no_confirmation(line) {
            PromptKind::YesNo
        } else if line.to_lowercase().ends_with("password:")
            || (line.starts_with("[sudo]") && line.ends_with(':'))
        {
            PromptKind::Password
        } else {
            return None;
        };
        self.prompt_reported = true;
        Some(prompt)
    }
}

/// Package managers' own confirmations, in any capitalisation and with or
/// without a trailing colon or question mark: dnf `Is this ok [y/N]:`, apt `Do you want to
/// continue? [Y/n]`, Homebrew `Do you want to proceed with the upgrade? [y/n]`.
fn is_yes_no_confirmation(line: &str) -> bool {
    let line = line.trim_end_matches([':', '?']).trim_end();
    let tail = line.get(line.len().saturating_sub(5)..).unwrap_or("");
    let tail = tail.to_ascii_lowercase();
    tail == "[y/n]" || tail == "(y/n)"
}

impl vte::Perform for State {
    fn print(&mut self, c: char) {
        if self.pending_cr {
            self.line.clear();
            self.pending_cr = false;
        }
        self.line.push(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => self.on_line(),
            b'\r' => self.pending_cr = true,
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        // Some tools move the cursor right instead of printing spaces
        // (`Updating\x1b[1Cmarketplaces`); keep the words apart.
        if action == 'C' && intermediates.is_empty() {
            let n = params
                .iter()
                .next()
                .and_then(|p| p.first().copied())
                .unwrap_or(1)
                .max(1);
            for _ in 0..n.min(512) {
                self.print(' ');
            }
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        // OSC 0 / 2 set the window title; a title containing ';' is split.
        if let [b"0" | b"2", rest @ ..] = params {
            let title = rest
                .iter()
                .map(|p| String::from_utf8_lossy(p))
                .collect::<Vec<_>>()
                .join(";");
            self.on_title(&title);
        }
    }
}

/// `── [(<prefix>) ][HH:MM:SS - ]Name ────` or `―― [(<prefix>) ]Name ――`
/// (no-width fallback).
fn parse_separator(line: &str) -> Option<(Option<String>, &str)> {
    let message = if let Some(rest) = line.strip_prefix("── ") {
        rest.trim_end_matches('─').trim_end()
    } else {
        line.strip_prefix("―― ")?.strip_suffix(" ――")?
    };
    let (group, message) = match message.strip_prefix('(').and_then(|m| m.split_once(") ")) {
        Some((prefix, rest)) => (parse_group(&format!("({prefix})")), rest),
        None => (None, message),
    };
    let name = strip_time(message).trim();
    (!name.is_empty()).then_some((group, name))
}

/// `('Toolbx fedora-toolbox-44')` → `Toolbx fedora-toolbox-44`.
fn parse_group(prefix: &str) -> Option<String> {
    let inner = prefix.strip_prefix('(')?.strip_suffix(')')?;
    let inner = inner.trim_matches(|c| c == '\'' || c == '"').trim();
    (!inner.is_empty()).then(|| inner.to_string())
}

fn strip_time(message: &str) -> &str {
    let b = message.as_bytes();
    let is_time = b.len() > 11
        && [0, 1, 3, 4, 6, 7].iter().all(|&i| b[i].is_ascii_digit())
        && b[2] == b':'
        && b[5] == b':'
        && &b[8..11] == b" - ";
    if is_time {
        &message[11..]
    } else {
        message
    }
}

/// `Name: OK`, `Name: FAILED`, `Name: IGNORED`, `Name: SKIPPED: reason`.
fn parse_result(line: &str) -> Option<(&str, StepStatus, Option<&str>)> {
    const STATUSES: [(&str, StepStatus); 4] = [
        ("OK", StepStatus::Ok),
        ("FAILED", StepStatus::Failed),
        ("IGNORED", StepStatus::Ignored),
        ("SKIPPED", StepStatus::Skipped),
    ];
    // Find the first ": STATUS" that ends the line or is followed by ": reason".
    for (pos, _) in line.match_indices(": ") {
        let (name, rest) = (&line[..pos], &line[pos + 2..]);
        for (word, status) in STATUSES {
            let Some(after) = rest.strip_prefix(word) else {
                continue;
            };
            if after.is_empty() {
                return Some((name, status, None));
            }
            if let Some(reason) = after.strip_prefix(": ") {
                return Some((name, status, Some(reason)));
            }
        }
    }
    None
}

/// Lowercase alphanumerics only, without parenthesized parts like `(ARM)`.
fn normalize(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            c if depth == 0 && c.is_alphanumeric() => out.extend(c.to_lowercase()),
            _ => {}
        }
    }
    out
}

fn fuzzy_match(step: &str, summary: &str) -> bool {
    let (step_n, summary_n) = (normalize(step), normalize(summary));
    if summary_n.is_empty() {
        return false;
    }
    // `System update` ⊂ `macOS system update`, `yarn` ⊂ `Yarn Package Manager`.
    if step_n.contains(&summary_n) || summary_n.contains(&step_n) {
        return true;
    }
    // `npm` = Node Package Manager, `pnpm` = Performant Node Package Manager.
    let initials: String = step
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .flat_map(char::to_lowercase)
        .collect();
    initials == summary_n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(input: &[u8]) -> Vec<ParseEvent> {
        let mut parser = Parser::default();
        let mut events: Vec<_> = parser.feed(input).into_iter().map(|(_, e)| e).collect();
        events.extend(parser.finish());
        events
    }

    fn step_names(events: &[ParseEvent]) -> Vec<&str> {
        events
            .iter()
            .filter_map(|e| match e {
                ParseEvent::StepStarted { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Final per-step outcome: (group, step name, summary name, status).
    type Outcome = (Option<String>, String, Option<String>, Option<StepStatus>);

    /// Folds events into one row per step plus unmatched summary names.
    fn outcomes(events: &[ParseEvent]) -> (Vec<Outcome>, Vec<String>) {
        let mut steps: Vec<Outcome> = Vec::new();
        let mut unmatched = Vec::new();
        for e in events {
            match e {
                ParseEvent::StepStarted { index, name, group } => {
                    assert_eq!(*index, steps.len());
                    steps.push((group.clone(), name.clone(), None, None));
                }
                ParseEvent::StepResult {
                    step_index: Some(i),
                    name,
                    status,
                    ..
                } => {
                    let step = &mut steps[*i];
                    assert!(step.3.is_none(), "{} matched twice", step.1);
                    step.2 = Some(name.clone());
                    step.3 = Some(*status);
                }
                ParseEvent::StepResult {
                    step_index: None,
                    name,
                    ..
                } => unmatched.push(name.clone()),
                _ => {}
            }
        }
        (steps, unmatched)
    }

    fn summary_name<'a>(steps: &'a [Outcome], group: Option<&str>, step: &str) -> Option<&'a str> {
        let row = steps
            .iter()
            .find(|s| s.0.as_deref() == group && s.1 == step)
            .unwrap_or_else(|| panic!("no step {step}"));
        row.2.as_deref()
    }

    #[test]
    fn macos_dry_run_fixture() {
        let events = events(include_bytes!("../tests/fixtures/macos-dry-run.log"));
        let (steps, unmatched) = outcomes(&events);
        assert_eq!(steps.len(), 23);
        assert!(unmatched.is_empty());
        for (step, summary) in [
            ("macOS system update", "System update"),
            ("Bun Packages", "bun-packages"),
            ("Node Package Manager", "npm"),
            ("Yarn Package Manager", "yarn"),
            ("Performant Node Package Manager", "pnpm"),
            ("Claude Code", "Claude Code"),
            ("Skills", "Skills"),
            ("Bun", "bun"),
            ("Android Studio plugins", "Android Studio Plugins"),
            ("Brew", "Brew (ARM)"),
            ("Brew - Cask", "Brew Cask (ARM)"),
        ] {
            assert_eq!(summary_name(&steps, None, step), Some(summary), "{step}");
        }
        // Abort with a DryRun error, so topgrade doesn't report them.
        assert_eq!(summary_name(&steps, None, "Claude Code Plugins"), None);
        assert_eq!(summary_name(&steps, None, "uv"), None);
    }

    #[test]
    fn macos_run_fixture() {
        let events = events(include_bytes!("../tests/fixtures/macos-run.log"));
        let (steps, unmatched) = outcomes(&events);
        assert_eq!(steps.len(), 22);
        assert!(unmatched.is_empty());
        assert!(
            steps.iter().all(|s| s.3 == Some(StepStatus::Ok)),
            "{steps:#?}"
        );
        assert_eq!(
            summary_name(&steps, None, "Claude Code Plugins"),
            Some("Claude Code Plugins")
        );
    }

    #[test]
    fn fedora_run_fixture() {
        let events = events(include_bytes!("../tests/fixtures/fedora-run.log"));
        let (steps, unmatched) = outcomes(&events);
        assert_eq!(steps.len(), 20);
        // No separator for this step.
        assert_eq!(unmatched, ["config-update"]);

        // Nested topgrade inside Toolbx: own group and own summary.
        let toolbx = Some("Toolbx fedora-toolbox-44");
        assert_eq!(
            summary_name(&steps, toolbx, "System update"),
            Some("System update")
        );
        assert_eq!(
            summary_name(&steps, None, "System update"),
            Some("System update")
        );
        assert_eq!(summary_name(&steps, None, "Toolbx"), Some("toolbx"));
        assert!(events.contains(&ParseEvent::SummaryStarted {
            group: toolbx.map(Into::into)
        }));

        // One summary line covering several steps.
        assert_eq!(
            summary_name(&steps, None, "Flatpak User Packages"),
            Some("Flatpak")
        );
        assert_eq!(
            summary_name(&steps, None, "Flatpak System Packages"),
            Some("Flatpak")
        );
        assert_eq!(summary_name(&steps, None, "Nix Channels"), Some("nix"));
        assert_eq!(summary_name(&steps, None, "Nix Profiles"), Some("nix"));
        assert_eq!(
            summary_name(&steps, None, "Firmware upgrades"),
            Some("Firmware")
        );
        assert_eq!(
            summary_name(&steps, None, "Claude Code"),
            Some("Claude Code")
        );
        assert_eq!(
            summary_name(&steps, None, "Claude Code Plugins"),
            Some("Claude Code Plugins")
        );

        // Skipped silently (nothing installed), so not in the summary.
        assert_eq!(summary_name(&steps, None, "Node Package Manager"), None);
        assert_eq!(
            summary_name(&steps, None, "Performant Node Package Manager"),
            None
        );
    }

    fn chunked_events(input: &[u8], size: usize) -> Vec<ParseEvent> {
        let mut parser = Parser::default();
        let mut events: Vec<_> = input
            .chunks(size)
            .flat_map(|c| parser.feed(c))
            .map(|(_, e)| e)
            .collect();
        events.extend(parser.finish());
        events
    }

    fn is_prompt(e: &ParseEvent) -> bool {
        matches!(e, ParseEvent::Prompt { .. })
    }

    #[test]
    fn chunking_does_not_change_events() {
        for input in [
            &include_bytes!("../tests/fixtures/macos-dry-run.log")[..],
            &include_bytes!("../tests/fixtures/fedora-run.log")[..],
        ] {
            let whole = events(input);
            for size in [1, 7, 100, 4096] {
                // Prompts are only seen at chunk ends (where a real PTY read
                // stops while topgrade waits), so they depend on chunking.
                let mut chunked = chunked_events(input, size);
                chunked.retain(|e| !is_prompt(e));
                assert_eq!(chunked, whole, "chunk size {size}");
            }
        }
    }

    #[test]
    fn fedora_prompts_detected_when_streamed() {
        let events = chunked_events(include_bytes!("../tests/fixtures/fedora-run.log"), 1);
        let prompts: Vec<_> = events.iter().filter(|e| is_prompt(e)).collect();
        assert_eq!(
            prompts,
            [
                &ParseEvent::Prompt {
                    prompt: PromptKind::Password
                },
                // dnf on the host, then inside Toolbx
                &ParseEvent::Prompt {
                    prompt: PromptKind::YesNo
                },
                &ParseEvent::Prompt {
                    prompt: PromptKind::YesNo
                },
            ]
        );
    }

    #[test]
    fn package_manager_confirmations_detected() {
        for line in [
            "Is this ok [y/N]: ",
            "Do you want to continue? [Y/n] ",
            "==> Do you want to proceed with the upgrade? [y/n] ",
            "\x1b[1m==>\x1b[0m Do you want to proceed with the upgrade? [y/n]",
            "Proceed (y/n)? ",
        ] {
            let events = Parser::default().feed(line.as_bytes());
            assert_eq!(
                events.last().map(|(_, e)| e),
                Some(&ParseEvent::Prompt {
                    prompt: PromptKind::YesNo
                }),
                "{line:?}"
            );
        }
        // Mentioning the choice mid-line isn't a prompt.
        assert!(Parser::default().feed(b"Answer [y/n] below").is_empty());
    }

    #[test]
    fn event_offsets_point_after_trigger() {
        let input = b"before\n\x1b]0;Topgrade - Foo\x07\r\n\xe2\x94\x80\xe2\x94\x80 Foo \xe2\x94\x80\xe2\x94\x80\r\n";
        let events = Parser::default().feed(input);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, b"before\n\x1b]0;Topgrade - Foo\x07".len());
    }

    #[test]
    fn separator_without_title_starts_step() {
        let events = events("\r\n── 10:00:00 - Flatpak ───────\r\n―― dnf ――\r\n".as_bytes());
        assert_eq!(step_names(&events), ["Flatpak", "dnf"]);
    }

    #[test]
    fn separator_variants() {
        assert_eq!(
            parse_separator("── Flatpak ──────"),
            Some((None, "Flatpak"))
        );
        assert_eq!(
            parse_separator("── ('Toolbx f44') 13:36:09 - System update ────"),
            Some((Some("Toolbx f44".into()), "System update"))
        );
        assert_eq!(
            parse_separator("―― (box) dnf ――"),
            Some((Some("box".into()), "dnf"))
        );
        assert_eq!(parse_separator("some output ── here"), None);
    }

    #[test]
    fn prefixed_title_dedupes_with_header() {
        let input = "\x1b]0;('box') Topgrade - dnf\x07\r\n── ('box') 10:00:00 - dnf ──\r\n";
        let events = events(input.as_bytes());
        assert_eq!(
            events,
            [ParseEvent::StepStarted {
                index: 0,
                name: "dnf".into(),
                group: Some("box".into())
            }]
        );
    }

    #[test]
    fn awaiting_user_title_is_not_a_step() {
        let input = "\x1b]0;Topgrade - Foo\x07── Foo ──\r\n\x1b]0;Topgrade - Awaiting user\x07";
        assert_eq!(step_names(&events(input.as_bytes())), ["Foo"]);
    }

    #[test]
    fn summary_results_with_reasons() {
        let input = "── Foo ──\r\n── Bar ──\r\n── Summary ──\r\n\
            Foo: \x1b[31mFAILED\x1b[0m\r\nBar: SKIPPED: Could not find sudo\r\nBaz: IGNORED\r\n";
        let events = events(input.as_bytes());
        assert!(events.contains(&ParseEvent::StepResult {
            step_index: Some(0),
            name: "Foo".into(),
            status: StepStatus::Failed,
            reason: None,
        }));
        assert!(events.contains(&ParseEvent::StepResult {
            step_index: Some(1),
            name: "Bar".into(),
            status: StepStatus::Skipped,
            reason: Some("Could not find sudo".into()),
        }));
        assert!(events.contains(&ParseEvent::StepResult {
            step_index: None,
            name: "Baz".into(),
            status: StepStatus::Ignored,
            reason: None,
        }));
    }

    #[test]
    fn last_result_extends_to_similar_steps_at_end() {
        let input = "── A ──\r\n── Flatpak User ──\r\n── Flatpak System ──\r\n\
            ── Summary ──\r\nA: OK\r\nFlatpak: FAILED\r\n";
        let (steps, _) = outcomes(&events(input.as_bytes()));
        assert_eq!(steps[2].3, Some(StepStatus::Failed));
    }

    #[test]
    fn result_lines_outside_summary_are_ignored() {
        let events = events(b"Foo: OK\r\n");
        assert!(events.is_empty());
    }

    #[test]
    fn prompts_detected_on_partial_line_once() {
        let mut parser = Parser::default();
        let retry = "\r\n\x1b[33;1mRetry? (y)es/(N)o/(s)hell/(q)uit\x1b[0m";
        assert_eq!(
            parser.feed(retry.as_bytes()),
            [(
                retry.len(),
                ParseEvent::Prompt {
                    prompt: PromptKind::Retry
                }
            )]
        );
        assert!(parser.feed(b"").is_empty());

        assert_eq!(
            events(b"Password:"),
            [ParseEvent::Prompt {
                prompt: PromptKind::Password
            }]
        );
        assert_eq!(
            events(b"[sudo] password for s: "),
            [ParseEvent::Prompt {
                prompt: PromptKind::Password
            }]
        );
        for input in [
            &b"Continue? (Y)es/(N)o"[..],
            b"Is this ok [y/N]: ",
            b"Do you want to continue? [Y/n] :",
        ] {
            assert_eq!(
                events(input),
                [ParseEvent::Prompt {
                    prompt: PromptKind::YesNo,
                }],
                "{}",
                String::from_utf8_lossy(input)
            );
        }
    }

    #[test]
    fn cursor_forward_becomes_spaces() {
        let mut state = State::default();
        let mut vte = vte::Parser::new();
        vte.advance(&mut state, b"Updating\x1b[Cmarketplaces\x1b[2Cdone");
        assert_eq!(state.line, "Updating marketplaces  done");
    }

    #[test]
    fn carriage_return_overwrites_line() {
        let events = events("progress 50%\r── Foo ──\r\n".as_bytes());
        assert_eq!(step_names(&events), ["Foo"]);
    }
}
