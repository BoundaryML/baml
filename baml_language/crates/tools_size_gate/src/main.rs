#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::exit,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss
)]

mod baseline;
mod ceilings;
mod compare;
mod config;
mod fetch;
mod human_size;
mod measure;
mod output;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use baseline::{PlatformBaseline, baseline_path, current_git_sha, now_iso8601};
use ceilings::sync_ceilings;
use clap::{Parser, Subcommand, ValueEnum};
use compare::check_policy;
use config::{Config, host_triple};
use measure::{ArtifactMeasurement, build_artifact, locate_artifact_public, measure_artifact};
use output::{
    JsonReport, ReportRow, has_any_failure, has_missing_baseline, render_aggregate_markdown,
    render_json, render_markdown, render_markdown_fragment, render_table,
};

/// Exit codes.
const EXIT_OK: i32 = 0;
const EXIT_POLICY_VIOLATED: i32 = 1;
const EXIT_BASELINE_MISSING: i32 = 2;
#[allow(dead_code)]
const EXIT_BUILD_FAILED: i32 = 3;
const EXIT_TOOL_ERROR: i32 = 4;

#[derive(Parser)]
#[command(name = "cargo")]
#[command(bin_name = "cargo")]
enum Cargo {
    SizeGate(Args),
}

#[derive(Parser)]
#[command(version, about = "Binary size gating for Rust workspace artifacts")]
struct Args {
    #[command(subcommand)]
    command: Command,

    /// Output format.
    #[arg(long, global = true, default_value = "table")]
    format: OutputFormat,

    /// Only process these artifacts (comma-separated).
    #[arg(long, global = true, value_delimiter = ',')]
    only: Option<Vec<String>>,

    /// Skip the build step (assume artifacts already exist).
    #[arg(long, global = true)]
    no_build: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Build, measure, and write baseline files.
    Record,

    /// Build, measure, and check against baseline + policies.
    Check,

    /// Show diff between baseline and current measurements (no gating).
    Diff,

    /// Aggregate multiple JSON report files into a unified markdown report.
    ///
    /// Reads JSON files produced by `check --format json` and renders
    /// a single CodSpeed-style markdown report suitable for PR comments.
    Agg {
        /// JSON report files to aggregate.
        files: Vec<PathBuf>,

        /// Optional URL to the CI workflow run (shown in the report footer).
        #[arg(long)]
        run_url: Option<String>,
    },

    /// Adopt CI-measured sizes as the new baseline — WITHOUT rebuilding.
    ///
    /// With `--branch` (the default), fetches the latest CI `size-gate`
    /// reports for that branch via `gh` and adopts them — the one command to
    /// run when a PR trips the gate (no local rebuild, all platforms at once,
    /// exact CI sizes). With explicit report files, adopts those instead
    /// (used by the daily refresh workflow, which already downloaded them).
    ///
    /// Writes `.ci/size-gate/<platform>.toml` from each report's measured
    /// sizes and rewrites the per-platform ceilings in `.cargo/size-gate.toml`
    /// to `--margin-pct` above them as human-readable KiB/MiB/GiB values.
    /// Idempotent: when the sizes already match, nothing is written.
    Bake {
        /// JSON report files produced by `check --format json`. When omitted,
        /// reports are fetched from `--branch` instead.
        files: Vec<PathBuf>,

        /// Adopt the latest CI-measured sizes from this branch (via `gh`).
        /// Ignored when explicit report `files` are given. Defaults to the
        /// `canary` default branch when no files are provided.
        #[arg(long)]
        branch: Option<String>,

        /// Repo to fetch from as `owner/name`. Defaults to the current
        /// checkout's `origin` remote.
        #[arg(long)]
        repo: Option<String>,

        /// Where to download fetched reports. Defaults to a temp directory.
        #[arg(long)]
        download_dir: Option<PathBuf>,

        /// Write a JSON provenance summary (run id, head sha, violations,
        /// changed) to this path — consumed by the refresh workflow.
        #[arg(long)]
        summary_out: Option<PathBuf>,

        /// How far above each recorded size to set the absolute ceiling.
        /// Defaults to 3% — matching the `max_delta_pct` regression guard.
        #[arg(long, default_value = "3.0")]
        margin_pct: f64,

        /// Git SHA the reports were produced from, recorded in each baseline
        /// file. Defaults to the fetched run's head SHA, else the current HEAD.
        #[arg(long)]
        git_sha: Option<String>,
    },
}

#[derive(Clone, ValueEnum)]
enum OutputFormat {
    Table,
    #[value(alias = "md")]
    Markdown,
    /// Markdown fragment: table rows only (no header), for CI composition.
    #[value(alias = "md-fragment")]
    MarkdownFragment,
    /// JSON output for CI composition via jq.
    Json,
}

fn main() {
    let Cargo::SizeGate(args) = Cargo::parse();

    let result = match &args.command {
        Command::Record => cmd_record(&args),
        Command::Check => cmd_check(&args),
        Command::Diff => cmd_diff(&args),
        Command::Agg { files, run_url } => cmd_agg(files, run_url.as_deref()),
        Command::Bake {
            files,
            branch,
            repo,
            download_dir,
            summary_out,
            margin_pct,
            git_sha,
        } => cmd_bake(BakeArgs {
            files,
            branch: branch.as_deref(),
            repo: repo.as_deref(),
            download_dir: download_dir.as_deref(),
            summary_out: summary_out.as_deref(),
            margin_pct: *margin_pct,
            git_sha: git_sha.as_deref(),
        }),
    };

    match result {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("error: {e:#}");
            std::process::exit(EXIT_TOOL_ERROR);
        }
    }
}

/// Find the workspace root via cargo metadata.
fn find_workspace_root() -> Result<PathBuf> {
    let output = std::process::Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .context("failed to run cargo metadata")?;
    if !output.status.success() {
        anyhow::bail!("cargo metadata failed");
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let root = json["workspace_root"]
        .as_str()
        .context("no workspace_root in metadata")?;
    Ok(PathBuf::from(root))
}

/// Determine which artifact names are relevant for the current host.
/// Returns a map: platform -> artifact names.
fn relevant_artifacts(config: &Config, filter: Option<&[String]>) -> BTreeMap<String, Vec<String>> {
    let host = host_triple();
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (name, artifact) in &config.artifacts {
        if let Some(only) = filter {
            if !only.iter().any(|o| o == name) {
                continue;
            }
        }

        let platform = Config::platform_for_artifact(artifact);

        // Only include artifacts buildable on this host
        if !artifact.is_wasm() && platform != host {
            continue;
        }

        groups.entry(platform).or_default().push(name.clone());
    }

    groups
}

/// Build and measure all relevant artifacts.
/// Returns: platform -> list of (name, measurement) pairs.
fn build_and_measure(
    workspace_root: &Path,
    config: &Config,
    args: &Args,
) -> Result<BTreeMap<String, Vec<(String, ArtifactMeasurement)>>> {
    let groups = relevant_artifacts(config, args.only.as_deref());
    let mut results: BTreeMap<String, Vec<(String, ArtifactMeasurement)>> = BTreeMap::new();

    for (platform, artifact_names) in &groups {
        let mut platform_results = Vec::new();

        for name in artifact_names {
            let artifact_config = &config.artifacts[name];
            eprintln!("measuring {name} (platform: {platform})");

            let artifact_path = if args.no_build {
                locate_artifact_public(workspace_root, name, artifact_config)?
            } else {
                build_artifact(workspace_root, name, artifact_config)?
            };

            let measurement = measure_artifact(&artifact_path, artifact_config.should_strip())
                .with_context(|| format!("failed to measure {name}"))?;

            eprintln!(
                "  file: {} | gzip: {}",
                format_bytes_short(measurement.file_bytes),
                format_bytes_short(measurement.gzip_bytes)
            );

            platform_results.push((name.clone(), measurement));
        }

        results.insert(platform.clone(), platform_results);
    }

    Ok(results)
}

fn cmd_record(args: &Args) -> Result<i32> {
    let workspace_root = find_workspace_root()?;
    let config = Config::load(&workspace_root)?;
    let results = build_and_measure(&workspace_root, &config, args)?;

    let git_sha = current_git_sha(&workspace_root);
    let timestamp = now_iso8601();

    for (platform, measurements) in &results {
        let path = baseline_path(&workspace_root, &config.baseline_dir, platform);

        // Load existing baseline to preserve artifacts not being recorded this run
        let mut artifacts = match PlatformBaseline::load(&path)? {
            Some(existing) => existing.artifacts,
            None => BTreeMap::new(),
        };
        for (name, measurement) in measurements {
            artifacts.insert(name.clone(), measurement.clone());
        }

        let baseline = PlatformBaseline {
            version: 1,
            recorded_at: timestamp.clone(),
            git_sha: git_sha.clone(),
            artifacts,
        };

        baseline.save(&path)?;
        eprintln!("wrote baseline: {}", path.display());
    }

    // Print the report
    let rows = build_report_rows(&results, &BTreeMap::new(), &config);
    render_output(args, &rows);

    Ok(EXIT_OK)
}

fn cmd_check(args: &Args) -> Result<i32> {
    let workspace_root = find_workspace_root()?;
    let config = Config::load(&workspace_root)?;
    let results = build_and_measure(&workspace_root, &config, args)?;

    // Load baselines for each platform
    let mut baselines: BTreeMap<String, PlatformBaseline> = BTreeMap::new();

    for platform in results.keys() {
        let path = baseline_path(&workspace_root, &config.baseline_dir, platform);
        if let Some(b) = PlatformBaseline::load(&path)? {
            baselines.insert(platform.clone(), b);
        }
    }

    let rows = build_report_rows(&results, &baselines, &config);
    render_output(args, &rows);

    // Fail on violations first, then missing baselines
    if has_any_failure(&rows) {
        if has_missing_baseline(&rows) {
            return Ok(EXIT_BASELINE_MISSING);
        }
        return Ok(EXIT_POLICY_VIOLATED);
    }
    Ok(EXIT_OK)
}

fn cmd_agg(files: &[PathBuf], run_url: Option<&str>) -> Result<i32> {
    if files.is_empty() {
        eprintln!("warning: no JSON files provided to aggregate");
        // Still produce a valid (empty) report
        render_aggregate_markdown(&[], run_url);
        return Ok(EXIT_OK);
    }

    let mut reports = Vec::new();
    let mut any_failure = false;

    for path in files {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read report: {}", path.display()))?;
        let report: JsonReport = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse JSON report: {}", path.display()))?;
        if !report.ok {
            any_failure = true;
        }
        reports.push(report);
    }

    render_aggregate_markdown(&reports, run_url);

    if any_failure {
        Ok(EXIT_POLICY_VIOLATED)
    } else {
        Ok(EXIT_OK)
    }
}

/// Resolved inputs for `cmd_bake`. Cheap to copy (all fields are borrows
/// or scalars), so pass it by value.
#[derive(Clone, Copy)]
struct BakeArgs<'a> {
    files: &'a [PathBuf],
    branch: Option<&'a str>,
    repo: Option<&'a str>,
    download_dir: Option<&'a Path>,
    summary_out: Option<&'a Path>,
    margin_pct: f64,
    git_sha: Option<&'a str>,
}

/// Adopt CI report measurements as the new baseline without rebuilding.
fn cmd_bake(args: BakeArgs) -> Result<i32> {
    let workspace_root = find_workspace_root()?;
    let config = Config::load(&workspace_root)?;

    // Source the reports: explicit files (CI / advanced use) or fetch the
    // latest CI run for a branch (the default developer path).
    let (files, run_id, head_sha) = if args.files.is_empty() {
        let branch = args.branch.unwrap_or("canary");
        let dir = args
            .download_dir
            .map(Path::to_path_buf)
            .unwrap_or_else(default_download_dir);
        eprintln!("fetching latest size-gate reports from `{branch}`...");
        let fetched = fetch::fetch_branch_reports(branch, args.repo, &dir)?;
        eprintln!("adopting run {} ({})", fetched.run_id, fetched.head_sha);
        (fetched.files, Some(fetched.run_id), Some(fetched.head_sha))
    } else {
        (args.files.to_vec(), None, None)
    };

    // Collect every artifact measurement out of the reports, grouped by
    // platform. A baseline file is per-platform, so several artifacts from
    // the same platform's report land in the same file.
    let mut by_platform: BTreeMap<String, BTreeMap<String, ArtifactMeasurement>> = BTreeMap::new();
    let mut violations = 0u32;
    for path in &files {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read report: {}", path.display()))?;
        let report: JsonReport = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse JSON report: {}", path.display()))?;
        if !report.ok {
            violations += 1;
        }
        for artifact in &report.artifacts {
            let measurement = ArtifactMeasurement {
                file_bytes: artifact.file_bytes,
                stripped_bytes: artifact.stripped_bytes,
                gzip_bytes: artifact.gzip_bytes,
                sections: BTreeMap::new(),
            };
            by_platform
                .entry(artifact.platform.clone())
                .or_default()
                .insert(artifact.artifact.clone(), measurement);
        }
    }

    if by_platform.is_empty() {
        anyhow::bail!("reports contained no artifacts to bake");
    }

    let git_sha = args
        .git_sha
        .map(str::to_owned)
        .or_else(|| head_sha.clone())
        .or_else(|| current_git_sha(&workspace_root));
    let timestamp = now_iso8601();

    // Write each platform baseline, preserving any artifact already in the
    // file that this batch of reports didn't cover. Skip the write entirely
    // when the measured sizes are unchanged — bumping `recorded_at`/`git_sha`
    // on every run would churn a no-op baseline-refresh PR daily.
    let mut changed = false;
    for (platform, artifacts) in &by_platform {
        let path = baseline_path(&workspace_root, &config.baseline_dir, platform);
        let existing = PlatformBaseline::load(&path)?;
        let mut merged = match &existing {
            Some(existing) => existing.artifacts.clone(),
            None => BTreeMap::new(),
        };
        for (name, measurement) in artifacts {
            merged.insert(name.clone(), measurement.clone());
        }

        if existing
            .as_ref()
            .is_some_and(|baseline| baseline.measurements_match(&merged))
        {
            eprintln!("baseline unchanged: {}", path.display());
            continue;
        }

        let baseline = PlatformBaseline {
            version: 1,
            recorded_at: timestamp.clone(),
            git_sha: git_sha.clone(),
            artifacts: merged,
        };
        baseline.save(&path)?;
        changed = true;
        eprintln!("wrote baseline: {}", path.display());
    }

    // Re-peg the absolute ceilings to the freshly recorded sizes. This is a
    // no-op on disk (no diff) when the ceilings already sit margin% above.
    let ceilings_changed = sync_ceilings(&workspace_root, &config, &by_platform, args.margin_pct)?;
    changed = changed || ceilings_changed;

    if let Some(path) = args.summary_out {
        let summary = serde_json::json!({
            "run_id": run_id,
            "head_sha": head_sha.or(git_sha),
            "violations": violations,
            "changed": changed,
        });
        std::fs::write(path, serde_json::to_string_pretty(&summary)?)
            .with_context(|| format!("failed to write summary: {}", path.display()))?;
    }

    if changed {
        eprintln!("baseline updated.");
    } else {
        eprintln!("baseline already up to date — nothing to change.");
    }
    Ok(EXIT_OK)
}

/// Default location for fetched reports when `--download-dir` is unset.
fn default_download_dir() -> PathBuf {
    std::env::temp_dir().join("baml-size-gate-reports")
}

fn cmd_diff(args: &Args) -> Result<i32> {
    let workspace_root = find_workspace_root()?;
    let config = Config::load(&workspace_root)?;
    let results = build_and_measure(&workspace_root, &config, args)?;

    let mut baselines: BTreeMap<String, PlatformBaseline> = BTreeMap::new();
    for platform in results.keys() {
        let path = baseline_path(&workspace_root, &config.baseline_dir, platform);
        if let Some(b) = PlatformBaseline::load(&path)? {
            baselines.insert(platform.clone(), b);
        }
    }

    let rows = build_report_rows(&results, &baselines, &config);
    render_output(args, &rows);

    Ok(EXIT_OK)
}

fn build_report_rows(
    results: &BTreeMap<String, Vec<(String, ArtifactMeasurement)>>,
    baselines: &BTreeMap<String, PlatformBaseline>,
    config: &Config,
) -> Vec<ReportRow> {
    let mut rows = Vec::new();

    for (platform, measurements) in results {
        let platform_baseline = baselines.get(platform);
        let platform_file_exists = platform_baseline.is_some();

        for (name, measurement) in measurements {
            let baseline_measurement = platform_baseline.and_then(|b| b.artifacts.get(name));

            let gate = config
                .artifacts
                .get(name)
                .map(|c| c.gate)
                .unwrap_or_default();
            let violations = if let Some(artifact_config) = config.artifacts.get(name) {
                let policy = artifact_config.effective_policy(platform);
                check_policy(measurement, baseline_measurement, &policy, gate)
            } else {
                Vec::new()
            };

            rows.push(ReportRow {
                artifact: name.clone(),
                platform: platform.clone(),
                current: measurement.clone(),
                baseline: baseline_measurement.cloned(),
                violations,
                platform_file_exists,
                gate,
            });
        }
    }

    rows
}

fn render_output(args: &Args, rows: &[ReportRow]) {
    match args.format {
        OutputFormat::Table => render_table(rows),
        OutputFormat::Markdown => render_markdown(rows),
        OutputFormat::MarkdownFragment => render_markdown_fragment(rows),
        OutputFormat::Json => render_json(rows),
    }
}

fn format_bytes_short(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.1} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{bytes} B")
    }
}
