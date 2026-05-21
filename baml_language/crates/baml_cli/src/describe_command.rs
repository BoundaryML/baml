#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{collections::HashMap, path::PathBuf, sync::LazyLock};

use anyhow::{Context, Result};
use baml_db::baml_compiler2_hir;
use baml_lsp2_actions::{ResolvedTarget, SymbolDescription, describe};
use baml_project::ProjectDatabase;
use clap::Args;

use crate::project_load::load_project_from;

#[derive(serde::Deserialize)]
struct BamlKeywordDoc {
    summary: String,
    #[serde(default)]
    syntax: Option<String>,
    #[serde(default)]
    details: Option<String>,
}

#[derive(serde::Deserialize)]
struct TsKeywordDoc {
    message: String,
    #[serde(default)]
    see: Option<String>,
}

static BAML_KEYWORDS: LazyLock<HashMap<String, BamlKeywordDoc>> = LazyLock::new(|| {
    serde_yaml::from_str(baml_builtins2::BAML_KEYWORDS_YAML)
        .expect("failed to parse baml_keywords.yaml")
});

static TS_KEYWORDS: LazyLock<HashMap<String, TsKeywordDoc>> = LazyLock::new(|| {
    serde_yaml::from_str(baml_builtins2::TS_KEYWORDS_YAML)
        .expect("failed to parse ts_keywords.yaml")
});

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

/// Find FQNs across the user and builtin packages that are fuzzy-similar to `name`.
///
/// Used to power "did you mean?" hints when a path doesn't resolve. Returns up
/// to `limit` candidates sorted by Jaro-Winkler similarity (descending).
pub fn suggest_similar(db: &ProjectDatabase, name: &str, limit: usize) -> Vec<String> {
    use baml_compiler2_hir::package::{PackageId, package_items};

    let mut all_paths: Vec<String> = Vec::new();

    // User package: items + namespace dotted paths.
    let user_pkg = PackageId::new(db, baml_db::Name::new("user"));
    for entry in baml_lsp2_actions::list_package_items(db, user_pkg) {
        all_paths.push(entry.fqn());
    }
    let user_pkg_items = package_items(db, user_pkg);
    for ns_path in user_pkg_items.namespaces.keys() {
        if !ns_path.is_empty() {
            all_paths.push(
                ns_path
                    .iter()
                    .map(baml_db::Name::as_str)
                    .collect::<Vec<_>>()
                    .join("."),
            );
        }
    }

    // Builtin packages: bare package name + items + namespaces (prefixed).
    for pkg_name in baml_lsp2_actions::non_user_package_names(db) {
        all_paths.push(pkg_name.clone());
        let pkg = PackageId::new(db, baml_db::Name::new(&pkg_name));
        for entry in baml_lsp2_actions::list_package_items(db, pkg) {
            all_paths.push(format!("{}.{}", pkg_name, entry.fqn()));
        }
        let pkg_info = package_items(db, pkg);
        for ns_path in pkg_info.namespaces.keys() {
            if !ns_path.is_empty() {
                let dotted = ns_path
                    .iter()
                    .map(baml_db::Name::as_str)
                    .collect::<Vec<_>>()
                    .join(".");
                all_paths.push(format!("{pkg_name}.{dotted}"));
            }
        }
    }

    // Match case-insensitively: agents shouldn't need to remember casing to
    // get a useful "did you mean?" hint. `strsim::jaro_winkler` is case-
    // sensitive, so we lowercase both sides before scoring.
    let needle_lower = name.to_ascii_lowercase();
    let mut scored: Vec<(f64, String)> = all_paths
        .into_iter()
        .map(|p| {
            let p_lower = p.to_ascii_lowercase();
            // Jaro-Winkler on lowercased strings handles typos; substring
            // presence is an extra boost for cases like "Confg" → "Config".
            let mut score = strsim::jaro_winkler(&p_lower, &needle_lower);
            if p_lower.contains(&needle_lower) {
                score += 0.15;
            }
            (score, p)
        })
        .filter(|(s, _)| *s > 0.7)
        .collect();

    // Sort by score desc, then alphabetically for stability.
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });
    // Dedup adjacent duplicates after sort.
    scored.dedup_by(|a, b| a.1 == b.1);
    scored.into_iter().take(limit).map(|(_, p)| p).collect()
}

/// Print a "Did you mean?" hint for `name` to stderr if any similar paths exist.
fn print_did_you_mean(db: &ProjectDatabase, name: &str) {
    let suggestions = suggest_similar(db, name, 5);
    if !suggestions.is_empty() {
        eprintln!();
        eprintln!("Did you mean:");
        for s in suggestions {
            eprintln!("  {s}");
        }
    }
}

/// Dispatch a name string to a `ResolvedTarget`.
///
/// Handles the package-name prefix routing (user vs. builtin packages) and
/// delegates within-package path resolution to `baml_lsp2_actions::resolve_target`.
///
/// - Empty string → `Package(user)`
/// - `"baml"` → `Package(baml)`
/// - `"baml.env"` → `resolve_target(baml_pkg, "env")` → `Namespace`
/// - `"foo.bar.Baz"` → `resolve_target(user_pkg, "foo.bar.Baz")` → `Item`
pub fn dispatch<'db>(db: &'db ProjectDatabase, name: &str) -> Option<ResolvedTarget<'db>> {
    if name.is_empty() {
        let user_pkg = baml_compiler2_hir::package::PackageId::new(db, baml_db::Name::new("user"));
        return Some(ResolvedTarget::Package(user_pkg));
    }

    // Check for keyword (BAML or TS/JS crosswalk) before package routing.
    if BAML_KEYWORDS.contains_key(name) || TS_KEYWORDS.contains_key(name) {
        return Some(ResolvedTarget::Keyword(name.to_string()));
    }

    // Force user-package resolution with `root.` prefix.
    if let Some(rest) = name.strip_prefix("root.") {
        let user_pkg = baml_compiler2_hir::package::PackageId::new(db, baml_db::Name::new("user"));
        return baml_lsp2_actions::resolve_target(db, user_pkg, rest);
    }

    let (first, rest) = name.split_once('.').unwrap_or((name, ""));

    // Builtin package shadows user namespace with same name.
    let builtin_packages = baml_lsp2_actions::non_user_package_names(db);
    if builtin_packages.contains(first) {
        let pkg = baml_compiler2_hir::package::PackageId::new(db, baml_db::Name::new(first));
        return if rest.is_empty() {
            Some(ResolvedTarget::Package(pkg))
        } else {
            baml_lsp2_actions::resolve_target(db, pkg, rest)
        };
    }

    // User package.
    let user_pkg = baml_compiler2_hir::package::PackageId::new(db, baml_db::Name::new("user"));
    baml_lsp2_actions::resolve_target(db, user_pkg, name)
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
                "warning: --symbols is deprecated. Use `baml describe` with no arguments instead."
            );
        }

        let name = self.name.as_deref().unwrap_or("");
        let target = dispatch(&db, name);

        match target {
            Some(ResolvedTarget::Keyword(ref kw)) => {
                if self.json {
                    let json = render_keyword_json(kw);
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json)
                            .context("Failed to serialize keyword output as JSON")?
                    );
                } else {
                    render_keyword(kw);
                }
                Ok(crate::ExitCode::Success)
            }
            Some(ResolvedTarget::Package(pkg)) => {
                let entries = baml_lsp2_actions::list_package_items(&db, pkg);
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
                // Check for name collision: does the user also have an item
                // matching the package name? Only hint when the resolved package
                // is a builtin (i.e., the bare name matches a non-user package).
                let pkg_name = name.split('.').next().unwrap_or(name);
                let builtin_names = baml_lsp2_actions::non_user_package_names(&db);
                if builtin_names.contains(pkg_name) {
                    let user_pkg = baml_compiler2_hir::package::PackageId::new(
                        &db,
                        baml_db::Name::new("user"),
                    );
                    if baml_lsp2_actions::resolve_target(&db, user_pkg, pkg_name).is_some() {
                        eprintln!();
                        eprintln!(
                            "Note: your project also defines `{pkg_name}`. \
                             Use `baml describe root.{pkg_name}` to see your definition."
                        );
                    }
                }
                Ok(crate::ExitCode::Success)
            }
            Some(ResolvedTarget::Namespace { package, ns_path }) => {
                let entries = baml_lsp2_actions::list_namespace_items(&db, package, &ns_path)
                    .unwrap_or_default();
                if entries.is_empty() {
                    eprintln!("No symbols found in namespace.");
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
                Ok(crate::ExitCode::Success)
            }
            Some(ResolvedTarget::Item(def)) => {
                let describe_files = baml_compiler2_hir::compiler2_all_files(&db);
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
                    Ok(crate::ExitCode::Success)
                } else {
                    eprintln!("No symbol found: {name}");
                    print_did_you_mean(&db, name);
                    Ok(crate::ExitCode::Other)
                }
            }
            Some(ResolvedTarget::Member {
                parent,
                member_name,
            }) => {
                let describe_files = baml_compiler2_hir::compiler2_all_files(&db);
                if let Some(desc) = baml_lsp2_actions::describe_item_member(
                    &db,
                    &describe_files,
                    parent,
                    member_name.as_str(),
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
                    Ok(crate::ExitCode::Success)
                } else {
                    eprintln!("No symbol found: {name}");
                    print_did_you_mean(&db, name);
                    Ok(crate::ExitCode::Other)
                }
            }
            None => {
                // Substring fallback (existing behavior for unresolved names).
                let describe_files = baml_compiler2_hir::compiler2_all_files(&db);
                let descriptions = describe(&db, &describe_files, name);

                if descriptions.is_empty() {
                    eprintln!("No symbol found: {name}");
                    print_did_you_mean(&db, name);
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
    }
}

/// Render keyword documentation to a writer.
pub fn write_keyword(w: &mut impl std::io::Write, name: &str) -> std::io::Result<()> {
    if let Some(doc) = BAML_KEYWORDS.get(name) {
        writeln!(w, "{name} — {}", doc.summary)?;
        if let Some(ref syntax) = doc.syntax {
            writeln!(w)?;
            writeln!(w, "Syntax:")?;
            for line in syntax.trim_end().lines() {
                writeln!(w, "  {line}")?;
            }
        }
        if let Some(ref details) = doc.details {
            writeln!(w)?;
            writeln!(w, "{details}")?;
        }
    } else if let Some(doc) = TS_KEYWORDS.get(name) {
        writeln!(w, "{name} — {}", doc.message)?;
        if let Some(ref see) = doc.see {
            writeln!(w)?;
            writeln!(w, "See: baml describe {see}")?;
        }
    }
    Ok(())
}

/// Render keyword documentation to stdout.
fn render_keyword(name: &str) {
    let _ = write_keyword(&mut std::io::stdout(), name);
}

/// Render keyword documentation as JSON.
fn render_keyword_json(name: &str) -> serde_json::Value {
    if let Some(doc) = BAML_KEYWORDS.get(name) {
        serde_json::json!({
            "type": "keyword",
            "name": name,
            "summary": doc.summary,
            "syntax": doc.syntax,
            "details": doc.details,
        })
    } else if let Some(doc) = TS_KEYWORDS.get(name) {
        serde_json::json!({
            "type": "crosswalk",
            "name": name,
            "message": doc.message,
            "see": doc.see,
        })
    } else {
        serde_json::json!(null)
    }
}

/// Render a SymbolDescription to stdout with budget-based output.
pub fn render_description(
    db: &ProjectDatabase,
    desc: &SymbolDescription,
    budget: usize,
    project_root: &std::path::Path,
) {
    let _ = write_description(&mut std::io::stdout(), db, desc, budget, project_root);
}

/// Render a SymbolDescription to a writer with budget-based output.
pub fn write_description(
    w: &mut impl std::io::Write,
    db: &ProjectDatabase,
    desc: &SymbolDescription,
    budget: usize,
    project_root: &std::path::Path,
) -> std::io::Result<()> {
    let file_path = desc.file.path(db);
    let file_text = desc.file.text(db);
    let line_num = line_number_at_offset(file_text, desc.name_span.start().into());

    // ── Header: kind name  file:line ────────────────────────────────────────
    let kind_str = desc.kind.as_str();
    let rel_path = relative_path(&file_path, project_root);
    let path_display = rel_path.display();

    writeln!(
        w,
        "{kind_str} {name}  {path_display}:{line_num}",
        name = desc.name
    )?;

    let mut lines_used = 1;

    // ── Body ─────────────────────────────────────────────────────────────────
    // The body slice already includes any leading `///` doc-comments
    // (the CST parser attaches them as the first children of the item
    // node, so they fall inside `node.text_range()` and round-trip
    // through `slice_text`). Rendering `desc.docstring` separately
    // would just duplicate the same lines, so leave the docstring
    // surfacing to JSON consumers and let the body show it once.
    let is_local = matches!(
        desc.kind,
        baml_lsp2_actions::DefinitionKind::Parameter | baml_lsp2_actions::DefinitionKind::Binding
    );
    let body_lines: Vec<&str> = desc.full_body.lines().collect();

    if !is_local {
        writeln!(w)?;
        let available_for_body = budget.saturating_sub(lines_used);
        let was_truncated = body_lines.len() > available_for_body;
        let shown_body_lines;
        if body_lines.len() <= available_for_body {
            for line in &body_lines {
                writeln!(w, "{line}")?;
            }
            shown_body_lines = body_lines.len();
            lines_used += shown_body_lines;
        } else if available_for_body >= 5 {
            let truncated = truncate_body(&body_lines, available_for_body);
            for line in &truncated {
                writeln!(w, "{line}")?;
            }
            shown_body_lines = truncated.len();
            lines_used += shown_body_lines;
        } else {
            let elided_text = shape_with_elision(&desc.shape, &desc.full_body);
            let elided_lines: Vec<&str> = elided_text.lines().collect();
            for line in &elided_lines {
                writeln!(w, "{line}")?;
            }
            shown_body_lines = elided_lines.len();
            lines_used += shown_body_lines;
        }
        if was_truncated {
            writeln!(w)?;
            writeln!(
                w,
                "[INFO] Showing {shown} of {total} lines. Use --budget {needed} for full output.",
                shown = shown_body_lines,
                total = body_lines.len(),
                needed = body_lines.len() + 1,
            )?;
        }
    }

    // ── Instance methods ─────────────────────────────────────────────────────
    if !desc.instance_methods.is_empty() {
        writeln!(w)?;
        writeln!(w, "instance_methods:")?;
        for m in &desc.instance_methods {
            let m_path = relative_path(&m.file.path(db), project_root);
            let m_line = line_number_at_offset(m.file.text(db), m.name_span.start().into());
            writeln!(
                w,
                "  {:<16} {:<32} {}:{}",
                m.kind.as_str(),
                m.name,
                m_path.display(),
                m_line
            )?;
        }
    }

    // ── Static methods ───────────────────────────────────────────────────────
    if !desc.static_methods.is_empty() {
        writeln!(w)?;
        writeln!(w, "static_methods:")?;
        for m in &desc.static_methods {
            let m_path = relative_path(&m.file.path(db), project_root);
            let m_line = line_number_at_offset(m.file.text(db), m.name_span.start().into());
            writeln!(
                w,
                "  {:<16} {:<32} {}:{}",
                m.kind.as_str(),
                m.name,
                m_path.display(),
                m_line
            )?;
        }
    }

    // ── Container ────────────────────────────────────────────────────────────
    if let Some(ref c) = desc.container {
        writeln!(w)?;
        writeln!(w, "container:")?;
        let c_path = relative_path(&c.file.path(db), project_root);
        let c_line = line_number_at_offset(c.file.text(db), c.name_span.start().into());
        writeln!(
            w,
            "  {:<16} {:<32} {}:{}",
            c.kind.as_str(),
            c.name,
            c_path.display(),
            c_line
        )?;
    }

    // ── Dependencies ─────────────────────────────────────────────────────────
    if !desc.dependencies.is_empty() {
        writeln!(w)?;
        writeln!(w, "dependencies:")?;
        for dep in &desc.dependencies {
            let dep_path = relative_path(&dep.file.path(db), project_root);
            let dep_line = line_number_at_offset(dep.file.text(db), dep.name_span.start().into());
            writeln!(
                w,
                "  {:<16} {:<32} {}:{}",
                dep.kind.as_str(),
                dep.name,
                dep_path.display(),
                dep_line,
            )?;
        }
    }

    // ── References ───────────────────────────────────────────────────────────
    writeln!(w)?;
    writeln!(w, "references ({}):", desc.references.len())?;
    for r in &desc.references {
        let ref_path = relative_path(&r.file.path(db), project_root);
        writeln!(
            w,
            "  {}:{}  {}",
            ref_path.display(),
            r.line_number,
            r.line_text.trim()
        )?;
    }

    let _ = lines_used; // budget tracking removed with new format
    Ok(())
}

/// Render a flat listing of entries to stdout.
fn render_listing(entries: &[baml_lsp2_actions::ListingEntry], project_root: &std::path::Path) {
    let _ = write_listing(&mut std::io::stdout(), entries, project_root);
}

/// Render a flat listing of entries to a writer.
pub fn write_listing(
    w: &mut impl std::io::Write,
    entries: &[baml_lsp2_actions::ListingEntry],
    project_root: &std::path::Path,
) -> std::io::Result<()> {
    for entry in entries {
        let rel = relative_path(std::path::Path::new(&entry.file_path), project_root);
        writeln!(
            w,
            "{:<16} {:<32} {}:{}",
            entry.kind.as_str(),
            entry.fqn(),
            rel.display(),
            entry.line,
        )?;
    }
    Ok(())
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
                "name": entry.fqn(),
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
            println!("  [... skipped {} lines ...]", total_lines - 1);
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
            println!("  [... skipped {} lines ...]", total_lines - end);
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
        println!("  [... skipped {} lines ...]", context_start - 1);
        lines_printed += 1;
    }

    // Print the context lines.
    for line in &body_lines[context_start..context_end] {
        println!("{line}");
        lines_printed += 1;
    }

    // Print skip marker if there's more after context.
    if context_end < total_lines {
        println!("  [... skipped {} lines ...]", total_lines - context_end);
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
                    result.push(format!("{}[... skipped {} lines ...]", indent_str, skipped));
                }
            }
            result.push(line.to_string());
            last_included = Some(idx);
        }
    }

    result
}
