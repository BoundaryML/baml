//! Fetch the latest CI-measured size-gate reports for a branch via `gh`.
//!
//! The size-gate jobs upload a `size-gate-<platform>.json` report on every
//! CI run. Rather than rebuild locally — which produces sizes that differ
//! from CI and only covers the host platform — we adopt those exact reports.
//! This is what `bake --branch <name>` uses so a developer (or their agent)
//! can fix a stale baseline with a single command.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// The per-platform report artifacts a complete CI run uploads. We require
/// all of them so a fetch never adopts a partial set (which would leave some
/// platforms on a stale baseline).
const REQUIRED_ARTIFACTS: [&str; 4] = [
    "size-gate-linux",
    "size-gate-macos",
    "size-gate-windows",
    "size-gate-wasm",
];

/// The CI workflow whose runs carry the size-gate report artifacts.
const CI_WORKFLOW: &str = "ci.yaml";

/// How many recent runs to scan for a complete set of reports.
const SCAN_LIMIT: usize = 20;

pub(crate) struct Fetched {
    pub files: Vec<PathBuf>,
    pub run_id: String,
    pub head_sha: String,
}

#[derive(Deserialize)]
struct RunListEntry {
    #[serde(rename = "databaseId")]
    database_id: u64,
    #[serde(rename = "headSha")]
    head_sha: String,
}

/// Find the newest *completed* CI run on `branch` that has all four
/// size-gate reports, download them into `download_dir`, and return their
/// paths plus the run's id and head SHA.
///
/// The overall run conclusion is intentionally ignored: CI is frequently
/// red for reasons unrelated to size while the size-gate jobs themselves
/// pass and upload valid reports, so the conclusion is the wrong signal.
pub(crate) fn fetch_branch_reports(
    branch: &str,
    repo: Option<&str>,
    download_dir: &Path,
) -> Result<Fetched> {
    let repo = match repo {
        Some(r) => r.to_owned(),
        None => gh(&[
            "repo",
            "view",
            "--json",
            "nameWithOwner",
            "--jq",
            ".nameWithOwner",
        ])
        .context("could not determine the repo — pass --repo <owner/name>")?
        .trim()
        .to_owned(),
    };

    // Newest-first completed runs on the branch.
    let runs_json = gh(&[
        "run",
        "list",
        "--repo",
        &repo,
        "--workflow",
        CI_WORKFLOW,
        "--branch",
        branch,
        "--status",
        "completed",
        "--limit",
        &SCAN_LIMIT.to_string(),
        "--json",
        "databaseId,headSha",
    ])?;
    let runs: Vec<RunListEntry> =
        serde_json::from_str(&runs_json).context("failed to parse `gh run list` output")?;
    if runs.is_empty() {
        bail!("no completed `{CI_WORKFLOW}` runs found on branch `{branch}` in repo `{repo}`");
    }

    // Pick the newest run that has every required report artifact.
    let mut chosen: Option<&RunListEntry> = None;
    for run in &runs {
        if run_has_all_reports(&repo, run.database_id)? {
            chosen = Some(run);
            break;
        }
        eprintln!(
            "  run {}: missing one or more size-gate reports — skipping",
            run.database_id
        );
    }
    let chosen = chosen.with_context(|| {
        format!(
            "none of the last {SCAN_LIMIT} completed `{CI_WORKFLOW}` runs on `{branch}` \
             had all four size-gate reports"
        )
    })?;
    let run_id = chosen.database_id.to_string();

    // Download the reports into a clean directory.
    if download_dir.exists() {
        std::fs::remove_dir_all(download_dir)
            .with_context(|| format!("failed to clear {}", download_dir.display()))?;
    }
    std::fs::create_dir_all(download_dir)
        .with_context(|| format!("failed to create {}", download_dir.display()))?;
    gh(&[
        "run",
        "download",
        &run_id,
        "--repo",
        &repo,
        // `size-gate-*` matches the per-platform reports and NOT the
        // `cargo-timings-size-gate-*` artifacts (different prefix).
        "--pattern",
        "size-gate-*",
        "--dir",
        &download_dir.to_string_lossy(),
    ])
    .with_context(|| format!("failed to download reports from run {run_id}"))?;

    let files = find_json_reports(download_dir)?;
    if files.len() < REQUIRED_ARTIFACTS.len() {
        bail!(
            "expected {} size-gate reports from run {run_id}, found {}",
            REQUIRED_ARTIFACTS.len(),
            files.len()
        );
    }

    Ok(Fetched {
        files,
        run_id,
        head_sha: chosen.head_sha.clone(),
    })
}

/// True if run `run_id` has every artifact in `REQUIRED_ARTIFACTS`.
fn run_has_all_reports(repo: &str, run_id: u64) -> Result<bool> {
    let names = gh(&[
        "api",
        "--paginate",
        &format!("repos/{repo}/actions/runs/{run_id}/artifacts"),
        "--jq",
        ".artifacts[].name",
    ])?;
    let present: Vec<&str> = names.lines().map(str::trim).collect();
    Ok(REQUIRED_ARTIFACTS.iter().all(|need| present.contains(need)))
}

/// Recursively collect `size-gate-*.json` files under `dir`. `gh run
/// download` nests each artifact in its own subdirectory.
fn find_json_reports(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    collect_json(dir, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect_json(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in
        std::fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))?
    {
        let path = entry?.path();
        if path.is_dir() {
            collect_json(&path, out)?;
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("size-gate-"))
            && path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        {
            out.push(path);
        }
    }
    Ok(())
}

/// Run `gh` with `args`, returning stdout. Surfaces a clear error if `gh`
/// is missing or unauthenticated.
fn gh(args: &[&str]) -> Result<String> {
    let output = Command::new("gh").args(args).output().context(
        "failed to run `gh` — install the GitHub CLI and run `gh auth login` \
         (or pass explicit report files instead of --branch)",
    )?;
    if !output.status.success() {
        bail!(
            "`gh {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
