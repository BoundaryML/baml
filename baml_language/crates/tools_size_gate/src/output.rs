use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    compare::{Violation, format_bytes},
    config::GateMetric,
    measure::ArtifactMeasurement,
};

/// A single row of the output report.
pub(crate) struct ReportRow {
    pub artifact: String,
    pub platform: String,
    pub current: ArtifactMeasurement,
    pub baseline: Option<ArtifactMeasurement>,
    pub violations: Vec<Violation>,
    /// True if the platform baseline file exists on disk (even if this artifact isn't in it).
    pub platform_file_exists: bool,
    /// The metric this artifact is gated on (file vs gzip).
    pub gate: GateMetric,
}

impl ReportRow {
    fn has_failure(&self) -> bool {
        !self.violations.is_empty() || self.baseline.is_none()
    }

    /// Current value of the gated metric.
    fn gated_bytes(&self) -> u64 {
        match self.gate {
            GateMetric::File => self.current.file_bytes,
            GateMetric::Gzip => self.current.gzip_bytes,
        }
    }

    /// Baseline value of the gated metric, if a baseline exists.
    fn gated_baseline_bytes(&self) -> Option<u64> {
        self.baseline.as_ref().map(|b| match self.gate {
            GateMetric::File => b.file_bytes,
            GateMetric::Gzip => b.gzip_bytes,
        })
    }
}

/// One-line legend explaining the 🔒 / "gated on" marker, shown beneath
/// every table so the file-vs-gzip distinction is never ambiguous.
const GATE_LEGEND: &str = "🔒 = the size this artifact is GATED on (ceiling + delta). Binaries gate on \
     **file** size (installed binary); WASM gates on **gzip** (download size). \
     The other size is shown for information only.";

/// Plain-text legend for the terminal table.
const GATE_LEGEND_PLAIN: &str = "[*] = GATED metric (ceiling + delta). Binaries gate on file size; WASM gates \
     on gzip. Delta column is measured on the gated metric.";

/// Render a report as a terminal table. The gated metric's value is
/// marked with `[*]`; the Delta column is measured on that metric.
pub(crate) fn render_table(rows: &[ReportRow]) {
    // Header
    println!(
        "{:<20} {:>13} {:>10} {:>13} {:>6} {:>12} {:>10} {:>8}",
        "Artifact", "File", "Stripped", "Gzip", "Gated", "Delta", "Delta%", "Status"
    );
    println!("{}", "-".repeat(96));

    for row in rows {
        // Mark the gated metric's cell with a trailing [*].
        let mark = |is_gated: bool, s: String| if is_gated { format!("{s} [*]") } else { s };
        let file_str = mark(
            row.gate == GateMetric::File,
            format_bytes(row.current.file_bytes),
        );
        let stripped_str = row
            .current
            .stripped_bytes
            .map(format_bytes)
            .unwrap_or_else(|| "-".into());
        let gzip_str = mark(
            row.gate == GateMetric::Gzip,
            format_bytes(row.current.gzip_bytes),
        );

        let (delta_str, delta_pct_str) = delta_strings(row);

        let status = row_status(row);

        println!(
            "{:<20} {:>13} {:>10} {:>13} {:>6} {:>12} {:>10} {:>8}",
            row.artifact,
            file_str,
            stripped_str,
            gzip_str,
            row.gate.label(),
            delta_str,
            delta_pct_str,
            status
        );
    }

    println!("\n{GATE_LEGEND_PLAIN}");
    print_fix_hint(rows);
}

/// Render a report as GitHub-flavored markdown. The 🔒 marks the gated
/// metric's value; Baseline/Delta are measured on that metric.
pub(crate) fn render_markdown(rows: &[ReportRow]) {
    println!("## Size Gate Results\n");
    println!("| Artifact | File | Gzip | Gated on | Delta | Delta% | Status |");
    println!("|----------|------|------|----------|-------|--------|--------|");

    for row in rows {
        let file_str = gated_cell(row, GateMetric::File, format_bytes(row.current.file_bytes));
        let gzip_str = gated_cell(row, GateMetric::Gzip, format_bytes(row.current.gzip_bytes));

        let (delta_str, delta_pct_str) = delta_strings(row);

        let status = row_status_md(row);

        println!(
            "| {:<20} | {} | {} | **{}** | {} | {} | {} |",
            row.artifact,
            file_str,
            gzip_str,
            row.gate.label(),
            delta_str,
            delta_pct_str,
            status
        );
    }
    println!("\n> {GATE_LEGEND}");

    let has_any_failure = rows.iter().any(ReportRow::has_failure);
    if has_any_failure {
        // Print violations
        let has_violations = rows.iter().any(|r| !r.violations.is_empty());
        if has_violations {
            println!("\n### Violations\n");
            for row in rows {
                for v in &row.violations {
                    println!(
                        "- **{}** `{}`: {} exceeds limit of {} (exceeded by {}, policy: `{}`)",
                        row.artifact, v.metric, v.actual, v.limit, v.exceeded_by, v.policy_name
                    );
                }
            }
        }

        // Print missing baselines
        let missing: Vec<_> = rows.iter().filter(|r| r.baseline.is_none()).collect();
        if !missing.is_empty() {
            println!("\n### Missing baselines\n");
            for row in &missing {
                if row.platform_file_exists {
                    println!(
                        "- **{}** — artifact not found in `.ci/size-gate/{}.toml`",
                        row.artifact, row.platform
                    );
                } else {
                    println!(
                        "- **{}** — baseline file `.ci/size-gate/{}.toml` does not exist",
                        row.artifact, row.platform
                    );
                }
            }
        }

        print_fix_hint_md(rows);
    }
}

/// Render a markdown fragment: table rows only (no header/title) for CI composition.
///
/// Output structure (to stdout):
/// 1. Table rows (one per artifact, no `|---|` header — caller adds that)
/// 2. If there are failures, a `<!-- VIOLATIONS -->` marker followed by violation details
/// 3. If there are fix hints, a `<!-- FIX_HINTS -->` marker followed by fix hint details
pub(crate) fn render_markdown_fragment(rows: &[ReportRow]) {
    for row in rows {
        let file_str = gated_cell(row, GateMetric::File, format_bytes(row.current.file_bytes));
        let gzip_str = gated_cell(row, GateMetric::Gzip, format_bytes(row.current.gzip_bytes));

        let (delta_str, delta_pct_str) = delta_strings(row);

        let status = row_status_md(row);

        println!(
            "| {} | {} | {} | **{}** | {} | {} | {} |",
            row.artifact,
            file_str,
            gzip_str,
            row.gate.label(),
            delta_str,
            delta_pct_str,
            status
        );
    }

    let has_failure = rows.iter().any(ReportRow::has_failure);
    if has_failure {
        // Violations
        let has_violations = rows.iter().any(|r| !r.violations.is_empty());
        if has_violations {
            println!("\n<!-- VIOLATIONS -->");
            for row in rows {
                for v in &row.violations {
                    println!(
                        "- **{}** `{}`: {} exceeds limit of {} (exceeded by {}, policy: `{}`)",
                        row.artifact, v.metric, v.actual, v.limit, v.exceeded_by, v.policy_name
                    );
                }
            }
        }

        // Missing baselines
        let missing: Vec<_> = rows.iter().filter(|r| r.baseline.is_none()).collect();
        if !missing.is_empty() {
            println!("\n<!-- MISSING -->");
            for row in &missing {
                if row.platform_file_exists {
                    println!(
                        "- **{}** — artifact not found in `.ci/size-gate/{}.toml`",
                        row.artifact, row.platform
                    );
                } else {
                    println!(
                        "- **{}** — baseline file `.ci/size-gate/{}.toml` does not exist",
                        row.artifact, row.platform
                    );
                }
            }
        }

        // Fix hints
        println!("\n<!-- FIX_HINTS -->");
        print_fix_hint_md(rows);
    }
}

/// JSON output for CI composition via jq.
pub(crate) fn render_json(rows: &[ReportRow]) {
    let artifacts: Vec<JsonArtifact> = rows
        .iter()
        .map(|row| {
            let (delta, delta_pct) = delta_strings(row);
            let status = row_status(row);
            JsonArtifact {
                artifact: row.artifact.clone(),
                platform: row.platform.clone(),
                status: status.to_owned(),
                gate: row.gate.label().to_owned(),
                file_bytes: row.current.file_bytes,
                file_display: format_bytes(row.current.file_bytes),
                stripped_bytes: row.current.stripped_bytes,
                stripped_display: row
                    .current
                    .stripped_bytes
                    .map(format_bytes)
                    .unwrap_or_else(|| "-".into()),
                gzip_bytes: row.current.gzip_bytes,
                gzip_display: format_bytes(row.current.gzip_bytes),
                baseline_file_bytes: row.baseline.as_ref().map(|b| b.file_bytes),
                baseline_gzip_bytes: row.baseline.as_ref().map(|b| b.gzip_bytes),
                delta,
                delta_pct,
                violations: row
                    .violations
                    .iter()
                    .map(|v| JsonViolation {
                        metric: v.metric.clone(),
                        policy_name: v.policy_name.clone(),
                        actual: v.actual.clone(),
                        limit: v.limit.clone(),
                        exceeded_by: v.exceeded_by.clone(),
                    })
                    .collect(),
                baseline_missing: row.baseline.is_none(),
                baseline_file_missing: !row.platform_file_exists,
                baseline_snippet: baseline_toml_snippet(row),
            }
        })
        .collect();

    let report = JsonReport {
        ok: !rows.iter().any(ReportRow::has_failure),
        artifacts,
    };

    println!(
        "{}",
        serde_json::to_string(&report).expect("failed to serialize JSON report")
    );
}

#[derive(Serialize, Deserialize)]
pub(crate) struct JsonReport {
    pub ok: bool,
    pub artifacts: Vec<JsonArtifact>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct JsonArtifact {
    pub artifact: String,
    pub platform: String,
    pub status: String,
    /// The metric this artifact is gated on: `file` or `gzip`. Defaulted
    /// for forward-compat when aggregating an older report.
    #[serde(default)]
    pub gate: String,
    pub file_bytes: u64,
    pub file_display: String,
    pub stripped_bytes: Option<u64>,
    pub stripped_display: String,
    pub gzip_bytes: u64,
    pub gzip_display: String,
    pub baseline_file_bytes: Option<u64>,
    pub baseline_gzip_bytes: Option<u64>,
    pub delta: String,
    pub delta_pct: String,
    pub violations: Vec<JsonViolation>,
    pub baseline_missing: bool,
    pub baseline_file_missing: bool,
    /// TOML snippet to add/update this artifact's baseline.
    pub baseline_snippet: String,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct JsonViolation {
    pub metric: String,
    pub policy_name: String,
    pub actual: String,
    pub limit: String,
    pub exceeded_by: String,
}

/// Returns true if any row has a failure (violation or missing baseline).
pub(crate) fn has_any_failure(rows: &[ReportRow]) -> bool {
    rows.iter().any(ReportRow::has_failure)
}

/// Returns true if any row is missing a baseline.
pub(crate) fn has_missing_baseline(rows: &[ReportRow]) -> bool {
    rows.iter().any(|r| r.baseline.is_none())
}

fn row_status(row: &ReportRow) -> &'static str {
    if row.baseline.is_none() {
        "MISSING"
    } else if row.violations.is_empty() {
        "OK"
    } else {
        "FAIL"
    }
}

fn row_status_md(row: &ReportRow) -> &'static str {
    if row.baseline.is_none() {
        ":warning:"
    } else if row.violations.is_empty() {
        ":white_check_mark:"
    } else {
        ":x:"
    }
}

/// Format a metric cell, prefixing 🔒 when `metric` is the artifact's
/// gated metric so the reader can't miss which size is being enforced.
fn gated_cell(row: &ReportRow, metric: GateMetric, value: String) -> String {
    if row.gate == metric {
        format!("🔒 {value}")
    } else {
        value
    }
}

/// Delta strings are computed on the gated metric (file size for
/// binaries, gzip for WASM) — never a mix.
fn delta_strings(row: &ReportRow) -> (String, String) {
    match (row.gated_baseline_bytes(), row.gated_bytes()) {
        (Some(base), cur) => {
            let delta = cur as i64 - base as i64;
            let pct = if base > 0 {
                ((cur as f64 - base as f64) / base as f64) * 100.0
            } else {
                0.0
            };
            (format_delta_bytes(delta), format!("{pct:+.1}%"))
        }
        (None, _) => ("n/a".into(), "n/a".into()),
    }
}

fn format_delta_bytes(bytes: i64) -> String {
    let abs = bytes.unsigned_abs();
    let sign = if bytes >= 0 { "+" } else { "-" };
    if abs >= 1_000_000 {
        format!("{sign}{:.1} MB", abs as f64 / 1_000_000.0)
    } else if abs >= 1_000 {
        format!("{sign}{:.1} KB", abs as f64 / 1_000.0)
    } else {
        format!("{sign}{abs} B")
    }
}

// ---------------------------------------------------------------------------
// Fix hints
// ---------------------------------------------------------------------------

/// Categorise the violations across all rows.
struct HintContext<'a> {
    has_delta_violations: bool,
    has_absolute_violations: bool,
    /// Rows with missing baselines.
    missing_rows: Vec<&'a ReportRow>,
    /// Rows with delta violations (need baseline update).
    delta_rows: Vec<&'a ReportRow>,
    /// Rows with absolute violations (need policy bump or binary diet).
    absolute_rows: Vec<&'a ReportRow>,
}

fn analyse_rows(rows: &[ReportRow]) -> HintContext<'_> {
    let mut ctx = HintContext {
        has_delta_violations: false,
        has_absolute_violations: false,
        missing_rows: Vec::new(),
        delta_rows: Vec::new(),
        absolute_rows: Vec::new(),
    };

    for row in rows {
        if row.baseline.is_none() {
            ctx.missing_rows.push(row);
        }
        let has_delta = row
            .violations
            .iter()
            .any(|v| v.policy_name.contains("delta"));
        let has_abs = row
            .violations
            .iter()
            .any(|v| !v.policy_name.contains("delta"));
        if has_delta {
            ctx.has_delta_violations = true;
            ctx.delta_rows.push(row);
        }
        if has_abs {
            ctx.has_absolute_violations = true;
            ctx.absolute_rows.push(row);
        }
    }

    ctx
}

/// Generate a TOML snippet showing the new baseline values for an artifact.
fn baseline_toml_snippet(row: &ReportRow) -> String {
    let mut lines = vec![format!("[artifacts.{}]", row.artifact)];
    lines.push(format!("file_bytes = {}", row.current.file_bytes));
    if let Some(stripped) = row.current.stripped_bytes {
        lines.push(format!("stripped_bytes = {stripped}"));
    }
    lines.push(format!("gzip_bytes = {}", row.current.gzip_bytes));
    lines.join("\n")
}

/// Group rows by platform, returning (platform, toml snippets) in sorted order.
fn group_by_platform<'a>(rows: &[&'a ReportRow]) -> Vec<(&'a str, Vec<String>)> {
    let mut map: BTreeMap<&'a str, Vec<String>> = BTreeMap::new();
    for row in rows {
        map.entry(&row.platform)
            .or_default()
            .push(baseline_toml_snippet(row));
    }
    map.into_iter().collect()
}

/// Print fix hints (terminal table format).
fn print_fix_hint(rows: &[ReportRow]) {
    let ctx = analyse_rows(rows);

    let has_anything =
        !ctx.missing_rows.is_empty() || ctx.has_delta_violations || ctx.has_absolute_violations;
    if !has_anything {
        return;
    }

    println!();

    if !ctx.missing_rows.is_empty() {
        let artifacts: Vec<_> = ctx
            .missing_rows
            .iter()
            .map(|r| r.artifact.as_str())
            .collect();
        eprintln!("error: no baseline found for: {}", artifacts.join(", "));
        // Distinguish missing platform files from missing artifact entries
        let missing_files: BTreeSet<_> = ctx
            .missing_rows
            .iter()
            .filter(|r| !r.platform_file_exists)
            .map(|r| &r.platform)
            .collect();
        let missing_entries: Vec<_> = ctx
            .missing_rows
            .iter()
            .filter(|r| r.platform_file_exists)
            .collect();
        for platform in &missing_files {
            eprintln!("  missing file: .ci/size-gate/{platform}.toml");
        }
        for row in &missing_entries {
            eprintln!(
                "  artifact `{}` not found in .ci/size-gate/{}.toml",
                row.artifact, row.platform
            );
        }
        eprintln!();
        eprintln!("  add the following to your baseline files:\n");
        for (platform, snippets) in group_by_platform(&ctx.missing_rows) {
            eprintln!("  # .ci/size-gate/{platform}.toml");
            for snippet in &snippets {
                for line in snippet.as_str().lines() {
                    eprintln!("  {line}");
                }
                eprintln!();
            }
        }
    }

    if ctx.has_delta_violations {
        eprintln!("hint: size grew vs the committed baseline. If this is intentional, adopt the");
        eprintln!("      exact sizes the latest canary CI run measured (no local rebuild; all");
        eprintln!("      platforms at once) and commit the result:");
        eprintln!();
        eprintln!("    cargo run -p cargo-size-gate -- size-gate bake --branch canary");
        eprintln!();
        eprintln!("  (use --branch <name> to adopt from a different branch's latest CI run.)");
    }

    if ctx.has_absolute_violations {
        eprintln!("hint: absolute size limit exceeded:");
        for row in &ctx.absolute_rows {
            for v in &row.violations {
                if v.policy_name.contains("delta") {
                    continue;
                }
                eprintln!(
                    "  {}: {} is {} over the {} limit",
                    row.artifact, v.actual, v.exceeded_by, v.limit
                );
                eprintln!(
                    "    → either reduce the artifact size or raise `artifacts.{}.policy.{}` in .cargo/size-gate.toml",
                    row.artifact, v.policy_name
                );
            }
        }
    }
}

/// Print fix hints (markdown format).
fn print_fix_hint_md(rows: &[ReportRow]) {
    let ctx = analyse_rows(rows);

    println!("\n### How to fix\n");

    if !ctx.missing_rows.is_empty() {
        println!("**Missing baselines** — add the following to your baseline files:\n");
        for (platform, snippets) in group_by_platform(&ctx.missing_rows) {
            println!("**`.ci/size-gate/{platform}.toml`**:");
            println!("```toml");
            for snippet in &snippets {
                println!("{snippet}");
            }
            println!("```\n");
        }
    }

    if ctx.has_delta_violations {
        println!(
            "**Size grew vs the committed baseline.** If this is intentional, adopt the exact \
             sizes the latest canary CI run measured (no local rebuild; all platforms at once) \
             and commit the result:\n"
        );
        println!("```");
        println!("cargo run -p cargo-size-gate -- size-gate bake --branch canary");
        println!("```\n");
        println!(
            "_Use `--branch <name>` to adopt from a different branch's latest CI run. \
             The daily baseline-refresh job does this automatically, so most drift is \
             corrected without any manual step._\n"
        );
    }

    if ctx.has_absolute_violations {
        println!("**Absolute size limit exceeded:**\n");
        for row in &ctx.absolute_rows {
            for v in &row.violations {
                if v.policy_name.contains("delta") {
                    continue;
                }
                println!(
                    "- **{}**: {} is {} over the {} limit — reduce the artifact size or \
                     raise `artifacts.{}.policy.{}` in `.cargo/size-gate.toml`",
                    row.artifact, v.actual, v.exceeded_by, v.limit, row.artifact, v.policy_name
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Aggregate report (reads multiple JSON files, renders unified markdown)
// ---------------------------------------------------------------------------

/// Shorten a platform triple to a human-friendly label.
fn short_platform(platform: &str) -> &str {
    match platform {
        s if s.contains("apple-darwin") => "macOS",
        s if s.contains("linux") => "Linux",
        s if s.contains("windows") => "Windows",
        s if s.contains("wasm") => "WASM",
        other => other,
    }
}

/// Render a unified CodSpeed-style markdown report from multiple JSON reports.
///
/// The `run_url` is optional and used for the footer link to the workflow run.
pub(crate) fn render_aggregate_markdown(reports: &[JsonReport], run_url: Option<&str>) {
    // Merge all artifacts from all reports
    let all_artifacts: Vec<&JsonArtifact> = reports.iter().flat_map(|r| &r.artifacts).collect();

    let fail_count = all_artifacts
        .iter()
        .filter(|a| a.status == "FAIL" || a.status == "MISSING")
        .count();
    let ok_count = all_artifacts.iter().filter(|a| a.status == "OK").count();
    let all_ok = fail_count == 0;

    // Header
    if all_ok {
        println!("# Binary size checks passed");
    } else {
        println!("# Binary size checks failed");
    }
    println!();

    // Summary line
    let mut summary_parts = Vec::new();
    if fail_count > 0 {
        summary_parts.push(format!("❌ **{fail_count}** violations"));
    }
    if ok_count > 0 {
        summary_parts.push(format!("✅ **{ok_count}** passed"));
    }
    println!("{}", summary_parts.join("  ·  "));
    println!();

    // Warning callout
    if !all_ok {
        println!(
            "> ⚠️ Please fix the size gate issues or acknowledge them by updating baselines.\n"
        );
    }

    // Unified table. Each artifact is gated on ONE metric — file size for
    // binaries, gzip for WASM. 🔒 marks that metric's value, and the
    // Baseline/Delta columns are measured on it. The other size is shown
    // for information only.
    println!("| | Artifact | Platform | File | Gzip | Gated on | Baseline | Delta | Status |");
    println!("|---|----------|----------|------|------|----------|----------|-------|--------|");

    for a in &all_artifacts {
        let icon = match a.status.as_str() {
            "OK" => ":white_check_mark:",
            "FAIL" => ":x:",
            _ => ":warning:",
        };
        let gates_gzip = a.gate == "gzip";
        // 🔒 marks the gated metric's value.
        let file_cell = if gates_gzip {
            a.file_display.clone()
        } else {
            format!("🔒 {}", a.file_display)
        };
        let gzip_cell = if gates_gzip {
            format!("🔒 {}", a.gzip_display)
        } else {
            a.gzip_display.clone()
        };
        let baseline_gated = if gates_gzip {
            a.baseline_gzip_bytes
        } else {
            a.baseline_file_bytes
        };
        let baseline_display = baseline_gated.map_or("n/a".into(), format_bytes);
        let platform_label = short_platform(&a.platform);

        println!(
            "| {icon} | `{}` | {platform_label} | {file_cell} | {gzip_cell} | **{}** | {baseline_display} | {} ({}) | {} |",
            a.artifact, a.gate, a.delta, a.delta_pct, a.status
        );
    }
    println!();
    println!("> {GATE_LEGEND}");
    println!();

    // Details section (only if failures)
    if !all_ok {
        println!("<details>");
        println!("<summary>Details & how to fix</summary>");
        println!();

        // Violations
        let violations: Vec<_> = all_artifacts
            .iter()
            .filter(|a| !a.violations.is_empty())
            .collect();
        if !violations.is_empty() {
            println!("**Violations:**\n");
            for a in &violations {
                for v in &a.violations {
                    println!(
                        "- **{}** ({}) `{}`: {} exceeds limit of {} (exceeded by {}, policy: `{}`)",
                        a.artifact,
                        short_platform(&a.platform),
                        v.metric,
                        v.actual,
                        v.limit,
                        v.exceeded_by,
                        v.policy_name
                    );
                }
            }
            println!();
        }

        // Missing baselines
        let missing: Vec<_> = all_artifacts
            .iter()
            .filter(|a| a.baseline_missing)
            .collect();
        if !missing.is_empty() {
            println!("**Missing baselines:**\n");
            for a in &missing {
                if a.baseline_file_missing {
                    println!(
                        "- **{}** — baseline file `.ci/size-gate/{}.toml` does not exist",
                        a.artifact, a.platform
                    );
                } else {
                    println!(
                        "- **{}** — artifact not found in `.ci/size-gate/{}.toml`",
                        a.artifact, a.platform
                    );
                }
            }
            println!();
        }

        // TOML snippets grouped by platform
        let needs_fix: Vec<_> = all_artifacts
            .iter()
            .filter(|a| a.baseline_missing || !a.violations.is_empty())
            .collect();
        if !needs_fix.is_empty() {
            println!("**Add/update baselines:**\n");
            // Group by platform
            let mut by_platform: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
            for a in &needs_fix {
                by_platform
                    .entry(&a.platform)
                    .or_default()
                    .push(&a.baseline_snippet);
            }
            for (platform, snippets) in &by_platform {
                println!("**`.ci/size-gate/{platform}.toml`**:");
                println!("```toml");
                for snippet in snippets {
                    println!("{snippet}");
                }
                println!("```\n");
            }
        }

        println!("</details>");
        println!();
    }

    // Footer
    println!("---");
    if let Some(url) = run_url {
        println!("*Generated by `cargo size-gate` · [workflow run]({url})*");
    } else {
        println!("*Generated by `cargo size-gate`*");
    }
}
