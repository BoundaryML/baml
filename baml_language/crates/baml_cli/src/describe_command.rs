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
            let kind_filter = crate::grep_command::parse_kind_filter(&self.kind);
            let symbols = baml_lsp2_actions::list_symbols(&db, &source_files, &kind_filter);
            if symbols.is_empty() {
                eprintln!("No symbols found.");
                return Ok(crate::ExitCode::Other);
            }
            for sym in &symbols {
                let rel = relative_path(&sym.file.path(&db), &from);
                let line = line_number_at_offset(
                    sym.file.text(&db),
                    sym.name_span.start().into(),
                );
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
        baml_lsp2_actions::DefinitionKind::Parameter
            | baml_lsp2_actions::DefinitionKind::Binding
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
            let ty = desc
                .resolved_type
                .as_deref()
                .unwrap_or("unknown");
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
        println!(
            "── {kind_str} ──────────────────────────────── {path_display}:{line_num}"
        );
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
    // Skip body for params/bindings — everything is in the header.
    let shape_lines: Vec<&str> = desc.shape.lines().collect();
    let body_lines: Vec<&str> = desc.full_body.lines().collect();

    if !is_local {
        if lines_used + body_lines.len() <= budget {
            for line in &body_lines {
                println!("{line}");
            }
            lines_used += body_lines.len();
        } else {
            for line in &shape_lines {
                println!("{line}");
            }
            lines_used += shape_lines.len();
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
        println!("── references ({}) ─────────────────────────", desc.references.len());
        lines_used += 2;

        let mut refs_printed = 0;
        for r in &desc.references {
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
            refs_printed += 1;
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
                let dep_line = line_number_at_offset(dep.file.text(db), dep.name_span.start().into());
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
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_path_buf()
}
