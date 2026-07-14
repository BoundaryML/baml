#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{Context, Result, anyhow};
use baml_db::{
    FileId, Span,
    baml_compiler_diagnostics::{Diagnostic, DiagnosticId, DiagnosticPhase, Severity, render},
};
use clap::Args;
use sdkgen_python_pydantic2::{NamingConvention, OutputType};
use text_size::{TextRange, TextSize};
use toml::Spanned;

use crate::{commands::release_version, project_load::load_project_for_build, reporter::Reporter};

#[derive(Args, Clone, Debug)]
pub struct GenerateArgs {
    /// Project search starting point. Defaults to the current directory.
    #[arg(long, value_name = "PATH")]
    pub from: Option<PathBuf>,

    /// Output directory override (takes precedence over generator config)
    #[arg(long, short = 'o')]
    pub output: Option<PathBuf>,
}

/// A validated generator, resolved from a `[generator.<name>]` section of
/// `baml.toml`.
struct GeneratorDef {
    name: String,
    output_type: OutputType,
    /// Resolved output directory (absolute).
    output_dir: PathBuf,
    /// Required `naming_convention` from the generator section. No default
    /// is permitted — generators must spell out the policy explicitly.
    naming_convention: NamingConvention,
}

impl GenerateArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let reporter = Reporter::new();
        reporter.status(
            "Generating",
            format!("clients with CLI version: {}", release_version()),
        );
        let (db, from, baml_files) =
            load_project_for_build(self.from.as_deref(), &reporter, false)?;
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
        // source-snippet output doesn't fight with the lamb. No "Checking"
        // line here: the meaningful "Resolving" and "Compiling" phases below
        // carry the progress, and a "Checking N file(s)" would just duplicate
        // the "Compiling N file(s)" count.
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

        // Discover generator definitions from `baml.toml`'s
        // `[generator.<name>]` sections and validate per-target rules (e.g.
        // python requires `naming_convention`). Validation runs here — at the
        // CLI layer, not in the compiler — so the manifest never has to flow
        // into salsa and non-codegen tooling stays codegen-agnostic.
        reporter.spin("Resolving", "[generator] sections in baml.toml");
        let (generators, gen_diags) = discover_generators(&from);
        if !gen_diags.is_empty() {
            // Manifest diagnostics carry spans into `baml.toml`, which isn't a
            // salsa source file — register it under a dedicated pseudo
            // [`FileId`] so ariadne can render the offending snippet.
            let mut sources = HashMap::new();
            let mut file_paths = HashMap::new();
            for sf in &source_files {
                let file_id = sf.file_id(&db);
                sources.insert(file_id, sf.text(&db).to_string());
                file_paths.insert(file_id, sf.path(&db));
            }
            let toml_path = from.join("baml.toml");
            if let Ok(content) = std::fs::read_to_string(&toml_path) {
                sources.insert(manifest_file_id(), content);
                file_paths.insert(manifest_file_id(), toml_path);
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
            crate::reporter::print_error("no `[generator.<name>]` sections found in baml.toml");
            #[allow(clippy::print_stderr)]
            {
                eprintln!();
                eprintln!("Add a generator section to your baml.toml, e.g.:");
                eprintln!();
                eprintln!("  [generator.my_client]");
                eprintln!("  output_type = \"python/pydantic\"");
                eprintln!("  output_dir = \"../python\"");
                eprintln!("  naming_convention = \"preserve-case\"");
            }
            return Ok(crate::ExitCode::Other);
        }

        // Build the codegen SymbolPool from the compiler database.
        let pool = baml_project::build_symbol_pool(&db);

        reporter.spin("Compiling", format!("{} file(s)", source_files.len()));
        let program = db
            .get_bytecode()
            .map_err(|e| anyhow!("Compilation failed: {e:?}"))?;
        let baml_bytecode = borsh::to_vec(&program)
            .map_err(|e| anyhow!("Failed to serialize BAML bytecode: {e}"))?;

        let mut total_files = 0;

        for generator in &generators {
            reporter.spin("Generating", &generator.name);
            let output_dir = self
                .output
                .clone()
                .unwrap_or_else(|| generator.output_dir.clone());

            // Unified to bytes at the write boundary: python/TS emit text
            // only; the rust generator also ships the embedded bytecode as
            // a binary file.
            let generated: Vec<(PathBuf, Vec<u8>)> = match generator.output_type {
                OutputType::PythonPydantic | OutputType::PythonPydanticV1 => {
                    sdkgen_python_pydantic2::to_source_code_with_bytecode(
                        &pool,
                        &baml_bytecode,
                        generator.naming_convention,
                    )
                    .into_iter()
                    .map(|(path, content)| (path, content.into_bytes()))
                    .collect()
                }
                OutputType::TypescriptNode => sdkgen_typescript_node::to_source_code_with_bytecode(
                    &pool,
                    &baml_bytecode,
                    generator.naming_convention,
                )
                .into_iter()
                .map(|(path, content)| (path, content.into_bytes()))
                .collect(),
                OutputType::Rust => {
                    let generated = sdkgen_rust::to_source_code_with_bytecode(
                        &pool,
                        &baml_bytecode,
                        &sdkgen_rust::RustGenOptions {
                            naming_convention: generator.naming_convention,
                            package_name: "baml_sdk".to_string(),
                            // The runtime crate is not published yet; pin the
                            // matching version for when it is.
                            runtime_dep: format!("\"={}\"", baml_version::CANONICAL_VERSION),
                            manifest_extra: None,
                            edition: "2024".to_string(),
                        },
                    );
                    for warning in &generated.warnings {
                        reporter.warning(format!("skipped `{}`: {}", warning.fqn, warning.reason));
                    }
                    generated
                        .files
                        .into_iter()
                        .map(|(path, content)| (path, content.into_bytes()))
                        .collect()
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

/// Pseudo [`FileId`] for `baml.toml`. The manifest isn't a salsa source
/// file, but generator diagnostics still need to point into it, so we mint a
/// dedicated id at the top of the 28-bit range — far above any real
/// sequentially-assigned source-file id, so a collision is impossible.
fn manifest_file_id() -> FileId {
    FileId::new(0x0FFF_FFFF)
}

/// Read `baml.toml`'s `[generator.<name>]` sections and run per-target
/// validation (e.g. Python requires `naming_convention`). Returns the
/// validated `GeneratorDef`s plus any diagnostics collected during
/// validation (spans point into `baml.toml` via [`manifest_file_id()`]).
///
/// A manifest-less project (`baml_src/` only) has no `baml.toml`, hence no
/// generators — the caller surfaces that as the "no `[generator]` sections"
/// hint. A malformed manifest is impossible to reach here: the strict
/// project loader parses and validates `baml.toml` before we ever get this
/// far, so a parse error returns empty rather than double-reporting.
fn discover_generators(root: &Path) -> (Vec<GeneratorDef>, Vec<Diagnostic>) {
    let mut generators = Vec::new();
    let mut diags = Vec::new();

    let Ok(content) = std::fs::read_to_string(root.join("baml.toml")) else {
        return (generators, diags);
    };
    let Ok(manifest) = crate::manifest::parse(&content) else {
        return (generators, diags);
    };

    for (name, spanned) in &manifest.generator {
        let table_range = to_text_range(spanned.span());
        let generator = spanned.get_ref();

        // Run both validators unconditionally so a section missing multiple
        // required properties surfaces all of its issues at once.
        let output_type = parse_required_property::<OutputType>(
            name,
            "output_type",
            generator.output_type.as_ref(),
            r#"one of: "python/pydantic", "python/pydantic/v1", "typescript/node", "rust""#,
            table_range,
            &mut diags,
        );
        let naming_convention = parse_required_property::<NamingConvention>(
            name,
            "naming_convention",
            generator.naming_convention.as_ref(),
            r#""preserve-case" or "language""#,
            table_range,
            &mut diags,
        );

        // `output_dir` is resolved relative to the project root and defaults
        // to "..", with `baml_sdk` appended (matching the historic
        // `generator {}` behavior).
        let raw_output_dir = generator.output_dir.as_deref().unwrap_or("..");
        let output_dir = root.join(raw_output_dir).join("baml_sdk");

        // Skip codegen for sections that failed validation; their
        // diagnostics block the run upstream.
        let (Some(output_type), Some(naming_convention)) = (output_type, naming_convention) else {
            continue;
        };

        generators.push(GeneratorDef {
            name: name.clone(),
            output_type,
            output_dir,
            naming_convention,
        });
    }

    (generators, diags)
}

/// Parse a required `[generator.<name>]` property as `T` via strum. Pushes a
/// `MissingGeneratorProperty` diagnostic if absent and an
/// `InvalidGeneratorPropertyValue` diagnostic if present-but-unparseable;
/// returns `None` in either case so the caller can keep going (and surface
/// any other issues on the same section in one pass). `table_range` locates
/// the section header for the "missing" case; the value's own span locates
/// the "invalid" case.
fn parse_required_property<T: FromStr>(
    generator_name: &str,
    property: &str,
    value: Option<&Spanned<String>>,
    expected: &str,
    table_range: TextRange,
    diags: &mut Vec<Diagnostic>,
) -> Option<T> {
    let Some(value) = value else {
        diags.push(
            Diagnostic::error(
                DiagnosticId::MissingGeneratorProperty,
                format!(
                    "generator `{generator_name}` is missing required property \
                     `{property}` (expected {expected})"
                ),
            )
            .with_primary(
                Span {
                    file_id: manifest_file_id(),
                    range: table_range,
                },
                "missing required property",
            )
            .with_phase(DiagnosticPhase::Validation),
        );
        return None;
    };

    match value.get_ref().parse::<T>() {
        Ok(parsed) => Some(parsed),
        Err(_) => {
            diags.push(
                Diagnostic::error(
                    DiagnosticId::InvalidGeneratorPropertyValue,
                    format!(
                        "invalid value `{}` for `{property}` on generator \
                         `{generator_name}` (expected {expected})",
                        value.get_ref()
                    ),
                )
                .with_primary(
                    Span {
                        file_id: manifest_file_id(),
                        range: to_text_range(value.span()),
                    },
                    "invalid value",
                )
                .with_phase(DiagnosticPhase::Validation),
            );
            None
        }
    }
}

/// Convert a `toml::Spanned` byte range into the `TextRange` our diagnostics
/// use.
fn to_text_range(span: std::ops::Range<usize>) -> TextRange {
    TextRange::new(
        TextSize::new(span.start as u32),
        TextSize::new(span.end as u32),
    )
}
