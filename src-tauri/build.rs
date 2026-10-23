use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Semver from the nearest `vX.Y.Z` tag: the tag itself on a tagged commit,
/// otherwise `X.Y.Z-dev.N+gSHA` (N commits past the tag). Without tags (or
/// git) it falls back to the Cargo.toml version.
fn version() -> String {
    let fallback = std::env::var("CARGO_PKG_VERSION").unwrap();
    let dirty = git(&["status", "--porcelain"]).is_some_and(|s| !s.is_empty());
    let sha = git(&["rev-parse", "--short", "HEAD"]);
    let described = git(&["describe", "--tags", "--long", "--match", "v[0-9]*"]);

    let mut version = match (described, &sha) {
        (Some(d), _) => {
            // `v1.2.3-4-gabc1234`
            let mut parts = d.rsplitn(3, '-');
            let (_sha, ahead, tag) = (parts.next(), parts.next(), parts.next());
            let base = tag.unwrap_or(&d).trim_start_matches('v').to_string();
            match (ahead, sha) {
                (Some("0"), _) | (None, _) => base,
                (Some(n), Some(sha)) => format!("{base}-dev.{n}+g{sha}"),
                (Some(n), None) => format!("{base}-dev.{n}"),
            }
        }
        (None, Some(sha)) => format!("{fallback}+g{sha}"),
        (None, None) => fallback,
    };
    if dirty {
        version.push_str(if version.contains('+') { ".dirty" } else { "+dirty" });
    }
    version
}

fn main() {
    if let Some(git_dir) = git(&["rev-parse", "--git-dir"]) {
        println!("cargo:rerun-if-changed={git_dir}/HEAD");
        println!("cargo:rerun-if-changed={git_dir}/refs/tags");
        println!("cargo:rerun-if-changed={git_dir}/packed-refs");
    }
    println!("cargo:rustc-env=APP_VERSION={}", version());
    tauri_build::build()
}
