//! Turns topgrade's raw terminal output into structured events.
//!
//! Step boundaries come from two signals: the OSC window title
//! (`Topgrade - <Step>`) and the separator line (`── HH:MM:SS - <Step> ───`).
//! Whichever arrives first starts the step; the other is deduplicated.

use serde::Serialize;

const TITLE_PREFIX: &str = "Topgrade - ";
const SUMMARY: &str = "Summary";
/// Title topgrade sets while showing the retry prompt; not a step.
const AWAITING_USER: &str = "Awaiting user";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ParseEvent {
    StepStarted { index: usize, name: String },
    SummaryStarted,
    #[serde(rename_all = "camelCase")]
    StepResult {
        /// Step this summary line belongs to; `None` if no step header matched.
        step_index: Option<usize>,
        name: String,
        status: StepStatus,
        reason: Option<String>,
    },
    Prompt { prompt: PromptKind },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
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

#[derive(Default)]
struct State {
    line: String,
    /// A `\r` was seen; the next printed char overwrites the line.
    pending_cr: bool,
    /// Prompt already reported for the current line.
    prompt_reported: bool,
    /// Step started from the title whose separator line is still expected.
    awaiting_header: Option<String>,
    steps: Vec<String>,
    matched: Vec<bool>,
    in_summary: bool,
    /// Summary lines are in step order; only search steps from here on.
    summary_cursor: usize,
    events: Vec<ParseEvent>,
}

impl Default for Parser {
    fn default() -> Self {
        Self { vte: vte::Parser::new(), state: State::default() }
    }
}

impl Parser {
    /// Feeds raw output and returns events, each paired with the offset into
    /// `bytes` just after the byte that completed it.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<(usize, ParseEvent)> {
        let mut out = Vec::new();
        for (i, byte) in bytes.iter().enumerate() {
            self.vte.advance(&mut self.state, std::slice::from_ref(byte));
            out.extend(self.state.events.drain(..).map(|e| (i + 1, e)));
        }
        // Prompts wait for input without a trailing newline, so check the
        // unfinished line once the chunk is consumed.
        if let Some(prompt) = self.state.detect_prompt() {
            out.push((bytes.len(), ParseEvent::Prompt { prompt }));
        }
        out
    }
}

impl State {
    fn start_step(&mut self, name: String) {
        if name == SUMMARY {
            self.in_summary = true;
            self.events.push(ParseEvent::SummaryStarted);
            return;
        }
        self.in_summary = false;
        self.steps.push(name.clone());
        self.matched.push(false);
        self.events.push(ParseEvent::StepStarted { index: self.steps.len() - 1, name });
    }

    fn on_title(&mut self, title: &str) {
        let Some(name) = title.strip_prefix(TITLE_PREFIX) else { return };
        if name == AWAITING_USER {
            return;
        }
        self.awaiting_header = Some(name.to_string());
        self.start_step(name.to_string());
    }

    fn on_line(&mut self) {
        let line = std::mem::take(&mut self.line);
        self.prompt_reported = false;
        self.pending_cr = false;
        let line = line.trim();

        if let Some(name) = parse_separator(line) {
            // Already started from the title.
            if self.awaiting_header.take().as_deref() != Some(name) {
                self.start_step(name.to_string());
            }
            return;
        }
        if self.in_summary {
            if let Some((name, status, reason)) = parse_result(line) {
                let step_index = self.match_summary(name);
                self.events.push(ParseEvent::StepResult {
                    step_index,
                    name: name.to_string(),
                    status,
                    reason: reason.map(str::to_string),
                });
            }
        }
    }

    /// Summary keys differ from separator names (`npm` vs
    /// `Node Package Manager`), and some steps have no summary line, so match
    /// fuzzily but in order: exact normalized match first, then fuzzy.
    fn match_summary(&mut self, name: &str) -> Option<usize> {
        let candidates = self.summary_cursor..self.steps.len();
        let found = candidates
            .clone()
            .find(|&i| !self.matched[i] && normalize(&self.steps[i]) == normalize(name))
            .or_else(|| candidates.clone().find(|&i| !self.matched[i] && fuzzy_match(&self.steps[i], name)))?;
        self.matched[found] = true;
        self.summary_cursor = found + 1;
        Some(found)
    }

    fn detect_prompt(&mut self) -> Option<PromptKind> {
        if self.prompt_reported {
            return None;
        }
        let line = self.line.trim_end();
        let prompt = if line.ends_with("(y)es/(N)o/(s)hell/(q)uit") {
            PromptKind::Retry
        } else if line.ends_with("(Y)es/(N)o") {
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

/// `── HH:MM:SS - Name ────` (time optional) or `―― Name ――` (no-width fallback).
fn parse_separator(line: &str) -> Option<&str> {
    let message = if let Some(rest) = line.strip_prefix("── ") {
        rest.trim_end_matches('─').trim_end()
    } else {
        line.strip_prefix("―― ")?.strip_suffix(" ――")?
    };
    let name = strip_time(message).trim();
    (!name.is_empty()).then_some(name)
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
            let Some(after) = rest.strip_prefix(word) else { continue };
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
        Parser::default().feed(input).into_iter().map(|(_, e)| e).collect()
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

    #[test]
    fn macos_dry_run_fixture() {
        let input = include_bytes!("../tests/fixtures/macos-dry-run.log");
        let events = events(input);

        let steps = step_names(&events);
        assert_eq!(steps.len(), 23);
        assert_eq!(steps[0], "macOS system update");
        assert_eq!(steps[22], "Brew - Cask");
        assert_eq!(events.iter().filter(|e| **e == ParseEvent::SummaryStarted).count(), 1);

        let results: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                ParseEvent::StepResult { step_index, name, status, .. } => {
                    Some((step_index.map(|i| steps[i]), name.as_str(), *status))
                }
                _ => None,
            })
            .collect();
        assert_eq!(results.len(), 21);
        assert!(results.iter().all(|(_, _, s)| *s == StepStatus::Ok));
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
            assert!(results.contains(&(Some(step), summary, StepStatus::Ok)), "{summary} -> {step}");
        }
        assert!(results.iter().all(|(step, _, _)| step.is_some()));
    }

    #[test]
    fn chunking_does_not_change_events() {
        let input = include_bytes!("../tests/fixtures/macos-dry-run.log");
        let whole = events(input);
        for size in [1, 7, 100, 4096] {
            let mut parser = Parser::default();
            let chunked: Vec<_> = input
                .chunks(size)
                .flat_map(|c| parser.feed(c))
                .map(|(_, e)| e)
                .collect();
            assert_eq!(chunked, whole, "chunk size {size}");
        }
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
    fn separator_without_time() {
        assert_eq!(parse_separator("── Flatpak ──────"), Some("Flatpak"));
        assert_eq!(parse_separator("some output ── here"), None);
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
            [(retry.len(), ParseEvent::Prompt { prompt: PromptKind::Retry })]
        );
        assert!(parser.feed(b"").is_empty());

        assert_eq!(events(b"Password:"), [ParseEvent::Prompt { prompt: PromptKind::Password }]);
        assert_eq!(
            events(b"[sudo] password for s: "),
            [ParseEvent::Prompt { prompt: PromptKind::Password }]
        );
        assert_eq!(
            events(b"Continue? (Y)es/(N)o"),
            [ParseEvent::Prompt { prompt: PromptKind::YesNo }]
        );
    }

    #[test]
    fn carriage_return_overwrites_line() {
        let events = events("progress 50%\r── Foo ──\r\n".as_bytes());
        assert_eq!(step_names(&events), ["Foo"]);
    }
}
