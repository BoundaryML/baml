#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use baml_db::{
    Span,
    baml_compiler_diagnostics::{Diagnostic, DiagnosticId, DiagnosticPhase, Severity, render},
    baml_compiler2_hir::{
        self, file_package,
        ids::{GeneratorMarker, LocalItemId},
        item_tree::{Generator, GeneratorConfigItem, ItemTreeSourceMap},
    },
};
use baml_project::ProjectDatabase;
use clap::Args;
use codegen_python::{NamingConvention, OutputType};

use crate::{project_load::load_project_from_reporting, reporter::Reporter};

#[derive(Args, Clone, Debug)]
pub struct GenerateArgs {
    /// Path to the baml_src directory
    #[arg(long, default_value = ".")]
    pub from: PathBuf,

    /// Output directory override (takes precedence over generator config)
    #[arg(long, short = 'o')]
    pub output: Option<PathBuf>,
}

/// A parsed generator definition from a BAML source file.
struct GeneratorDef {
    name: String,
    output_type: OutputType,
    /// Resolved output directory (absolute).
    output_dir: PathBuf,
    /// Required `naming_convention` from the generator block. No default
    /// is permitted — generators must spell out the policy explicitly.
    naming_convention: NamingConvention,
}

impl GenerateArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let reporter = Reporter::new();
        let (db, from, baml_files) = load_project_from_reporting(&self.from, &reporter)?;
        if baml_files.is_empty() {
            reporter.abandon();
            crate::reporter::print_error(format_args!(
                "no .baml files found in {}",
                from.display()
            ));
            return Ok(crate::ExitCode::Other);
        }
        let project = db
            .get_project()
            .ok_or_else(|| anyhow!("No project context"))?;

        // Compile-time diagnostics — same shape as run/pack: render the
        // ariadne block after abandoning the spinner so the colored
        // source-snippet output doesn't fight with the lamb.
        reporter.spin("Checking", format!("{} file(s)", baml_files.len()));
        let source_files = db.get_source_files();
        let diagnostics = baml_project::collect_diagnostics(&db, project, &source_files);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        if !errors.is_empty() {
            let mut sources = HashMap::new();
            let mut file_paths = HashMap::new();
            for sf in &source_files {
                let file_id = sf.file_id(&db);
                sources.insert(file_id, sf.text(&db).to_string());
                file_paths.insert(file_id, sf.path(&db));
            }
            let rendered = render::render_diagnostics(
                &errors.iter().copied().cloned().collect::<Vec<_>>(),
                &sources,
                &file_paths,
                &render::RenderConfig::cli_auto(),
            );
            reporter.abandon();
            eprintln!("{rendered}");
            return Ok(crate::ExitCode::Other);
        }

        // Discover generator definitions and validate per-target rules
        // (e.g. python requires `naming_convention`). Validation runs here
        // — not during HIR lowering — so non-codegen tooling (LSP, formatter)
        // doesn't have to care about codegen-specific generator rules.
        reporter.spin("Resolving", "generator blocks");
        let (generators, gen_diags) = discover_generators(&db, &from);
        if !gen_diags.is_empty() {
            let mut sources = HashMap::new();
            let mut file_paths = HashMap::new();
            for sf in &source_files {
                let file_id = sf.file_id(&db);
                sources.insert(file_id, sf.text(&db).to_string());
                file_paths.insert(file_id, sf.path(&db));
            }
            let rendered = render::render_diagnostics(
                &gen_diags,
                &sources,
                &file_paths,
                &render::RenderConfig::cli_auto(),
            );
            reporter.abandon();
            eprintln!("{rendered}");
            return Ok(crate::ExitCode::Other);
        }

        if generators.is_empty() {
            reporter.abandon();
            crate::reporter::print_error("no generator blocks found in BAML sources");
            #[allow(clippy::print_stderr)]
            {
                eprintln!();
                eprintln!("Add a generator block to your .baml files, e.g.:");
                eprintln!();
                eprintln!("  generator my_client {{");
                eprintln!("    output_type python/pydantic");
                eprintln!("    output_dir \"..\"");
                eprintln!("  }}");
            }
            return Ok(crate::ExitCode::Other);
        }

        // Build the codegen SymbolPool from the compiler database.
        let pool = baml_project::build_symbol_pool(&db);

        // Collect user BAML source files keyed by path relative to
        // `baml_src/` for inlining into `baml_sdk/baml/_inlinedbaml.py`.
        let user_baml_files = collect_user_baml_files(&db, &source_files, &from);

        let mut total_files = 0;

        for generator in &generators {
            reporter.spin("Generating", &generator.name);
            let output_dir = self
                .output
                .clone()
                .unwrap_or_else(|| generator.output_dir.clone());

            let generated = match generator.output_type {
                OutputType::PythonPydantic | OutputType::PythonPydanticV1 => {
                    codegen_python::to_source_code(
                        &pool,
                        &user_baml_files,
                        generator.naming_convention,
                    )
                }
            };

            std::fs::create_dir_all(&output_dir).with_context(|| {
                format!(
                    "Failed to create output directory: {}",
                    output_dir.display()
                )
            })?;
            let output_dir = std::fs::canonicalize(&output_dir).unwrap_or(output_dir);

            let mut count = 0;
            for (rel_path, content) in &generated {
                let dest = output_dir.join(rel_path);
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&dest, content)
                    .with_context(|| format!("Failed to write {}", dest.display()))?;
                count += 1;
            }

            // Persistent status line in the scrollback — one per
            // generator block. Matches cargo's `   Compiling foo
            // v0.1.0` pattern: per-unit progress that sticks around
            // above the spinner.
            reporter.status(
                "Generated",
                format!(
                    "{} ({count} file(s) → {})",
                    generator.name,
                    output_dir.display()
                ),
            );
            total_files += count;
        }

        if total_files == 0 {
            reporter.abandon();
            crate::reporter::print_error("no files generated (no supported generators found)");
            return Ok(crate::ExitCode::Other);
        }

        reporter.finish("Finished", format!("generated {total_files} file(s)"));
        Ok(crate::ExitCode::Success)
    }
}

/// Walk the HIR item trees to discover all `generator` blocks and their
/// config, and run per-target validation (e.g. Python requires
/// `naming_convention`). Returns the validated `GeneratorDef`s plus any
/// diagnostics collected during validation.
fn discover_generators(
    db: &ProjectDatabase,
    baml_src: &std::path::Path,
) -> (Vec<GeneratorDef>, Vec<Diagnostic>) {
    let mut generators = Vec::new();
    let mut diags = Vec::new();

    for source_file in db.get_source_files() {
        let pkg_info = file_package::file_package(db, source_file);
        if pkg_info.package.as_str() != "user" {
            continue;
        }

        let item_tree = baml_compiler2_hir::file_item_tree(db, source_file);
        let source_map = baml_compiler2_hir::file_item_tree_source_map(db, source_file);
        let file_id = source_file.file_id(db);

        for (id, generator_item) in &item_tree.generators {
            // Run both validators unconditionally so that a block missing
            // multiple required properties surfaces all of its issues at once.
            let output_type = parse_required_property::<OutputType>(
                *id,
                generator_item,
                "output_type",
                r#"one of: "python/pydantic", "python/pydantic/v1""#,
                &source_map,
                file_id,
                &mut diags,
            );
            let naming_convention = parse_required_property::<NamingConvention>(
                *id,
                generator_item,
                "naming_convention",
                r#""preserve-case" or "language""#,
                &source_map,
                file_id,
                &mut diags,
            );

            // output_dir is relative to baml_src, defaults to "../"
            let raw_output_dir = get_config(&generator_item.config_items, "output_dir")
                .unwrap_or_else(|| "..".to_string());
            // Strip surrounding quotes if present (config values may be quoted strings)
            let raw_output_dir = raw_output_dir.trim_matches('"').trim_matches('\'');

            let output_dir = baml_src.join(raw_output_dir).join("baml_sdk");

            // Skip codegen for generators that failed validation; their
            // diagnostics will block the run upstream.
            let (Some(output_type), Some(naming_convention)) = (output_type, naming_convention)
            else {
                continue;
            };

            generators.push(GeneratorDef {
                name: generator_item.name.to_string(),
                output_type,
                output_dir,
                naming_convention,
            });
        }
    }

    (generators, diags)
}

/// Look up `property` on a generator block and parse it as `T` via strum.
/// Pushes a `MissingGeneratorProperty` diagnostic if absent and an
/// `InvalidGeneratorPropertyValue` diagnostic if present-but-unparseable;
/// returns `None` in either case so the caller can keep going (and surface
/// any other issues on the same block in one pass).
fn parse_required_property<T: std::str::FromStr>(
    id: LocalItemId<GeneratorMarker>,
    generator: &Generator,
    property: &str,
    expected: &str,
    source_map: &ItemTreeSourceMap,
    file_id: baml_db::FileId,
    diags: &mut Vec<Diagnostic>,
) -> Option<T> {
    let block_range = source_map
        .generator_block_spans
        .get(&id)
        .copied()
        .unwrap_or_default();

    let Some(item_idx) = generator
        .config_items
        .iter()
        .position(|c| c.key.as_str() == property)
    else {
        diags.push(
            Diagnostic::error(
                DiagnosticId::MissingGeneratorProperty,
                format!(
                    "generator `{}` is missing required property `{property}` \
                     (expected {expected})",
                    generator.name
                ),
            )
            .with_primary(
                Span {
                    file_id,
                    range: block_range,
                },
                "missing required property",
            )
            .with_phase(DiagnosticPhase::Validation),
        );
        return None;
    };

    let value = &generator.config_items[item_idx].value;
    match value.parse::<T>() {
        Ok(parsed) => Some(parsed),
        Err(_) => {
            let item_range = source_map
                .generator_config_item_spans
                .get(&id)
                .and_then(|spans| spans.get(item_idx).copied())
                .unwrap_or(block_range);
            diags.push(
                Diagnostic::error(
                    DiagnosticId::InvalidGeneratorPropertyValue,
                    format!(
                        "invalid value `{value}` for `{property}` on generator \
                         `{}` (expected {expected})",
                        generator.name
                    ),
                )
                .with_primary(
                    Span {
                        file_id,
                        range: item_range,
                    },
                    "invalid value",
                )
                .with_phase(DiagnosticPhase::Validation),
            );
            None
        }
    }
}

/// Look up a config key in a generator's config items.
fn get_config(items: &[GeneratorConfigItem], key: &str) -> Option<String> {
    items
        .iter()
        .find(|item| item.key.as_str() == key)
        .map(|item| item.value.clone())
}

/// Collect user BAML source files as `(rel_path, contents)` pairs.
/// `rel_path` is relative to `baml_src/` so it matches the keys the
/// runtime's `initialize_runtime(...)` expects in the inlined `FILES`
/// dict.
fn collect_user_baml_files(
    db: &ProjectDatabase,
    source_files: &[baml_db::SourceFile],
    baml_src: &Path,
) -> Vec<(PathBuf, String)> {
    source_files
        .iter()
        .map(|sf| {
            let path = sf.path(db);
            let rel = path.strip_prefix(baml_src).unwrap_or(&path).to_path_buf();
            (rel, sf.text(db).to_string())
        })
        .collect()
}
