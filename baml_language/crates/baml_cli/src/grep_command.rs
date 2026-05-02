#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;

use anyhow::{Context, Result};
use baml_lsp2_actions::{
    DefinitionKind, GrepMode, GrepOptions, MatchAnnotation, SymbolDescription, TextMatch, describe,
    grep, list_symbols,
};
use baml_project::ProjectDatabase;
use clap::Args;

use crate::project_load::load_project_from;

#[derive(Args, Clone, Debug)]
pub struct GrepArgs {
    /// Search pattern (symbol name or text pattern).
    /// Not required when --symbols is used.
    pub pattern: Option<String>,

    // ── Semantic mode flags ──────────────────────────────────────────────────
    /// Show the definition of a symbol
    #[arg(long)]
    pub def: bool,

    /// Show all references/usages of a symbol
    #[arg(long)]
    pub refs: bool,

    /// List all symbols in the project
    #[arg(long)]
    pub symbols: bool,

    /// Filter by symbol kind (repeatable): class, enum, function, test,
    /// client, type_alias, template_string, retry_policy, generator, let
    #[arg(long, value_delimiter = ',')]
    pub kind: Vec<String>,

    // ── Budget/history ───────────────────────────────────────────────────────
    /// Soft line budget for output (default 30)
    #[arg(long, default_value_t = 30)]
    pub budget: usize,

    /// Already-seen symbol names (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub history: Vec<String>,

    /// Suppress "see also" hints
    #[arg(long)]
    pub no_hints: bool,

    /// Output results as JSON
    #[arg(long)]
    pub json: bool,

    // ── Standard grep flags ─────────────────────────────────────────────────
    /// Case-insensitive matching
    #[arg(short = 'i', long)]
    pub ignore_case: bool,

    // ── Project ─────────────────────────────────────────────────────────────
    /// Project root directory
    #[arg(long, default_value = ".")]
    pub from: PathBuf,
}

impl GrepArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let (db, from, baml_files) = load_project_from(&self.from)?;
        if baml_files.is_empty() {
            eprintln!("No .baml files found in {}", from.display());
            return Ok(crate::ExitCode::Other);
        }

        let source_files = db.get_source_files();
        let kind_filter = parse_kind_filter(&self.kind)?;

        // ── --symbols mode ──────────────────────────────────────────────────
        if self.symbols {
            let symbols = list_symbols(&db, &source_files, &kind_filter);
            if symbols.is_empty() {
                eprintln!("No symbols found.");
                return Ok(crate::ExitCode::Other);
            }
            if self.json {
                let json_output: Vec<serde_json::Value> = symbols
                    .iter()
                    .map(|sym| {
                        let rel = relative_path(&sym.file.path(&db), &from);
                        serde_json::json!({
                            "name": sym.name,
                            "kind": sym.kind.as_str(),
                            "file": rel.to_string_lossy(),
                            "line": line_number_at_offset(
                                sym.file.text(&db),
                                sym.name_span.start().into(),
                            ),
                        })
                    })
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json_output)
                        .context("Failed to serialize output as JSON")?
                );
                return Ok(crate::ExitCode::Success);
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

        let pattern = match &self.pattern {
            Some(p) => p.as_str(),
            None => {
                eprintln!("No pattern provided. Use --symbols to list all symbols.");
                return Ok(crate::ExitCode::InvalidArgs);
            }
        };

        // ── --def mode ──────────────────────────────────────────────────────
        if self.def {
            let descriptions = describe(&db, &source_files, pattern);
            if descriptions.is_empty() {
                eprintln!("No symbol found: {pattern}");
                return Ok(crate::ExitCode::Other);
            }
            if self.json {
                let budget = self.budget;
                let json_output: Vec<serde_json::Value> = descriptions
                    .iter()
                    .map(|d| description_to_json(&db, d, budget, &from))
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
            return Ok(crate::ExitCode::Success);
        }

        // ── --refs mode ─────────────────────────────────────────────────────
        if self.refs {
            let descriptions = describe(&db, &source_files, pattern);
            if descriptions.is_empty() {
                eprintln!("No symbol found: {pattern}");
                return Ok(crate::ExitCode::Other);
            }
            if self.json {
                let json_output: Vec<serde_json::Value> = descriptions
                    .iter()
                    .map(|d| {
                        let file_path = relative_path(&d.file.path(&db), &from);
                        serde_json::json!({
                            "name": d.name,
                            "kind": d.kind.as_str(),
                            "file": file_path.to_string_lossy(),
                            "line": line_number_at_offset(d.file.text(&db), d.name_span.start().into()),
                            "references": d.references.iter().map(|r| {
                                let ref_path = relative_path(&r.file.path(&db), &from);
                                serde_json::json!({
                                    "file": ref_path.to_string_lossy(),
                                    "line": r.line_number,
                                    "text": r.line_text.trim(),
                                })
                            }).collect::<Vec<_>>(),
                        })
                    })
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
                }
                render_refs_only(&db, desc, &from);
            }
            return Ok(crate::ExitCode::Success);
        }

        // ── Smart default: semantic if symbol exists, else text search ──────
        let opts = GrepOptions {
            pattern,
            ignore_case: self.ignore_case,
            kind_filter: &kind_filter,
        };

        let result = grep(&db, &source_files, &opts);

        if self.json {
            let json_output: serde_json::Value = match result.mode {
                GrepMode::Semantic => {
                    let budget = self.budget;
                    serde_json::json!({
                        "mode": "semantic",
                        "results": result.descriptions
                            .iter()
                            .map(|d| description_to_json(&db, d, budget, &from))
                            .collect::<Vec<_>>(),
                    })
                }
                GrepMode::TextSearch => {
                    serde_json::json!({
                        "mode": "text_search",
                        "results": result.text_matches
                            .iter()
                            .map(|m| text_match_to_json(&db, m, &from))
                            .collect::<Vec<_>>(),
                    })
                }
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&json_output)
                    .context("Failed to serialize output as JSON")?
            );
            return Ok(crate::ExitCode::Success);
        }

        match result.mode {
            GrepMode::Semantic => {
                if result.descriptions.is_empty() {
                    // Symbol exists but was filtered out by --kind.
                    eprintln!("No symbol found matching pattern and kind filter: {pattern}");
                    return Ok(crate::ExitCode::Other);
                }
                let history: std::collections::HashSet<&str> =
                    self.history.iter().map(|s| s.as_str()).collect();
                for (i, desc) in result.descriptions.iter().enumerate() {
                    if i > 0 {
                        println!();
                        println!();
                    }
                    render_description(&db, desc, self.budget, &history, self.no_hints, &from);
                }
            }
            GrepMode::TextSearch => {
                if result.text_matches.is_empty() {
                    eprintln!("No matches found for: {pattern}");
                    return Ok(crate::ExitCode::Other);
                }
                render_text_matches(&db, &result.text_matches, &from);
            }
        }

        Ok(crate::ExitCode::Success)
    }
}

// ── Rendering ────────────────────────────────────────────────────────────────

/// Render a full symbol description (reuses the describe_command renderer).
fn render_description(
    db: &ProjectDatabase,
    desc: &SymbolDescription,
    budget: usize,
    _history: &std::collections::HashSet<&str>,
    _no_hints: bool,
    project_root: &std::path::Path,
) {
    // Delegate to the describe_command renderer.
    crate::describe_command::render_description(db, desc, budget, project_root);
}

/// Render only the references section for a symbol.
fn render_refs_only(
    db: &ProjectDatabase,
    desc: &SymbolDescription,
    project_root: &std::path::Path,
) {
    let file_path = relative_path(&desc.file.path(db), project_root);
    let line_num = line_number_at_offset(desc.file.text(db), desc.name_span.start().into());

    // Show which symbol's refs we're listing.
    let kind_str = desc.kind.as_str();
    println!(
        "── {} references to {} ({kind_str}) ──── {}:{line_num}",
        desc.references.len(),
        desc.name,
        file_path.display(),
    );

    for r in &desc.references {
        let ref_path = relative_path(&r.file.path(db), project_root);
        println!(
            "  {}:{}  {}",
            ref_path.display(),
            r.line_number,
            r.line_text.trim()
        );
    }
}

/// Render text search matches grouped by file.
fn render_text_matches(
    db: &ProjectDatabase,
    matches: &[baml_lsp2_actions::TextMatch],
    project_root: &std::path::Path,
) {
    // Group matches by file.
    let mut current_file: Option<baml_db::SourceFile> = None;

    for m in matches {
        if current_file != Some(m.file) {
            if current_file.is_some() {
                println!();
            }
            let rel = relative_path(&m.file.path(db), project_root);
            println!("── {} ─────────────────────────────", rel.display());
            current_file = Some(m.file);
        }

        let annotation = match &m.annotation {
            Some(MatchAnnotation::Definition { kind }) => {
                format!("  ← definition ({})", kind.as_str())
            }
            Some(MatchAnnotation::Reference {
                target_name,
                target_kind,
            }) => {
                format!(
                    "  ← reference to {} ({})",
                    target_name,
                    target_kind.as_str()
                )
            }
            None => String::new(),
        };

        println!(
            " {:>4}│ {}{}",
            m.line_number,
            m.line_text.trim_end(),
            annotation,
        );
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Parse kind filter strings into DefinitionKind values.
pub fn parse_kind_filter(kinds: &[String]) -> Result<Vec<DefinitionKind>> {
    kinds
        .iter()
        .map(|s| match s.as_str() {
            "class" => Ok(DefinitionKind::Class),
            "enum" => Ok(DefinitionKind::Enum),
            "function" => Ok(DefinitionKind::Function),
            "test" => Ok(DefinitionKind::Test),
            "client" => Ok(DefinitionKind::Client),
            "type_alias" => Ok(DefinitionKind::TypeAlias),
            "template_string" => Ok(DefinitionKind::TemplateString),
            "retry_policy" => Ok(DefinitionKind::RetryPolicy),
            "generator" => Ok(DefinitionKind::Generator),
            "let" => Ok(DefinitionKind::Let),
            "field" => Ok(DefinitionKind::Field),
            "variant" => Ok(DefinitionKind::Variant),
            other => anyhow::bail!(
                "Unknown kind: {other}. Valid kinds: class, enum, function, test, client, type_alias, template_string, retry_policy, generator, let, field, variant"
            ),
        })
        .collect()
}

fn relative_path(path: &std::path::Path, root: &std::path::Path) -> std::path::PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

fn line_number_at_offset(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    text[..offset].chars().filter(|&c| c == '\n').count() + 1
}

/// Choose the appropriate body representation based on budget.
///
/// - If the full body fits in `budget` lines, return it as-is.
/// - If at least 5 lines are available, return a truncated body.
/// - Otherwise, return just the shape (compact signature).
fn budget_body(desc: &SymbolDescription, budget: usize) -> String {
    let body_lines: Vec<&str> = desc.full_body.lines().collect();

    if body_lines.len() <= budget {
        desc.full_body.clone()
    } else if budget >= 5 {
        crate::describe_command::truncate_body(&body_lines, budget).join("\n")
    } else {
        crate::describe_command::shape_with_elision(&desc.shape, &desc.full_body)
    }
}

/// Build a JSON value for a `TextMatch`.
fn text_match_to_json(
    db: &ProjectDatabase,
    m: &TextMatch,
    project_root: &std::path::Path,
) -> serde_json::Value {
    let file_path = relative_path(&m.file.path(db), project_root);
    let annotation = match &m.annotation {
        Some(MatchAnnotation::Definition { kind }) => {
            serde_json::json!({ "type": "definition", "kind": kind.as_str() })
        }
        Some(MatchAnnotation::Reference {
            target_name,
            target_kind,
        }) => {
            serde_json::json!({
                "type": "reference",
                "target_name": target_name,
                "target_kind": target_kind.as_str(),
            })
        }
        None => serde_json::Value::Null,
    };
    serde_json::json!({
        "file": file_path.to_string_lossy(),
        "line": m.line_number,
        "text": m.line_text.trim(),
        "annotation": annotation,
    })
}

/// Build a budget-aware JSON value for a `SymbolDescription`.
pub fn description_to_json(
    db: &ProjectDatabase,
    desc: &SymbolDescription,
    budget: usize,
    project_root: &std::path::Path,
) -> serde_json::Value {
    let file_path = relative_path(&desc.file.path(db), project_root);
    let body = budget_body(desc, budget);
    serde_json::json!({
        "name": desc.name,
        "kind": desc.kind.as_str(),
        "file": file_path.to_string_lossy(),
        "line": line_number_at_offset(desc.file.text(db), desc.name_span.start().into()),
        "shape": desc.shape,
        "body": body,
        "docstring": desc.docstring,
        "resolved_type": desc.resolved_type,
        "dependencies": desc.dependencies.iter().map(|dep| {
            let dep_path = relative_path(&dep.file.path(db), project_root);
            serde_json::json!({
                "name": dep.name,
                "kind": dep.kind.as_str(),
                "file": dep_path.to_string_lossy(),
                "line": line_number_at_offset(dep.file.text(db), dep.name_span.start().into()),
            })
        }).collect::<Vec<_>>(),
        "references": desc.references.iter().map(|r| {
            let ref_path = relative_path(&r.file.path(db), project_root);
            serde_json::json!({
                "file": ref_path.to_string_lossy(),
                "line": r.line_number,
                "text": r.line_text.trim(),
            })
        }).collect::<Vec<_>>(),
        "instance_methods": desc.instance_methods.iter().map(|m| {
            let m_path = relative_path(&m.file.path(db), project_root);
            serde_json::json!({
                "name": m.name,
                "kind": m.kind.as_str(),
                "file": m_path.to_string_lossy(),
                "line": line_number_at_offset(m.file.text(db), m.name_span.start().into()),
            })
        }).collect::<Vec<_>>(),
        "static_methods": desc.static_methods.iter().map(|m| {
            let m_path = relative_path(&m.file.path(db), project_root);
            serde_json::json!({
                "name": m.name,
                "kind": m.kind.as_str(),
                "file": m_path.to_string_lossy(),
                "line": line_number_at_offset(m.file.text(db), m.name_span.start().into()),
            })
        }).collect::<Vec<_>>(),
        "container": desc.container.as_ref().map(|c| {
            let c_path = relative_path(&c.file.path(db), project_root);
            serde_json::json!({
                "name": c.name,
                "kind": c.kind.as_str(),
                "file": c_path.to_string_lossy(),
                "line": line_number_at_offset(c.file.text(db), c.name_span.start().into()),
            })
        }),
    })
}
