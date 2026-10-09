//! dnf5 (and dnf4) transaction tables.
//!
//! ```text
//! Upgrading:
//!  firefox             x86_64 0:157.0.1-1.fc44  updates  291.1 MiB
//!    replacing firefox x86_64 0:157.0-1.fc44    updates  290.6 MiB
//! Installing dependencies:
//!  libfoo              x86_64 0:1.0-1.fc44      updates    1.0 MiB
//! Transaction Summary:
//! ...
//! Complete!
//! ```
//!
//! The table is printed before the `[y/N]` confirmation, so it only counts if
//! the transaction completed.

use std::sync::LazyLock;

use regex::Regex;

use super::{Extracted, Update};

/// Sections whose packages end up updated or newly installed.
const SECTIONS: &[&str] = &[
    "Upgrading:",
    "Downgrading:",
    "Installing:",
    "Installing dependencies:",
    "Installing weak dependencies:",
];

static ROW: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^ (\S+)\s+\S+\s+(\S+)\s+\S+").unwrap());
static REPLACING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s+replacing\s+\S+\s+\S+\s+(\S+)").unwrap());

pub fn extract(lines: &[String]) -> Extracted {
    if !lines.iter().any(|l| l.trim() == "Complete!") {
        return Extracted::default();
    }
    let mut items: Vec<Update> = Vec::new();
    let mut in_section = false;
    for line in lines {
        // Section headers start at column 0; rows are indented.
        if !line.starts_with(' ') {
            in_section = SECTIONS.contains(&line.trim_end());
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some(c) = REPLACING.captures(line) {
            if let Some(last) = items.last_mut() {
                last.from = Some(strip_epoch(&c[1]).to_string());
            }
        } else if let Some(c) = ROW.captures(line) {
            items.push(Update::new(&c[1], None, Some(strip_epoch(&c[2]))));
        }
    }
    Extracted {
        items,
        unlisted_count: None,
    }
}

/// `0:157.0-1.fc44` → `157.0-1.fc44`.
fn strip_epoch(version: &str) -> &str {
    version.strip_prefix("0:").unwrap_or(version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractors::lines;

    const TABLE: &str = "Package  Arch  Version  Repository  Size\n\
        Upgrading:\n \
        firefox  x86_64  0:157.0.1-1.fc44  updates  291.1 MiB\n   \
        replacing firefox  x86_64  0:157.0-1.fc44  updates  290.6 MiB\n\
        Installing dependencies:\n \
        libfoo  x86_64  2:1.0-1.fc44  updates  1.0 MiB\n\
        Removing:\n \
        oldthing  noarch  1.0-1.fc44  @System  1 KiB\n\
        \n\
        Transaction Summary:\n \
        Upgrading:  1 package\n";

    #[test]
    fn upgrades_and_installs() {
        let out = extract(&lines(&format!("{TABLE}Is this ok [y/N]: y\nComplete!")));
        assert_eq!(
            out.items,
            [
                Update::new("firefox", Some("157.0-1.fc44"), Some("157.0.1-1.fc44")),
                Update::new("libfoo", None, Some("2:1.0-1.fc44")),
            ]
        );
    }

    #[test]
    fn declined_transaction_reports_nothing() {
        let out = extract(&lines(&format!(
            "{TABLE}Is this ok [y/N]: n\nOperation aborted by the user."
        )));
        assert!(out.items.is_empty());
    }

    #[test]
    fn dnf4_rows_without_epoch() {
        let out = extract(&lines(
            "Upgrading:\n firefox  x86_64  157.0.1-1.fc44  updates  291 M\n\nComplete!",
        ));
        assert_eq!(
            out.items,
            [Update::new("firefox", None, Some("157.0.1-1.fc44"))]
        );
    }
}
