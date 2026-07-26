#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{Context, Result, anyhow};
use baml_codegen_types::{GeneratedOutputFile, write_generated_output};
use baml_db::{
    FileId, Span,
    baml_compiler_diagnostics::{Diagnostic, DiagnosticId, DiagnosticPhase, Severity, render},
};
use clap::Args;
use sdkgen_python_pydantic2::{NamingConvention, OutputType};
use text_size::{TextRange, TextSize};
use toml::Spanned;

use crate::{commands::release_version, reporter::Reporter};

#[derive(Args, Clone, Debug)]
pub struct GenerateArgs {
    /// Project or source directory. An explicit directory outside a discovered
    /// project's `baml_src/` is loaded directly. Defaults to the current directory.
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
    /// Required for Go so generated packages can import the SDK root and one
    /// another. Other generators leave this unset.
    sdk_import_path: Option<String>,
    max_typed_union_arity: usize,
}

impl GenerateArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let reporter = Reporter::new();
        reporter.status(
            "Generating",
            format!("clients with CLI version: {}", release_version()),
        );
        // Codegen reads types across the whole project, so take the shared
        // read-only session: warm seeds where they are provably faithful and
        // the parallel index prime, same as describe/grep.
        let mut session = crate::project_session::ProjectSession::open(
            self.from.as_deref(),
            crate::project_session::CacheUse::ReadOnly,
        )?;
        if session.is_empty() {
            reporter.abandon();
            crate::reporter::print_error(format_args!(
                "no .baml files found in {}",
                session.root().display()
            ));
            return Ok(crate::ExitCode::Other);
        }
        let _ = session.warm_prep_seeds_only();
        session.prime();
        let (db, from) = (session.db, session.resolved.root);
        // Compile-time diagnostics — same shape as run/pack: render the
        // diagnostic block after abandoning the spinner so the colored
        // source-snippet output doesn't fight with the lamb. No "Checking"
        // line here: the meaningful "Resolving" and "Compiling" phases below
        // carry the progress, and a "Checking N file(s)" would just duplicate
        // the "Compiling N file(s)" count.
        let source_files = db.get_source_files();
        let diagnostics = baml_project::collect_diagnostics(&db);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        if !errors.is_empty() {
            let rendered = crate::check_command::render_project_diagnostics(
                &db,
                &errors.iter().copied().cloned().collect::<Vec<_>>(),
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
            // [`FileId`] so the diagnostic renderer can show the offending snippet.
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
                &crate::output::policy().diagnostic_render_config(),
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
                eprintln!("add a generator section to `baml.toml`, for example:");
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
            .map_err(|e| anyhow!("compilation failed: {e:?}"))?;
        let baml_bytecode = borsh::to_vec(&program)
            .map_err(|e| anyhow!("failed to serialize BAML bytecode: {e}"))?;

        let mut total_files = 0;

        for generator in &generators {
            reporter.spin("Generating", &generator.name);
            let requested_output = self
                .output
                .clone()
                .unwrap_or_else(|| generator.output_dir.clone());
            let output_dir = if requested_output.is_absolute() {
                requested_output
            } else {
                std::env::current_dir()
                    .context("failed to resolve the current directory for generated output")?
                    .join(requested_output)
            };

            if generator.output_type == OutputType::CSharp {
                let report = sdkgen_csharp::generate_into(sdkgen_csharp::CSharpGenerateRequest {
                    symbols: &pool,
                    program_bytes: &baml_bytecode,
                    cli_version: release_version(),
                    required_bridge_version: baml_version::CANONICAL_VERSION,
                    program_identity: &generator.name,
                    output_directory: output_dir.clone(),
                })?;
                let count = report.written_files.len();
                reporter.status(
                    "Generated",
                    format!(
                        "{} ({count} file(s) → {})",
                        generator.name,
                        output_dir.display()
                    ),
                );
                total_files += count;
                continue;
            }

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
                OutputType::TypescriptNode => {
                    sdkgen_typescript_shared::sdkgen_typescript::to_source_code_with_bytecode(
                        &pool,
                        &baml_bytecode,
                        generator.naming_convention,
                    )
                    .into_iter()
                    .map(|(path, content)| (path, content.into_bytes()))
                    .collect()
                }
                OutputType::TypescriptWeb => {
                    sdkgen_typescript_shared::sdkgen_typescript_web::to_source_code_with_bytecode(
                        &pool,
                        &baml_bytecode,
                        generator.naming_convention,
                    )
                    .into_iter()
                    .map(|(path, content)| (path, content.into_bytes()))
                    .collect()
                }
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
                OutputType::Go => sdkgen_go::try_to_source_code_with_bytecode_and_options(
                    &pool,
                    &baml_bytecode,
                    &sdkgen_go::GoGenOptions {
                        naming_convention: generator.naming_convention,
                        sdk_import_path: generator
                            .sdk_import_path
                            .as_deref()
                            .expect("validated Go generator must have sdk_import_path"),
                        max_typed_union_arity: generator.max_typed_union_arity,
                    },
                )
                .map_err(|error| anyhow!("failed to generate Go SDK: {error}"))?
                .into_iter()
                .map(|(path, content)| (path, content.into_bytes()))
                .collect(),
                OutputType::Cpp => {
                    // The C++ emitter embeds source paths (reference
                    // comments only); the runtime payload is the bytecode.
                    let source_paths: Vec<PathBuf> = source_files
                        .iter()
                        .map(|sf| {
                            let path = sf.path(&db);
                            path.strip_prefix(&from).unwrap_or(&path).to_path_buf()
                        })
                        .collect();
                    sdkgen_cpp::to_source_code_with_bytecode(&pool, &source_paths, &baml_bytecode)
                        .into_iter()
                        .map(|(path, content)| (path, content.into_bytes()))
                        .collect()
                }
                OutputType::Java => sdkgen_java::to_source_code_with_bytecode(
                    &pool,
                    &baml_bytecode,
                    generator.naming_convention,
                )
                .into_iter()
                .map(|(path, content)| (path, content.into_bytes()))
                .collect(),
                OutputType::Swift => sdkgen_swift::to_source_code_with_bytecode(
                    &pool,
                    &baml_bytecode,
                    generator.naming_convention,
                )
                .into_iter()
                .map(|(path, content)| (path, content.into_bytes()))
                .collect(),
                OutputType::CSharp => unreachable!("C# generation commits atomically above"),
            };

            let output = generated
                .into_iter()
                .map(|(path, contents)| GeneratedOutputFile::new(path, contents))
                .collect();
            let report = write_generated_output(&output_dir, output).with_context(|| {
                format!(
                    "failed to install generated output in {}",
                    output_dir.display()
                )
            })?;
            let count = report.written_files.len();

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
            r#"one of: "python/pydantic", "python/pydantic/v1", "typescript/node", "typescript/web", "swift", "go", "rust", "java", "cpp", "csharp""#,
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
        let sdk_import_path = if matches!(output_type, Some(OutputType::Go)) {
            parse_required_go_import_path(
                name,
                generator.sdk_import_path.as_ref(),
                table_range,
                &mut diags,
            )
        } else {
            None
        };
        let max_typed_union_arity = if matches!(output_type, Some(OutputType::Go)) {
            match generator.max_typed_union_arity.as_ref() {
                None => sdkgen_go::DEFAULT_MAX_TYPED_UNION_ARITY,
                Some(value) if *value.get_ref() >= 0 => *value.get_ref() as usize,
                Some(value) => {
                    diags.push(
                        Diagnostic::error(
                            DiagnosticId::InvalidGeneratorPropertyValue,
                            format!(
                                "Go generator `{name}` requires `max_typed_union_arity` to be zero or greater"
                            ),
                        )
                        .with_primary(
                            Span {
                                file_id: manifest_file_id(),
                                range: to_text_range(value.span()),
                            },
                            "negative union threshold",
                        )
                        .with_phase(DiagnosticPhase::Validation),
                    );
                    sdkgen_go::DEFAULT_MAX_TYPED_UNION_ARITY
                }
            }
        } else {
            if let Some(value) = generator.max_typed_union_arity.as_ref() {
                diags.push(
                    Diagnostic::error(
                        DiagnosticId::InvalidGeneratorPropertyValue,
                        format!(
                            "generator `{name}` sets Go-only property `max_typed_union_arity` on a non-Go target"
                        ),
                    )
                    .with_primary(
                        Span {
                            file_id: manifest_file_id(),
                            range: to_text_range(value.span()),
                        },
                        "remove this Go-only property",
                    )
                    .with_phase(DiagnosticPhase::Validation),
                );
            }
            sdkgen_go::DEFAULT_MAX_TYPED_UNION_ARITY
        };

        // `output_dir` is resolved relative to the project root and defaults
        // to "..", with the target-owned generated directory appended.
        let raw_output_dir = generator.output_dir.as_deref().unwrap_or("..");
        let generated_directory = output_type.map_or("baml_sdk", OutputType::generated_directory);
        let output_dir = root.join(raw_output_dir).join(generated_directory);

        // Skip codegen for sections that failed validation; their
        // diagnostics block the run upstream.
        let (Some(output_type), Some(naming_convention)) = (output_type, naming_convention) else {
            continue;
        };
        if output_type == OutputType::Go {
            if naming_convention != NamingConvention::Language {
                let range = generator
                    .naming_convention
                    .as_ref()
                    .map(|value| to_text_range(value.span()))
                    .unwrap_or(table_range);
                diags.push(
                    Diagnostic::error(
                        DiagnosticId::InvalidGeneratorPropertyValue,
                        format!(
                            "Go generator `{name}` requires `naming_convention = \"language\"`"
                        ),
                    )
                    .with_primary(
                        Span {
                            file_id: manifest_file_id(),
                            range,
                        },
                        "Go identifiers use the canonical language projection",
                    )
                    .with_phase(DiagnosticPhase::Validation),
                );
                continue;
            }
            if sdk_import_path.is_none() {
                continue;
            }
        }

        generators.push(GeneratorDef {
            name: name.clone(),
            output_type,
            output_dir,
            naming_convention,
            sdk_import_path,
            max_typed_union_arity,
        });
    }

    (generators, diags)
}

fn parse_required_go_import_path(
    generator_name: &str,
    value: Option<&Spanned<String>>,
    table_range: TextRange,
    diags: &mut Vec<Diagnostic>,
) -> Option<String> {
    let Some(value) = value else {
        diags.push(
            Diagnostic::error(
                DiagnosticId::MissingGeneratorProperty,
                format!(
                    "Go generator `{generator_name}` is missing required property \
                     `sdk_import_path` (for example `example.com/project/baml_sdk`)"
                ),
            )
            .with_primary(
                Span {
                    file_id: manifest_file_id(),
                    range: table_range,
                },
                "missing Go SDK import path",
            )
            .with_phase(DiagnosticPhase::Validation),
        );
        return None;
    };

    let import_path = value.get_ref();
    let valid = is_valid_go_import_path(import_path);
    if valid {
        return Some(import_path.clone());
    }

    diags.push(
        Diagnostic::error(
            DiagnosticId::InvalidGeneratorPropertyValue,
            format!("invalid `sdk_import_path` `{import_path}` on Go generator `{generator_name}`"),
        )
        .with_primary(
            Span {
                file_id: manifest_file_id(),
                range: to_text_range(value.span()),
            },
            "expected slash-delimited non-empty segments without whitespace, backslashes, `.` or `..`",
        )
        .with_phase(DiagnosticPhase::Validation),
    );
    None
}

fn is_valid_go_import_path(import_path: &str) -> bool {
    !import_path.is_empty()
        && !import_path.chars().any(char::is_whitespace)
        && !import_path.contains('\\')
        && import_path
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
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

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{Diagnostic, GeneratorDef, discover_generators, is_valid_go_import_path};

    fn go_manifest(threshold: Option<i64>) -> String {
        let threshold = threshold
            .map(|value| format!("max_typed_union_arity = {value}\n"))
            .unwrap_or_default();
        format!(
            "[package]\nname = \"test\"\n\n[generator.go]\noutput_type = \"go\"\nnaming_convention = \"language\"\nsdk_import_path = \"example.com/test/baml_sdk\"\n{threshold}"
        )
    }

    fn discover_with_manifest(content: &str) -> (Vec<GeneratorDef>, Vec<Diagnostic>) {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("baml.toml"), content).unwrap();
        discover_generators(directory.path())
    }

    #[test]
    fn go_import_paths_reject_relative_empty_and_platform_specific_segments() {
        for invalid in [
            "",
            "/example.com/sdk",
            "example.com/sdk/",
            "example.com//sdk",
            "./sdk",
            "../sdk",
            "example.com/./sdk",
            "example.com/../sdk",
            "example.com\\project\\sdk",
            "example.com/project sdk",
        ] {
            assert!(!is_valid_go_import_path(invalid), "accepted {invalid:?}");
        }
        assert!(is_valid_go_import_path("example.com/project/baml_sdk"));
    }

    #[test]
    fn go_union_threshold_defaults_to_three_and_accepts_zero() {
        let (defaults, default_diags) = discover_with_manifest(&go_manifest(None));
        assert!(default_diags.is_empty(), "{default_diags:?}");
        assert_eq!(defaults[0].max_typed_union_arity, 3);

        let (disabled, disabled_diags) = discover_with_manifest(&go_manifest(Some(0)));
        assert!(disabled_diags.is_empty(), "{disabled_diags:?}");
        assert_eq!(disabled[0].max_typed_union_arity, 0);
    }

    #[test]
    fn negative_go_union_threshold_is_rejected() {
        let (_, diagnostics) = discover_with_manifest(&go_manifest(Some(-1)));
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(
            format!("{diagnostics:?}").contains("zero or greater"),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn go_union_threshold_on_non_go_generator_is_rejected() {
        let manifest = "[package]\nname = \"test\"\n\n[generator.ts]\noutput_type = \"typescript/node\"\nnaming_convention = \"language\"\nmax_typed_union_arity = 3\n";
        let (_, diagnostics) = discover_with_manifest(manifest);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(
            format!("{diagnostics:?}").contains("Go-only property"),
            "{diagnostics:?}"
        );
    }
}
