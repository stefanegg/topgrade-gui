//! Per-step "what was updated" parsers, keyed by topgrade's step name.
//!
//! Each extractor gets the step's output lines (ANSI stripped, `\r`
//! overwrites applied) and only reports what the tool says it actually did,
//! never what it planned. Unknown steps report nothing.

mod brew;
mod dnf;
mod tools;

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Update {
    pub name: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

impl Update {
    fn new(name: &str, from: Option<&str>, to: Option<&str>) -> Self {
        Self {
            name: name.to_string(),
            from: from.map(str::to_string),
            to: to.map(str::to_string),
        }
    }
}

/// What a step updated.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Extracted {
    pub items: Vec<Update>,
    /// Count reported by tools that don't list names (npm's "changed 12
    /// packages", which includes transitive dependencies).
    pub unlisted_count: Option<usize>,
}

type Extractor = fn(&[String]) -> Extracted;

const REGISTRY: &[(&str, Extractor)] = &[
    ("Brew", brew::extract),
    ("Brew - Cask", brew::extract),
    // Linux system package manager; also the nested run inside Toolbx.
    ("System update", dnf::extract),
    ("macOS system update", tools::softwareupdate),
    ("Flatpak", tools::flatpak),
    ("Flatpak User Packages", tools::flatpak),
    ("Flatpak System Packages", tools::flatpak),
    ("Nix Profiles", tools::nix_profiles),
    ("rustup", tools::rustup),
    ("uv", tools::uv),
    (".NET", tools::dotnet),
    ("Node Package Manager", tools::npm),
    ("Visual Studio Code extensions", tools::vscode),
    ("Android Studio plugins", tools::jetbrains_plugins),
    ("Claude Code", tools::claude_code),
    ("Claude Code Plugins", tools::claude_code_plugins),
    ("Antigravity CLI", tools::antigravity),
    ("oh-my-zsh", tools::oh_my_zsh),
];

/// Runs the extractor registered for `step`; `None` if there is none or it
/// found nothing.
pub fn extract(step: &str, lines: &[String]) -> Option<Extracted> {
    let (_, extractor) = REGISTRY.iter().find(|(name, _)| *name == step)?;
    let mut extracted = extractor(lines);
    // Tools that redraw progress can print the same line more than once.
    let mut seen = HashSet::new();
    extracted
        .items
        .retain(|item| seen.insert(item.name.clone()));
    (!extracted.items.is_empty() || extracted.unlisted_count.is_some()).then_some(extracted)
}

#[cfg(test)]
pub(crate) fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{ParseEvent, Parser};

    /// (group, step name, extracted) for every step with updates.
    fn fixture_updates(input: &[u8]) -> Vec<(Option<String>, String, Extracted)> {
        let mut parser = Parser::default();
        let mut events: Vec<_> = parser.feed(input).into_iter().map(|(_, e)| e).collect();
        events.extend(parser.finish());
        let mut steps = Vec::new();
        let mut out = Vec::new();
        for event in events {
            match event {
                ParseEvent::StepStarted { name, group, .. } => steps.push((group, name)),
                ParseEvent::StepUpdates { index, updates } => {
                    let (group, name) = steps[index].clone();
                    out.push((group, name, updates));
                }
                _ => {}
            }
        }
        out
    }

    fn find<'a>(
        all: &'a [(Option<String>, String, Extracted)],
        group: Option<&str>,
        step: &str,
    ) -> &'a Extracted {
        &all.iter()
            .find(|(g, s, _)| g.as_deref() == group && s == step)
            .unwrap_or_else(|| panic!("no updates for {step}"))
            .2
    }

    #[test]
    fn macos_run() {
        let all = fixture_updates(include_bytes!("../../tests/fixtures/macos-run.log"));
        let steps: Vec<_> = all.iter().map(|(_, s, _)| s.as_str()).collect();
        assert_eq!(
            steps,
            [
                "oh-my-zsh",
                "Visual Studio Code extensions",
                "Node Package Manager",
                "Claude Code",
                "Claude Code Plugins",
                "Android Studio plugins",
                "Antigravity CLI",
                "Brew",
            ]
        );

        let brew = find(&all, None, "Brew");
        assert_eq!(brew.items.len(), 16);
        assert_eq!(
            brew.items[0],
            Update::new("libevent", Some("2.1.13"), Some("2.1.13_1"))
        );
        assert!(brew
            .items
            .contains(&Update::new("go", Some("1.27.1"), Some("1.27.2"))));

        assert_eq!(
            find(&all, None, "Visual Studio Code extensions").items,
            [
                Update::new("shd101wyy.markdown-preview-enhanced", None, Some("0.8.40")),
                Update::new("anthropic.claude-code", None, Some("2.1.295")),
            ]
        );
        assert_eq!(
            find(&all, None, "Node Package Manager").unlisted_count,
            Some(1123)
        );
        assert_eq!(
            find(&all, None, "Claude Code").items,
            [Update::new("Claude Code", Some("2.1.294"), Some("2.1.295"))]
        );
        assert_eq!(
            find(&all, None, "Claude Code Plugins").items,
            [Update::new(
                "frontend-design",
                Some("315c4e48967d"),
                Some("8206942d3ec0")
            )]
        );
        assert_eq!(
            find(&all, None, "Android Studio plugins").items,
            [Update::new("Dart", None, Some("510.0.0"))]
        );
        assert_eq!(
            find(&all, None, "Antigravity CLI").items,
            [Update::new("Antigravity CLI", Some("1.3.1"), Some("1.3.2"))]
        );
        assert_eq!(
            find(&all, None, "oh-my-zsh").items,
            [Update::new("oh-my-zsh", None, None)]
        );
    }

    #[test]
    fn fedora_run() {
        let all = fixture_updates(include_bytes!("../../tests/fixtures/fedora-run.log"));

        let host = find(&all, None, "System update");
        assert_eq!(host.items.len(), 14);
        assert_eq!(
            host.items[1],
            Update::new("firefox", Some("157.0-1.fc44"), Some("157.0.1-1.fc44"))
        );

        let toolbx = find(&all, Some("Toolbx fedora-toolbox-44"), "System update");
        let names: Vec<_> = toolbx.items.iter().map(|u| u.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "hunspell",
                "hunspell-filesystem",
                "sudo",
                "sudo-python-plugin"
            ]
        );

        assert_eq!(
            find(&all, None, "Nix Profiles").items,
            [Update::new("scrcpy", None, None)]
        );
        assert_eq!(
            find(&all, None, "Claude Code").items,
            [Update::new("Claude Code", Some("2.1.294"), Some("2.1.295"))]
        );
    }

    #[test]
    fn dry_run_reports_nothing() {
        assert!(
            fixture_updates(include_bytes!("../../tests/fixtures/macos-dry-run.log")).is_empty()
        );
    }

    #[test]
    fn unknown_step_and_empty_results_are_none() {
        assert_eq!(extract("Some new step", &lines("updated everything")), None);
        assert_eq!(extract("Brew", &lines("Already up-to-date.")), None);
    }

    #[test]
    fn duplicates_are_removed() {
        let out = extract(
            "Visual Studio Code extensions",
            &lines(
                "Extension 'a.b' v1.0.0 was successfully updated.\n\
                 Extension 'a.b' v1.0.0 was successfully updated.",
            ),
        )
        .unwrap();
        assert_eq!(out.items.len(), 1);
    }
}
