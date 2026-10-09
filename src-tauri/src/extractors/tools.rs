//! Small extractors for tools that report updates on single lines.
//!
//! Formats marked "documented" have no real-run fixture yet.

use std::sync::LazyLock;

use regex::Regex;

use super::{Extracted, Update};

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap()
}

/// One `Update` per line matching `pattern`, built from its captures.
fn per_line(
    lines: &[String],
    pattern: &Regex,
    f: impl Fn(&regex::Captures) -> Update,
) -> Extracted {
    let items = lines
        .iter()
        .filter_map(|l| pattern.captures(l))
        .map(|c| f(&c))
        .collect();
    Extracted {
        items,
        unlisted_count: None,
    }
}

fn opt<'a>(c: &'a regex::Captures, i: usize) -> Option<&'a str> {
    c.get(i).map(|m| m.as_str())
}

/// `Extension 'publisher.name' v1.2.3 was successfully updated.`
pub fn vscode(lines: &[String]) -> Extracted {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| re(r"^Extension '(.+?)' v(\S+) was successfully updated"));
    per_line(lines, &RE, |c| Update::new(&c[1], None, opt(c, 2)))
}

/// `Successfully updated from 2.1.294 to version 2.1.295`
pub fn claude_code(lines: &[String]) -> Extracted {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| re(r"Successfully updated from (\S+) to version (\S+)"));
    per_line(lines, &RE, |c| {
        Update::new("Claude Code", opt(c, 1), opt(c, 2))
    })
}

/// `✔ Plugin "name" updated from 315c4e48967d to 8206942d3ec0 for scope user.`
pub fn claude_code_plugins(lines: &[String]) -> Extracted {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| re(r#"Plugin "(.+?)" updated from (\S+) to (\S+)"#));
    per_line(lines, &RE, |c| Update::new(&c[1], opt(c, 2), opt(c, 3)))
}

/// `⟳ Checking for updates... (current version 1.3.1)`,
/// `✓ Found new version 1.3.2.`, `✓ Update successful!`
pub fn antigravity(lines: &[String]) -> Extracted {
    static CURRENT: LazyLock<Regex> = LazyLock::new(|| re(r"\(current version ([^)\s]+)\)"));
    static NEW: LazyLock<Regex> = LazyLock::new(|| re(r"Found new version (\S+?)\.?$"));
    if !lines.iter().any(|l| l.contains("Update successful")) {
        return Extracted::default();
    }
    let find = |r: &Regex| {
        lines
            .iter()
            .find_map(|l| r.captures(l).map(|c| c[1].to_string()))
    };
    let item = Update {
        name: "Antigravity CLI".into(),
        from: find(&CURRENT),
        to: find(&NEW),
    };
    Extracted {
        items: vec![item],
        unlisted_count: None,
    }
}

/// `Plugins to update: [Dart version 510.0.0, Flutter version 90.0.0]`
pub fn jetbrains_plugins(lines: &[String]) -> Extracted {
    static LIST: LazyLock<Regex> = LazyLock::new(|| re(r"^Plugins to update: \[(.*)\]"));
    static ITEM: LazyLock<Regex> = LazyLock::new(|| re(r"^(.+?) version (\S+)$"));
    let items = lines
        .iter()
        .filter_map(|l| LIST.captures(l))
        .flat_map(|c| {
            c[1].split(", ")
                .map(|entry| match ITEM.captures(entry.trim()) {
                    Some(i) => Update::new(&i[1], None, opt(&i, 2)),
                    None => Update::new(entry.trim(), None, None),
                })
                .collect::<Vec<_>>()
        })
        .filter(|u| !u.name.is_empty())
        .collect();
    Extracted {
        items,
        unlisted_count: None,
    }
}

/// `upgrading 'legacyPackages.x86_64-linux.scrcpy' from flake '…' to '…'`
pub fn nix_profiles(lines: &[String]) -> Extracted {
    static RE: LazyLock<Regex> = LazyLock::new(|| re(r"^upgrading '([^']+)' from"));
    static ATTR: LazyLock<Regex> =
        LazyLock::new(|| re(r"^(?:legacyPackages|packages)\.[^.]+\.(.+)$"));
    per_line(lines, &RE, |c| {
        let attr = &c[1];
        let name = ATTR
            .captures(attr)
            .map_or(attr, |a| a.get(1).unwrap().as_str());
        Update::new(name, None, None)
    })
}

/// `Hooray! Oh My Zsh has been updated!`
pub fn oh_my_zsh(lines: &[String]) -> Extracted {
    let updated = lines
        .iter()
        .any(|l| l.contains("Oh My Zsh has been updated"));
    Extracted {
        items: updated
            .then(|| Update::new("oh-my-zsh", None, None))
            .into_iter()
            .collect(),
        unlisted_count: None,
    }
}

/// `added 1 package, and changed 1122 packages in 35s`: no names, count only.
pub fn npm(lines: &[String]) -> Extracted {
    static RE: LazyLock<Regex> = LazyLock::new(|| re(r"\b(added|changed) (\d+) packages?\b"));
    let counts: Vec<usize> = lines
        .iter()
        .filter(|l| l.contains(" in ") || l.starts_with("added") || l.starts_with("changed"))
        .flat_map(|l| RE.captures_iter(l).map(|c| c[2].parse().unwrap_or(0)))
        .collect();
    Extracted {
        items: Vec::new(),
        unlisted_count: (!counts.is_empty())
            .then(|| counts.iter().sum())
            .filter(|&n| n > 0),
    }
}

/// Documented: `  stable-x86_64-unknown-linux-gnu updated - rustc 1.99.0 (… ) (from rustc 1.98.0 (…))`
pub fn rustup(lines: &[String]) -> Extracted {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| re(r"^\s*(\S+) updated - rustc (\S+).*\(from rustc (\S+)"));
    per_line(lines, &RE, |c| Update::new(&c[1], opt(c, 3), opt(c, 2)))
}

/// Documented: `success: Upgraded uv from v0.12.23 to v0.12.24! https://…`
pub fn uv(lines: &[String]) -> Extracted {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| re(r"Upgraded uv from v?([0-9][^\s!]*) to v?([0-9][^\s!]*)"));
    per_line(lines, &RE, |c| Update::new("uv", opt(c, 1), opt(c, 2)))
}

/// Documented: `Tool 'dotnet-ef' was successfully updated from version '9.0.1' to version '10.0.12'.`
pub fn dotnet(lines: &[String]) -> Extracted {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        re(r"Tool '(.+?)' was successfully updated from version '(.+?)' to version '(.+?)'")
    });
    per_line(lines, &RE, |c| Update::new(&c[1], opt(c, 2), opt(c, 3)))
}

/// Documented: `Done with macOS Sequoia 15.6.1` per installed update.
pub fn softwareupdate(lines: &[String]) -> Extracted {
    static RE: LazyLock<Regex> = LazyLock::new(|| re(r"^Done with (.+)$"));
    per_line(lines, &RE, |c| Update::new(c[1].trim(), None, None))
}

/// Documented: ` 1. [✓] org.mozilla.firefox  stable  u  flathub  85 MB / 90 MB`.
/// Only finished (`[✓]`) updates (`u`); `i` rows are new runtimes/deps.
pub fn flatpak(lines: &[String]) -> Extracted {
    static RE: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*\d+\.\s+\[✓\]\s+(\S+)\s+\S+\s+u\s"));
    per_line(lines, &RE, |c| Update::new(&c[1], None, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractors::lines;

    #[test]
    fn jetbrains_multiple_plugins() {
        let out = jetbrains_plugins(&lines(
            "Plugins to update: [Dart version 510.0.0, Flutter version 90.0.0]",
        ));
        assert_eq!(
            out.items,
            [
                Update::new("Dart", None, Some("510.0.0")),
                Update::new("Flutter", None, Some("90.0.0")),
            ]
        );
    }

    #[test]
    fn antigravity_without_update() {
        let out = antigravity(&lines(
            "⟳ Checking for updates... (current version 1.3.2)\n✓ Already up to date.",
        ));
        assert!(out.items.is_empty());
    }

    #[test]
    fn npm_counts() {
        assert_eq!(
            npm(&lines("changed 3 packages in 2s")).unlisted_count,
            Some(3)
        );
        assert_eq!(npm(&lines("up to date in 1s")).unlisted_count, None);
    }

    #[test]
    fn rustup_toolchain_update() {
        let out = rustup(&lines(
            "  stable-aarch64-apple-darwin updated - rustc 1.99.0 (b940084d7 2026-09-28) (from rustc 1.98.0 (aaaa 2026-08-14))\n  \
             nightly-aarch64-apple-darwin unchanged - rustc 1.100.0-nightly",
        ));
        assert_eq!(
            out.items,
            [Update::new(
                "stable-aarch64-apple-darwin",
                Some("1.98.0"),
                Some("1.99.0")
            )]
        );
    }

    #[test]
    fn uv_self_update() {
        let out = uv(&lines("success: Upgraded uv from v0.12.23 to v0.12.24! https://github.com/astral-sh/uv/releases/tag/0.12.24"));
        assert_eq!(
            out.items,
            [Update::new("uv", Some("0.12.23"), Some("0.12.24"))]
        );
    }

    #[test]
    fn dotnet_tool_update() {
        let out = dotnet(&lines(
            "Tool 'dotnet-ef' was successfully updated from version '9.0.1' to version '10.0.12'.",
        ));
        assert_eq!(
            out.items,
            [Update::new("dotnet-ef", Some("9.0.1"), Some("10.0.12"))]
        );
    }

    #[test]
    fn softwareupdate_done_lines() {
        let out = softwareupdate(&lines(
            "Finding available software\nDownloading Safari 26.1\nInstalling Safari 26.1\nDone with Safari 26.1\nDone.",
        ));
        assert_eq!(out.items, [Update::new("Safari 26.1", None, None)]);
    }

    #[test]
    fn flatpak_finished_updates_only() {
        let out = flatpak(&lines(
            "        ID                     Branch  Op  Remote   Download\n \
             1. [✓] org.mozilla.firefox    stable  u   flathub  85.2 MB / 90.1 MB\n \
             2. [✓] org.gnome.Platform     49      i   flathub  1 MB / 1 MB\n \
             3. [ ] org.gimp.GIMP          stable  u   flathub  < 100 MB",
        ));
        assert_eq!(out.items, [Update::new("org.mozilla.firefox", None, None)]);
    }
}
