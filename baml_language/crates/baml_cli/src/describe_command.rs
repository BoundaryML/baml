#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;

use anyhow::{Context, Result};
use baml_db::baml_compiler2_hir;
use baml_lsp2_actions::{SymbolDescription, describe};
use baml_project::ProjectDatabase;
use clap::Args;

use crate::project_load::load_project_from;

#[derive(Args, Clone, Debug)]
pub struct DescribeArgs {
    /// Symbol name to describe (not required with --symbols)
    pub name: Option<String>,

    /// List all symbols in the project
    #[arg(long)]
    pub symbols: bool,

    /// Project root directory
    #[arg(long, default_value = ".")]
    pub from: PathBuf,

    /// Soft line budget for output (default 30)
    #[arg(long, default_value_t = 30)]
    pub budget: usize,

    /// Output results as JSON
    #[arg(long)]
    pub json: bool,
}

impl DescribeArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let (db, from, baml_files) = load_project_from(&self.from)?;
        if baml_files.is_empty() {
            eprintln!("No .baml files found in {}", from.display());
            return Ok(crate::ExitCode::Other);
        }

        // ── --symbols deprecation ───────────────────────────────────────────
        if self.symbols {
            eprintln!(
                "Warning: --symbols is deprecated. Use `baml describe` with no arguments instead."
            );
        }

        // ── No name → project-level listing ─────────────────────────────────
        if self.name.is_none() {
            let user_package_id = resolve_user_package_id(&db);
            let entries = baml_lsp2_actions::list_package_items(&db, user_package_id);
            if entries.is_empty() {
                eprintln!("No symbols found.");
                return Ok(crate::ExitCode::Other);
            }
            if self.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&listing_to_json(&db, &entries, &from))
                        .context("Failed to serialize output as JSON")?
                );
            } else {
                render_listing(&entries, &from);
            }
            return Ok(crate::ExitCode::Success);
        }

        let name = self.name.as_deref().unwrap();
        let segments: Vec<&str> = name.split('.').collect();

        let describe_files = baml_compiler2_hir::compiler2_all_files(&db);

        // Detect known builtin package names.
        let builtin_packages = baml_lsp2_actions::non_user_package_names(&db);

        // Check if first segment is a builtin package.
        if builtin_packages.contains(segments[0]) {
            let pkg_id =
                baml_compiler2_hir::package::PackageId::new(&db, baml_db::Name::new(segments[0]));
            if segments.len() == 1 {
                // `baml describe baml` → list entire builtin package
                let entries = baml_lsp2_actions::list_package_items(&db, pkg_id);
                if entries.is_empty() {
                    eprintln!("No symbols found in package: {}", segments[0]);
                    return Ok(crate::ExitCode::Other);
                }
                if self.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&listing_to_json(&db, &entries, &from))
                            .context("Failed to serialize output as JSON")?
                    );
                } else {
                    render_listing(&entries, &from);
                }
                return Ok(crate::ExitCode::Success);
            } else {
                // `baml describe baml.env` → list sub-namespace within builtin package
                let ns_path: Vec<baml_db::Name> =
                    segments[1..].iter().map(baml_db::Name::new).collect();
                if let Some(entries) =
                    baml_lsp2_actions::list_namespace_items(&db, pkg_id, &ns_path)
                {
                    if self.json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&listing_to_json(&db, &entries, &from))
                                .context("Failed to serialize output as JSON")?
                        );
                    } else {
                        render_listing(&entries, &from);
                    }
                    return Ok(crate::ExitCode::Success);
                }
                // Fall through to describe if namespace not found (might be an item).
            }
        }

        // Check if first segment is a user namespace.
        let user_package_id = resolve_user_package_id(&db);
        if segments.len() == 1 {
            // Single segment: check if it's a namespace name.
            let ns_name = baml_db::Name::new(segments[0]);
            let is_user_ns = {
                let user_pkg = baml_compiler2_hir::package::package_items(&db, user_package_id);
                user_pkg
                    .namespaces
                    .keys()
                    .any(|k| k == &vec![ns_name.clone()])
            };
            if is_user_ns {
                if let Some(entries) =
                    baml_lsp2_actions::list_namespace_items(&db, user_package_id, &[ns_name])
                {
                    if self.json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&listing_to_json(&db, &entries, &from))
                                .context("Failed to serialize output as JSON")?
                        );
                    } else {
                        render_listing(&entries, &from);
                    }
                    return Ok(crate::ExitCode::Success);
                }
            }
        }

        // ── Dot-notation resolution for user package items/members ───────────
        if segments.len() >= 2 {
            // Determine if the first segment is a user namespace.
            let ns_name = baml_db::Name::new(segments[0]);
            let is_user_ns = {
                let user_pkg = baml_compiler2_hir::package::package_items(&db, user_package_id);
                user_pkg
                    .namespaces
                    .keys()
                    .any(|k| k == &vec![ns_name.clone()])
            };

            if is_user_ns {
                // segments[0] is a namespace, segments[1] is an item name.
                let item_name = baml_db::Name::new(segments[1]);
                let ns_path = vec![ns_name.clone()];
                let item_def = {
                    let user_pkg = baml_compiler2_hir::package::package_items(&db, user_package_id);
                    user_pkg
                        .lookup_type(&ns_path, &item_name)
                        .or_else(|| user_pkg.lookup_value(&ns_path, &item_name))
                };

                if let Some(def) = item_def {
                    if segments.len() == 2 {
                        // `baml describe llm.Config` → item detail
                        if let Some(desc) =
                            baml_lsp2_actions::describe_by_definition(&db, &describe_files, def)
                        {
                            if self.json {
                                let json = crate::grep_command::description_to_json(
                                    &db,
                                    &desc,
                                    self.budget,
                                    &from,
                                );
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&[json])
                                        .context("Failed to serialize output as JSON")?
                                );
                            } else {
                                render_description(&db, &desc, self.budget, &from);
                            }
                            return Ok(crate::ExitCode::Success);
                        }
                    } else if segments.len() == 3 {
                        // `baml describe llm.Config.name` → member detail
                        if let Some(desc) = baml_lsp2_actions::describe_item_member(
                            &db,
                            &describe_files,
                            def,
                            segments[2],
                        ) {
                            if self.json {
                                let json = crate::grep_command::description_to_json(
                                    &db,
                                    &desc,
                                    self.budget,
                                    &from,
                                );
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&[json])
                                        .context("Failed to serialize output as JSON")?
                                );
                            } else {
                                render_description(&db, &desc, self.budget, &from);
                            }
                            return Ok(crate::ExitCode::Success);
                        }
                    }
                }
            } else {
                // Not a namespace — try as [item, member] in root namespace.
                let item_name = baml_db::Name::new(segments[0]);
                let root_ns: Vec<baml_db::Name> = vec![];
                let item_def = {
                    let user_pkg = baml_compiler2_hir::package::package_items(&db, user_package_id);
                    user_pkg
                        .lookup_type(&root_ns, &item_name)
                        .or_else(|| user_pkg.lookup_value(&root_ns, &item_name))
                };

                if let Some(def) = item_def {
                    if segments.len() == 2 {
                        // `baml describe Point.x` → member detail
                        if let Some(desc) = baml_lsp2_actions::describe_item_member(
                            &db,
                            &describe_files,
                            def,
                            segments[1],
                        ) {
                            if self.json {
                                let json = crate::grep_command::description_to_json(
                                    &db,
                                    &desc,
                                    self.budget,
                                    &from,
                                );
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&[json])
                                        .context("Failed to serialize output as JSON")?
                                );
                            } else {
                                render_description(&db, &desc, self.budget, &from);
                            }
                            return Ok(crate::ExitCode::Success);
                        }
                    }
                }
            }
        }

        // Fall through to existing describe() for single-name lookup.
        let descriptions = describe(&db, &describe_files, name);

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

        for (i, desc) in descriptions.iter().enumerate() {
            if i > 0 {
                println!();
                println!();
            }
            render_description(&db, desc, self.budget, &from);
        }

        Ok(crate::ExitCode::Success)
    }
}

/// Render a SymbolDescription to stdout with budget-based output.
pub fn render_description(
    db: &ProjectDatabase,
    desc: &SymbolDescription,
    budget: usize,
    project_root: &std::path::Path,
) {
    let file_path = desc.file.path(db);
    let file_text = desc.file.text(db);
    let line_num = line_number_at_offset(file_text, desc.name_span.start().into());

    // ── Header: kind name  file:line ────────────────────────────────────────
    let kind_str = desc.kind.as_str();
    let rel_path = relative_path(&file_path, project_root);
    let path_display = rel_path.display();

    println!(
        "{kind_str} {name}  {path_display}:{line_num}",
        name = desc.name
    );

    let mut lines_used = 1;

    // ── Docstring ────────────────────────────────────────────────────────────
    if let Some(ref doc) = desc.docstring {
        println!();
        for line in doc.lines() {
            println!("/// {line}");
            lines_used += 1;
        }
    }

    // ── Body ─────────────────────────────────────────────────────────────────
    let is_local = matches!(
        desc.kind,
        baml_lsp2_actions::DefinitionKind::Parameter | baml_lsp2_actions::DefinitionKind::Binding
    );
    let body_lines: Vec<&str> = desc.full_body.lines().collect();

    if !is_local {
        println!();
        let available_for_body = budget.saturating_sub(lines_used);
        if body_lines.len() <= available_for_body {
            for line in &body_lines {
                println!("{line}");
            }
            lines_used += body_lines.len();
        } else if available_for_body >= 5 {
            let truncated = truncate_body(&body_lines, available_for_body);
            for line in &truncated {
                println!("{line}");
            }
            lines_used += truncated.len();
        } else {
            let elided = shape_with_elision(&desc.shape, &desc.full_body);
            for line in elided.lines() {
                println!("{line}");
            }
            lines_used += elided.lines().count();
        }
    }

    // ── Instance methods ─────────────────────────────────────────────────────
    if !desc.instance_methods.is_empty() {
        println!();
        println!("instance_methods:");
        for m in &desc.instance_methods {
            let m_path = relative_path(&m.file.path(db), project_root);
            let m_line = line_number_at_offset(m.file.text(db), m.name_span.start().into());
            println!(
                "  {:<16} {:<32} {}:{}",
                m.kind.as_str(),
                m.name,
                m_path.display(),
                m_line
            );
        }
    }

    // ── Static methods ───────────────────────────────────────────────────────
    if !desc.static_methods.is_empty() {
        println!();
        println!("static_methods:");
        for m in &desc.static_methods {
            let m_path = relative_path(&m.file.path(db), project_root);
            let m_line = line_number_at_offset(m.file.text(db), m.name_span.start().into());
            println!(
                "  {:<16} {:<32} {}:{}",
                m.kind.as_str(),
                m.name,
                m_path.display(),
                m_line
            );
        }
    }

    // ── Container ────────────────────────────────────────────────────────────
    if let Some(ref c) = desc.container {
        println!();
        println!("container:");
        let c_path = relative_path(&c.file.path(db), project_root);
        let c_line = line_number_at_offset(c.file.text(db), c.name_span.start().into());
        println!(
            "  {:<16} {:<32} {}:{}",
            c.kind.as_str(),
            c.name,
            c_path.display(),
            c_line
        );
    }

    // ── Dependencies ─────────────────────────────────────────────────────────
    if !desc.dependencies.is_empty() {
        println!();
        println!("dependencies:");
        for dep in &desc.dependencies {
            let dep_path = relative_path(&dep.file.path(db), project_root);
            let dep_line = line_number_at_offset(dep.file.text(db), dep.name_span.start().into());
            println!(
                "  {:<16} {:<32} {}:{}",
                dep.kind.as_str(),
                dep.name,
                dep_path.display(),
                dep_line,
            );
        }
    }

    // ── References ───────────────────────────────────────────────────────────
    println!();
    println!("references ({}):", desc.references.len());
    for r in &desc.references {
        let ref_path = relative_path(&r.file.path(db), project_root);
        println!(
            "  {}:{}  {}",
            ref_path.display(),
            r.line_number,
            r.line_text.trim()
        );
    }

    let _ = lines_used; // budget tracking removed with new format
}

/// Resolve the user package's PackageId.
fn resolve_user_package_id(db: &ProjectDatabase) -> baml_compiler2_hir::package::PackageId<'_> {
    baml_compiler2_hir::package::PackageId::new(db, baml_db::Name::new("user"))
}

/// Render a flat listing of entries to stdout.
fn render_listing(entries: &[baml_lsp2_actions::ListingEntry], project_root: &std::path::Path) {
    for entry in entries {
        let rel = relative_path(std::path::Path::new(&entry.file_path), project_root);
        println!(
            "{:<16} {:<32} {}:{}",
            entry.kind.as_str(),
            entry.fqn,
            rel.display(),
            entry.line,
        );
    }
}

/// Convert listing entries to JSON array.
fn listing_to_json(
    db: &ProjectDatabase,
    entries: &[baml_lsp2_actions::ListingEntry],
    project_root: &std::path::Path,
) -> Vec<serde_json::Value> {
    let _ = db; // db not needed for listing JSON, but kept for consistency
    entries
        .iter()
        .map(|entry| {
            let rel = relative_path(std::path::Path::new(&entry.file_path), project_root);
            serde_json::json!({
                "kind": entry.kind.as_str(),
                "name": entry.fqn,
                "file": rel.to_string_lossy(),
                "line": entry.line,
            })
        })
        .collect()
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
