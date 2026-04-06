#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;

use anyhow::{Context, Result};
use baml_lsp2_actions::{
    DefinitionKind, GrepMode, GrepOptions, MatchAnnotation, SymbolDescription,
    describe, grep, list_symbols,
};
use baml_project::ProjectDatabase;
use baml_workspace::discover_baml_files;
use clap::Args;

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
        let kind_filter = parse_kind_filter(&self.kind);

        // ── --symbols mode ──────────────────────────────────────────────────
        if self.symbols {
            let symbols = list_symbols(&db, &source_files, &kind_filter);
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

        match result.mode {
            GrepMode::Semantic => {
                let history: std::collections::HashSet<&str> =
                    self.history.iter().map(|s| s.as_str()).collect();
                for (i, desc) in result.descriptions.iter().enumerate() {
                    if i > 0 {
                        println!();
                        println!();
                    }
                    render_description(
                        &db, desc, self.budget, &history, self.no_hints, &from,
                    );
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
    history: &std::collections::HashSet<&str>,
    no_hints: bool,
    project_root: &std::path::Path,
) {
    // Delegate to the describe_command renderer.
    crate::describe_command::render_description(db, desc, budget, history, no_hints, project_root);
}

/// Render only the references section for a symbol.
fn render_refs_only(
    db: &ProjectDatabase,
    desc: &SymbolDescription,
    project_root: &std::path::Path,
) {
    let file_path = relative_path(&desc.file.path(db), project_root);
    let line_num = line_number_at_offset(
        desc.file.text(db),
        desc.name_span.start().into(),
    );

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
                format!("  ← reference to {} ({})", target_name, target_kind.as_str())
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
pub fn parse_kind_filter(kinds: &[String]) -> Vec<DefinitionKind> {
    kinds
        .iter()
        .filter_map(|s| match s.as_str() {
            "class" => Some(DefinitionKind::Class),
            "enum" => Some(DefinitionKind::Enum),
            "function" => Some(DefinitionKind::Function),
            "test" => Some(DefinitionKind::Test),
            "client" => Some(DefinitionKind::Client),
            "type_alias" => Some(DefinitionKind::TypeAlias),
            "template_string" => Some(DefinitionKind::TemplateString),
            "retry_policy" => Some(DefinitionKind::RetryPolicy),
            "generator" => Some(DefinitionKind::Generator),
            "let" => Some(DefinitionKind::Let),
            "field" => Some(DefinitionKind::Field),
            "variant" => Some(DefinitionKind::Variant),
            other => {
                eprintln!("Unknown kind: {other}");
                None
            }
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
