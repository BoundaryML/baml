#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;

use anyhow::{Context, Result};
use baml_lsp2_actions::{SymbolDescription, describe};
use baml_project::ProjectDatabase;
use baml_workspace::discover_baml_files;
use clap::Args;

#[derive(Args, Clone, Debug)]
pub struct DescribeArgs {
    /// Symbol name to describe (not required with --symbols)
    pub name: Option<String>,

    /// List all symbols in the project
    #[arg(long)]
    pub symbols: bool,

    /// Filter by symbol kind (repeatable): class, enum, function, test, etc.
    #[arg(long, value_delimiter = ',')]
    pub kind: Vec<String>,

    /// Project root directory
    #[arg(long, default_value = ".")]
    pub from: PathBuf,

    /// Soft line budget for output (default 30)
    #[arg(long, default_value_t = 30)]
    pub budget: usize,

    /// Already-seen symbol names to skip (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub history: Vec<String>,

    /// Suppress "see also" hints
    #[arg(long)]
    pub no_hints: bool,

    /// Output results as JSON
    #[arg(long)]
    pub json: bool,
}

impl DescribeArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let from = std::fs::canonicalize(&self.from)
            .with_context(|| format!("Could not resolve path: {}", self.from.display()))?;

        // Set up the compiler database.
        let mut db = ProjectDatabase::new();
        let _project = db.set_project_root(&from);
        let baml_files = discover_baml_files(&from);
        if baml_files.is_empty() {
            eprintln!("No .baml files found in {}", from.display());
            return Ok(crate::ExitCode::Other);
        }

        for file_path in &baml_files {
            let content = std::fs::read_to_string(file_path)
                .with_context(|| format!("Failed to read {}", file_path.display()))?;
            db.add_or_update_file(file_path, &content);
        }

        let source_files = db.get_source_files();

        // ── --symbols mode ──────────────────────────────────────────────────
        if self.symbols {
            let kind_filter = crate::grep_command::parse_kind_filter(&self.kind)?;
            let symbols = baml_lsp2_actions::list_symbols(&db, &source_files, &kind_filter);
            if symbols.is_empty() {
                eprintln!("No symbols found.");
                return Ok(crate::ExitCode::Other);
            }
            for sym in &symbols {
                let rel = relative_path(&sym.file.path(&db), &from);
                let line = line_number_at_offset(sym.file.text(&db), sym.name_span.start().into());
                println!(
                    "{:<16} {:<10} {}:{}",
                    sym.name,
                    sym.kind.as_str(),
                    rel.display(),
                    line,
                );
            }
            return Ok(crate::ExitCode::Success);
        }

        let name = match &self.name {
            Some(n) => n.as_str(),
            None => {
                eprintln!("No symbol name provided. Use --symbols to list all symbols.");
                return Ok(crate::ExitCode::InvalidArgs);
            }
        };

        let descriptions = describe(&db, &source_files, name);

        if descriptions.is_empty() {
            eprintln!("No symbol found: {name}");
            return Ok(crate::ExitCode::Other);
        }

        if self.json {
            let budget = self.budget;
            let json_output: Vec<serde_json::Value> = descriptions
                .iter()
                .map(|d| crate::grep_command::description_to_json(&db, d, budget, &from))
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&json_output)
                    .context("Failed to serialize output as JSON")?
            );
            return Ok(crate::ExitCode::Success);
        }

        let history: std::collections::HashSet<&str> =
            self.history.iter().map(|s| s.as_str()).collect();

        for (i, desc) in descriptions.iter().enumerate() {
            if i > 0 {
                println!();
                println!();
            }
            render_description(&db, desc, self.budget, &history, self.no_hints, &from);
        }

        Ok(crate::ExitCode::Success)
    }
}

/// Render a SymbolDescription to stdout with budget-based output.
pub fn render_description(
    db: &ProjectDatabase,
    desc: &SymbolDescription,
    budget: usize,
    history: &std::collections::HashSet<&str>,
    no_hints: bool,
    project_root: &std::path::Path,
) {
    let file_path = desc.file.path(db);
    let file_text = desc.file.text(db);

    // Line number of the definition.
    let line_num = line_number_at_offset(file_text, desc.name_span.start().into());

    // ── Header ───────────────────────────────────────────────────────────────
    let kind_str = desc.kind.as_str();
    let rel_path = relative_path(&file_path, project_root);
    let path_display = rel_path.display();
    let is_local = matches!(
        desc.kind,
        baml_lsp2_actions::DefinitionKind::Parameter | baml_lsp2_actions::DefinitionKind::Binding
    );
    if desc.kind.is_member() || is_local {
        let container = desc
            .dependencies
            .first()
            .map(|d| format!("{}.", d.name))
            .unwrap_or_default();
        if is_local {
            // For params/bindings, put the type in the header — no separate
            // type line or body needed.
            let ty = desc.resolved_type.as_deref().unwrap_or("unknown");
            println!(
                "── {kind_str}: {container}{} : {ty} ──── {path_display}:{line_num}",
                desc.name
            );
        } else {
            println!(
                "── {kind_str}: {container}{} ──── {path_display}:{line_num}",
                desc.name
            );
        }
    } else {
        println!("── {kind_str} ──────────────────────────────── {path_display}:{line_num}");
    }

    let mut lines_used = 1; // header

    // ── Resolved type ────────────────────────────────────────────────────────
    // Skip for params/bindings (type in header) and classes/enums (body shows fields).
    let skip_type = is_local
        || matches!(
            desc.kind,
            baml_lsp2_actions::DefinitionKind::Class
                | baml_lsp2_actions::DefinitionKind::Enum
                | baml_lsp2_actions::DefinitionKind::Function
                | baml_lsp2_actions::DefinitionKind::Test
                | baml_lsp2_actions::DefinitionKind::TypeAlias
        );
    if !skip_type {
        if let Some(ref ty) = desc.resolved_type {
            println!("  type: {ty}");
            lines_used += 1;
        }
    }

    // ── Docstring ────────────────────────────────────────────────────────────
    if let Some(ref doc) = desc.docstring {
        for line in doc.lines() {
            println!("/// {line}");
            lines_used += 1;
        }
    }

    // ── Shape or full body (depending on budget) ─────────────────────────────
    let body_lines: Vec<&str> = desc.full_body.lines().collect();

    if is_local {
        // For locals, show function body with context around the variable.
        // Find which line within the body contains the variable.
        let var_line_in_body = find_line_in_body(&desc.full_body, desc.item_range, desc.name_span);
        lines_used += render_body_with_context(
            &body_lines,
            var_line_in_body,
            budget.saturating_sub(lines_used),
        );
    } else {
        let available_for_body = budget.saturating_sub(lines_used);

        if body_lines.len() <= available_for_body {
            // Full body fits — show it all.
            for line in &body_lines {
                println!("{line}");
            }
            lines_used += body_lines.len();
        } else if available_for_body >= 5 {
            // Enough room for truncated body (at least header + some content + skip marker).
            let truncated = truncate_body(&body_lines, available_for_body);
            for line in &truncated {
                println!("{line}");
            }
            lines_used += truncated.len();
        } else {
            // Not enough room for truncation — show shape with `{ ... }` elision.
            let elided = shape_with_elision(&desc.shape, &desc.full_body);
            for line in elided.lines() {
                println!("{line}");
            }
            lines_used += elided.lines().count();
        }
    }

    // ── Dependency shapes ────────────────────────────────────────────────────
    for dep in &desc.dependencies {
        if history.contains(dep.name.as_str()) {
            continue;
        }
        if lines_used >= budget {
            break;
        }

        let dep_path = relative_path(&dep.file.path(db), project_root);
        let dep_line = line_number_at_offset(dep.file.text(db), dep.name_span.start().into());
        println!();
        println!(
            "── dep: {} ({}) ──── {}:{dep_line}",
            dep.name,
            dep.kind.as_str(),
            dep_path.display()
        );
        lines_used += 2;
    }

    // ── References ───────────────────────────────────────────────────────────
    if !desc.references.is_empty() && lines_used < budget {
        println!();
        println!(
            "── references ({}) ─────────────────────────",
            desc.references.len()
        );
        lines_used += 2;

        for (refs_printed, r) in desc.references.iter().enumerate() {
            if lines_used >= budget {
                let remaining = desc.references.len() - refs_printed;
                if remaining > 0 {
                    println!("  ... and {remaining} more");
                }
                break;
            }
            let ref_path = relative_path(&r.file.path(db), project_root);
            println!(
                "  {}:{}  {}",
                ref_path.display(),
                r.line_number,
                r.line_text.trim()
            );
            lines_used += 1;
        }
    }

    // ── See also hints ───────────────────────────────────────────────────────
    if !no_hints && !desc.dependencies.is_empty() {
        let mut shown_names: Vec<&str> = vec![&desc.name];
        for dep in &desc.dependencies {
            shown_names.push(&dep.name);
        }
        for h in history {
            if !shown_names.contains(h) {
                shown_names.push(h);
            }
        }

        let unseen_deps: Vec<&baml_lsp2_actions::DepRef> = desc
            .dependencies
            .iter()
            .filter(|d| !history.contains(d.name.as_str()))
            .collect();

        if !unseen_deps.is_empty() {
            println!();
            println!("── see also ───────────────────────────────");
            for dep in &unseen_deps {
                let dep_path = relative_path(&dep.file.path(db), project_root);
                let dep_line =
                    line_number_at_offset(dep.file.text(db), dep.name_span.start().into());
                let history_str = shown_names.join(",");
                println!(
                    "  {:<16} {:<10} {}:{}  → baml describe {} --history {history_str}",
                    dep.name,
                    dep.kind.as_str(),
                    dep_path.display(),
                    dep_line,
                    dep.name,
                );
            }
        }
    }
}

/// Compute 1-based line number from byte offset.
fn line_number_at_offset(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    text[..offset].chars().filter(|&c| c == '\n').count() + 1
}

/// Make a path relative to the project root.
fn relative_path(path: &std::path::Path, root: &std::path::Path) -> std::path::PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

/// Find the 0-based line index within a body where a span starts.
/// `item_range` is the range of the whole body, `name_span` is the variable's span.
fn find_line_in_body(
    body: &str,
    item_range: text_size::TextRange,
    name_span: text_size::TextRange,
) -> usize {
    // Calculate offset of name_span within the body.
    let body_start: usize = item_range.start().into();
    let name_start: usize = name_span.start().into();
    let relative_offset = name_start.saturating_sub(body_start);

    // Count newlines before the relative offset.
    body.chars()
        .take(relative_offset)
        .filter(|&c| c == '\n')
        .count()
}

/// Render body lines with context around a target line, using truncation if needed.
/// Always shows the function header (first line), then context around the target.
/// Returns the number of lines printed.
fn render_body_with_context(
    body_lines: &[&str],
    target_line: usize,
    available_budget: usize,
) -> usize {
    let total_lines = body_lines.len();

    if total_lines == 0 {
        return 0;
    }

    // If everything fits, just print it all.
    if total_lines <= available_budget {
        for line in body_lines {
            println!("{line}");
        }
        return total_lines;
    }

    // Otherwise, always show header + context around target_line with truncation.
    // Reserve up to 2 lines for "... skipped N lines ..." markers.
    let mut lines_printed = 0;

    // Always print the function header (line 0).
    println!("{}", body_lines[0]);
    lines_printed += 1;

    if total_lines == 1 {
        return lines_printed;
    }

    // Calculate how much budget remains for context.
    let remaining_budget = available_budget.saturating_sub(lines_printed + 2); // reserve for skip markers
    if remaining_budget == 0 {
        if total_lines > 1 {
            println!("  ... skipped {} lines ...", total_lines - 1);
            lines_printed += 1;
        }
        return lines_printed;
    }

    // If target is on line 0 (the header), show lines after it.
    if target_line == 0 {
        let end = (1 + remaining_budget).min(total_lines);
        for line in &body_lines[1..end] {
            println!("{line}");
            lines_printed += 1;
        }
        if end < total_lines {
            println!("  ... skipped {} lines ...", total_lines - end);
            lines_printed += 1;
        }
        return lines_printed;
    }

    // Distribute context around target_line (excluding header which is already printed).
    let half = remaining_budget / 2;
    let context_start = target_line.saturating_sub(half).max(1); // don't re-print header
    let context_end = (target_line + half + 1).min(total_lines);

    // Print skip marker if there's a gap after the header.
    if context_start > 1 {
        println!("  ... skipped {} lines ...", context_start - 1);
        lines_printed += 1;
    }

    // Print the context lines.
    for line in &body_lines[context_start..context_end] {
        println!("{line}");
        lines_printed += 1;
    }

    // Print skip marker if there's more after context.
    if context_end < total_lines {
        println!("  ... skipped {} lines ...", total_lines - context_end);
        lines_printed += 1;
    }

    lines_printed
}

/// Truncate a function body to fit within a line budget while preserving key content.
///
/// When the budget is too tight for even truncated output, produce a shape
/// with `{ ... }` appended to indicate a body was elided.
///
/// If the full body contains a `{` block but the shape doesn't end with one,
/// appends ` { ... }`. Otherwise returns the shape as-is.
pub fn shape_with_elision(shape: &str, full_body: &str) -> String {
    let has_block = full_body.contains('{');
    let shape_already_has_block = shape.contains('{');
    if has_block && !shape_already_has_block {
        format!("{shape} {{ ... }}")
    } else {
        shape.to_string()
    }
}

/// Priority:
/// 1. Always show the function header (first line)
/// 2. Preserve `//# ...` annotated comment blocks and their following content
/// 3. Show first `head_lines` and last `tail_lines` of the body
/// 4. Insert "... skipped N lines ..." at the appropriate indentation
pub fn truncate_body(body_lines: &[&str], available_lines: usize) -> Vec<String> {
    if body_lines.len() <= available_lines {
        return body_lines.iter().map(|s| s.to_string()).collect();
    }

    // Find annotated comment blocks (lines starting with `//#` after trimming).
    let mut annotated_ranges: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < body_lines.len() {
        if body_lines[i].trim().starts_with("//#") {
            let start = i;
            // Include the comment and following non-empty, non-comment lines
            // until we hit another //# or a blank line or end of body.
            i += 1;
            while i < body_lines.len() {
                let trimmed = body_lines[i].trim();
                if trimmed.is_empty() || trimmed.starts_with("//#") {
                    break;
                }
                i += 1;
            }
            annotated_ranges.push((start, i));
        } else {
            i += 1;
        }
    }

    // Calculate how many lines annotated blocks consume.
    let annotated_line_count: usize = annotated_ranges
        .iter()
        .map(|(start, end)| end - start)
        .sum();

    // Reserve lines for: header (1), skip messages (up to 2).
    let skip_line_reserve = 2;
    let header_reserve = 1;
    let remaining_for_content = available_lines
        .saturating_sub(header_reserve)
        .saturating_sub(skip_line_reserve)
        .saturating_sub(annotated_line_count);

    // Distribute remaining between head and tail.
    let head_lines_count = remaining_for_content / 2;
    let tail_lines_count = remaining_for_content.saturating_sub(head_lines_count);

    let mut included: std::collections::HashSet<usize> = std::collections::HashSet::new();

    // Always include header (line 0).
    included.insert(0);

    // Include head lines (after header).
    for idx in 1..(1 + head_lines_count).min(body_lines.len()) {
        included.insert(idx);
    }

    // Include tail lines.
    let tail_start = body_lines.len().saturating_sub(tail_lines_count);
    for idx in tail_start..body_lines.len() {
        included.insert(idx);
    }

    // Include annotated ranges.
    for (start, end) in &annotated_ranges {
        for idx in *start..*end {
            included.insert(idx);
        }
    }

    // Build output with skip markers.
    let mut result = Vec::new();
    let mut last_included: Option<usize> = None;

    for (idx, line) in body_lines.iter().enumerate() {
        if included.contains(&idx) {
            // Check if we need a skip marker.
            if let Some(last) = last_included {
                let skipped = idx - last - 1;
                if skipped > 0 {
                    // Detect indentation from the current line.
                    let indent = line.len() - line.trim_start().len();
                    let indent_str: String = line.chars().take(indent).collect();
                    result.push(format!("{}... skipped {} lines ...", indent_str, skipped));
                }
            }
            result.push(line.to_string());
            last_included = Some(idx);
        }
    }

    result
}
