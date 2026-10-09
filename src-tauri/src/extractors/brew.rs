//! `brew upgrade --formula` / `--cask`.
//!
//! Brew first prints the plan (`==> Upgrading 3 outdated packages:` followed
//! by `name old -> new` rows), then `==> Upgrading <name>` per package as it
//! actually upgrades it. Formulae repeat `old -> new` on the next line; casks
//! don't, so versions come from the plan.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use super::{Extracted, Update};

static PLAN_HEADER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^==> Upgrading \d+ outdated packages?:$").unwrap());
static PLAN_ROW: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\S+)\s+(\S+)\s+->\s+(\S+)").unwrap());
static UPGRADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^==> Upgrading (\S+)$").unwrap());
static VERSIONS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s+(\S+)\s+->\s+(\S+)\s*$").unwrap());

pub fn extract(lines: &[String]) -> Extracted {
    let mut plan: HashMap<&str, (&str, &str)> = HashMap::new();
    let mut items = Vec::new();
    let mut in_plan = false;
    let mut iter = lines.iter().map(|l| l.trim_end()).peekable();

    while let Some(line) = iter.next() {
        if PLAN_HEADER.is_match(line) {
            in_plan = true;
            continue;
        }
        if in_plan {
            if let Some(c) = PLAN_ROW.captures(line) {
                let (name, from, to) = (c.get(1).unwrap(), c.get(2).unwrap(), c.get(3).unwrap());
                plan.insert(name.as_str(), (from.as_str(), to.as_str()));
                continue;
            }
            in_plan = false;
        }
        if let Some(c) = UPGRADING.captures(line) {
            let name = c.get(1).unwrap().as_str();
            let versions = iter
                .peek()
                .and_then(|next| VERSIONS.captures(next))
                .map(|v| (v.get(1).unwrap().as_str(), v.get(2).unwrap().as_str()))
                .or_else(|| plan.get(name).copied());
            items.push(Update::new(
                name,
                versions.map(|v| v.0),
                versions.map(|v| v.1),
            ));
        }
    }
    Extracted {
        items,
        unlisted_count: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractors::lines;

    #[test]
    fn formula_versions_from_upgrade_line() {
        let out = extract(&lines(
            "==> Upgrading 1 outdated package:\n\
             go 1.27.1 -> 1.27.2 (67.7MB)\n\
             ==> Fetching downloads for: go\n\
             ==> Upgrading go\n  1.27.1 -> 1.27.2\n\
             ==> Pouring go--1.27.2.arm64.bottle.tar.gz",
        ));
        assert_eq!(
            out.items,
            [Update::new("go", Some("1.27.1"), Some("1.27.2"))]
        );
    }

    #[test]
    fn cask_versions_from_plan() {
        let out = extract(&lines(
            "==> Upgrading 2 outdated packages:\n\
             firefox 130.0 -> 130.0.1\n\
             iterm2 3.5.0 -> 3.5.1\n\
             ==> Upgrading firefox\n\
             ==> Downloading https://example.com/firefox.dmg\n\
             🍺  firefox was successfully upgraded!",
        ));
        // iterm2 was planned but never upgraded (e.g. the run was stopped).
        assert_eq!(
            out.items,
            [Update::new("firefox", Some("130.0"), Some("130.0.1"))]
        );
    }

    #[test]
    fn nothing_to_upgrade() {
        assert!(
            extract(&lines("==> Updating Homebrew...\nAlready up-to-date."))
                .items
                .is_empty()
        );
    }
}
