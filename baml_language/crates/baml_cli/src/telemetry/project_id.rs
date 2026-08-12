//! Hashed project identifier. Direct port of Next.js's
//! `packages/next/src/telemetry/project-id.ts` with one deliberate change:
//! we walk the filesystem for `.git` instead of shelling out to `git`, so
//! `baml <cmd>` never spawns a subprocess just to compute telemetry.
//!
//! The output is the SHA-256 of the salted project root path. The salt
//! lives only in `<baml_home>/telemetry.toml`, so:
//!
//! - Aggregate dashboards can dedupe "same project seen twice" from the
//!   same user.
//! - We can never reverse the digest back to a filesystem path — the salt
//!   makes rainbow-table / dictionary attacks impractical, and it never
//!   leaves the user's machine.
//! - Two users with the same directory structure produce different hashes,
//!   which is what we want: we're counting projects per user, not paths
//!   across users.

use std::path::{Path, PathBuf};

use super::storage::Telemetry;

/// Compute the hashed project id for this invocation.
///
/// Roots (in order of preference):
///  1. The nearest ancestor of the current working directory containing a
///     `.git` entry (matches how `git rev-parse --show-toplevel` behaves,
///     minus the subprocess).
///  2. The current working directory if we couldn't find one.
///  3. The static string `"unknown"` if we couldn't even read the cwd.
pub(crate) fn compute(telemetry: &Telemetry) -> String {
    let root = project_root().unwrap_or_else(|| PathBuf::from("unknown"));
    telemetry.one_way_hash(root.as_os_str().as_encoded_bytes())
}

fn project_root() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    // Walk up until we find `.git`, or hit the filesystem root.
    let mut current: &Path = cwd.as_path();
    loop {
        if current.join(".git").exists() {
            return Some(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return Some(cwd),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `project_root` prefers the nearest ancestor with a `.git` entry.
    #[test]
    fn walks_up_to_find_git_root() {
        let root = tempfile::tempdir().unwrap();
        // A repo at `<tmp>/repo`, with `.git` inside; cwd at `<tmp>/repo/sub`.
        let repo = root.path().join("repo");
        let sub = repo.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        // We can't safely mutate the process cwd in a parallel test suite,
        // so directly exercise the walk with a helper. Match the logic in
        // `project_root` on a synthetic starting point.
        let found = walk_up_for_git(&sub);
        assert_eq!(found.as_deref(), Some(repo.as_path()));
    }

    /// Test-only mirror of `project_root`'s walk that takes a start dir,
    /// so we don't have to mutate the process cwd.
    fn walk_up_for_git(start: &Path) -> Option<PathBuf> {
        let mut current = start;
        loop {
            if current.join(".git").exists() {
                return Some(current.to_path_buf());
            }
            current = current.parent()?;
        }
    }
}
