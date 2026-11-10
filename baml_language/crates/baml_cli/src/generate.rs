#![allow(clippy::print_stderr)]

use std::{
    collections::HashMap,
    io::{self, Write},
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{Context, Result, anyhow};
use baml_db::{
    FileId, Span,
    baml_compiler_diagnostics::{Diagnostic, DiagnosticId, DiagnosticPhase, Severity, render},
};
use baml_sdkgen_types::{
    GeneratedOutputFile, Generator, NamingConvention, NamingSupport, OutputType, SkipWarning,
    write_generated_output,
};
use clap::{
    Args, Subcommand,
    builder::{PossibleValuesParser, TypedValueParser},
};
use serde::Serialize;
use text_size::{TextRange, TextSize};
use toml::Spanned;
use toml_edit::{DocumentMut, Item, Table, value};

use crate::{
    ExitCode,
    commands::release_version,
    output::{Command, CommandContext, Outcome, Report, Style},
    reporter::Reporter,
};

mod package_name;

/// What `baml generate` or `baml generate add` did.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum GenerateReport {
    Generated {
        generators: Vec<GeneratedSdk>,
    },
    Added {
        generator: String,
        manifest: PathBuf,
    },
}

#[derive(Debug, Serialize)]
pub(crate) struct GeneratedSdk {
    pub name: String,
    pub output_dir: PathBuf,
    /// Every file in the SDK, relative to `output_dir`.
    pub files: Vec<PathBuf>,
    /// Identifiers renamed to be legal in the target language.
    pub renames: Vec<IdentifierRename>,
}

#[derive(Debug, Serialize)]
pub(crate) struct IdentifierRename {
    pub kind: String,
    pub fqn: String,
    pub original: String,
    pub generated: String,
    pub reason: String,
}

impl Report for GenerateReport {
    fn text(&self, _out: &mut dyn Write, _style: &Style) -> io::Result<()> {
        Ok(())
    }
}

fn generated_status(sdk: &GeneratedSdk) -> String {
    let renames = sdk.renames.len();
    let label = if renames == 1 {
        "identifier rename"
    } else {
        "identifier renames"
    };
    format!(
        "{} ({} file(s), {renames} {label} → {})",
        sdk.name,
        sdk.files.len(),
        sdk.output_dir.display()
    )
}

/// Generate client code from BAML definitions.
///
/// Reads every `[generator.<name>]` section in `baml.toml`, validates the
/// project, and writes each configured client. Use `--output-dir` to override the
/// configured output directory for every generator in this invocation.
#[derive(Args, Clone, Debug)]
#[command(after_long_help = "\
Examples:
  Generate clients for the nearest project:
    baml generate

  Generate clients for a specific project:
    baml generate --project-dir ./my-project

  Override the output directory:
    baml generate --output-dir ./generated")]
pub struct GenerateArgs {
    #[command(flatten)]
    pub telemetry: crate::embed_telemetry::EmbedTelemetryArgs,
    #[command(subcommand)]
    pub command: Option<GenerateCommand>,

    #[command(flatten)]
    pub compiler: crate::commands::CompilerArgs,

    /// Write each SDK to exactly this directory, overriding both
    /// `output_dir` and the directory `package_name` selects
    #[arg(
        long = "output-dir",
        alias = "output",
        short = 'o',
        value_name = "PATH",
        help_heading = "Generation options"
    )]
    pub output: Option<PathBuf>,

    /// List every symbol a generator skips, including the standard
    /// library's, instead of summarizing those in one line
    #[arg(long, help_heading = "Generation options")]
    pub show_all_skips: bool,
}

#[derive(Subcommand, Clone, Debug)]
pub enum GenerateCommand {
    /// Add a client generator to baml.toml.
    #[command(after_long_help = "\
Examples:
  Add a Python client:
    baml generate add python/pydantic2

  Add a Go client:
    baml generate add go --sdk-import-path example.com/app/baml_sdk")]
    Add(AddGeneratorArgs),
}

#[derive(Args, Clone, Debug)]
pub struct AddGeneratorArgs {
    #[arg(value_name = "OUTPUT_TYPE", value_parser = add_output_type_parser())]
    pub output_type: OutputType,

    /// Go module import path for the generated baml_sdk package.
    #[arg(long, value_name = "IMPORT_PATH")]
    pub sdk_import_path: Option<String>,
}

/// A validated generator, resolved from a `[generator.<name>]` section of
/// `baml.toml`.
struct GeneratorDef {
    name: String,
    output_type: OutputType,
    /// Resolved `output_dir` (absolute): the directory the SDK is placed under.
    output_root: PathBuf,
    /// Resolved SDK directory (absolute): `output_root` joined with the
    /// directory `package_name` selects.
    output_dir: PathBuf,
    /// `package_name` when the generator section sets it.
    package_name: Option<String>,
    /// Required `naming_convention` from the generator section. No default
    /// is permitted — generators must spell out the policy explicitly.
    naming_convention: NamingConvention,
    /// Required for Go so generated packages can import the SDK root and one
    /// another. Other generators leave this unset.
    sdk_import_path: Option<String>,
    max_typed_union_arity: usize,
}

impl AddGeneratorArgs {
    fn run(&self, cx: &CommandContext) -> Result<Outcome<GenerateReport>> {
        let root = crate::project_load::find_project_root_from(cx.project_dir.as_deref())?
            .ok_or_else(|| {
                anyhow!("no BAML project found; run `baml init` before adding a generator")
            })?;
        let toml_path = root.join("baml.toml");
        if !toml_path.is_file() {
            anyhow::bail!(
                "`{}` has no baml.toml; run `baml init` before adding a generator",
                root.display()
            );
        }

        let content = std::fs::read_to_string(&toml_path)
            .with_context(|| format!("failed to read {}", toml_path.display()))?;
        let manifest = baml_db::manifest::parse(&content)
            .with_context(|| format!("failed to parse {}", toml_path.display()))?;
        baml_db::manifest::package_name(&manifest, &toml_path)?;
        // Every other manifest reader rejects these, so accepting them here
        // would write a generator into a file that the next build refuses to
        // load, reporting a failure that names neither this command nor the
        // table it choked on.
        baml_db::manifest::reject_stdlib_only_tables(&manifest, &toml_path)?;

        let mut generator = Generator::from(self.output_type);
        match (self.output_type, self.sdk_import_path.as_deref()) {
            (OutputType::Go, Some(import_path)) if is_valid_go_import_path(import_path) => {
                generator.sdk_import_path = Some(import_path.to_string());
            }
            (OutputType::Go, Some(import_path)) => {
                anyhow::bail!("invalid Go SDK import path `{import_path}`");
            }
            (OutputType::Go, None) => {
                anyhow::bail!("the Go generator requires `--sdk-import-path <MODULE>/baml_sdk`");
            }
            (_, Some(_)) => {
                anyhow::bail!("`--sdk-import-path` is only valid for the Go generator");
            }
            (_, None) => {}
        }

        let (updated, name) = add_generator_to_manifest(&content, &generator)
            .with_context(|| format!("failed to update {}", toml_path.display()))?;
        std::fs::write(&toml_path, updated)
            .with_context(|| format!("failed to write {}", toml_path.display()))?;

        Reporter::new().finish(
            "Added",
            format!("generator.{name} to {}", toml_path.display()),
        );
        Ok(Outcome::ok(GenerateReport::Added {
            generator: name,
            manifest: toml_path,
        }))
    }
}

fn parse_add_output_type(value: &str) -> Result<OutputType, String> {
    OutputType::all()
        .iter()
        .copied()
        .find(|output_type| value == output_type.add_name())
        .ok_or_else(|| {
            let expected = OutputType::all()
                .iter()
                .copied()
                .map(OutputType::add_name)
                .collect::<Vec<_>>()
                .join(", ");
            format!("unknown generator output type `{value}`; expected one of: {expected}")
        })
}

fn add_output_type_parser() -> impl TypedValueParser<Value = OutputType> {
    PossibleValuesParser::new(OutputType::all().iter().copied().map(OutputType::add_name)).map(
        |value| parse_add_output_type(&value).expect("possible generator output type must parse"),
    )
}

fn add_generator_to_manifest(content: &str, generator: &Generator) -> Result<(String, String)> {
    let manifest = baml_db::manifest::parse(content).context("invalid baml.toml")?;
    let name = (1..)
        .map(|index| format!("client{index}"))
        .find(|name| !manifest.generator.contains_key(name))
        .expect("generator index space is unbounded");

    let mut document = content
        .parse::<DocumentMut>()
        .context("invalid baml.toml")?;
    if document.get("generator").is_none() {
        let mut generators = Table::new();
        generators.set_implicit(true);
        document.insert("generator", Item::Table(generators));
    }
    let generators = document["generator"]
        .as_table_mut()
        .context("`generator` must be a TOML table")?;

    let mut table = Table::new();
    table.insert("output_type", value(generator.output_type.to_string()));
    if let Some(output_dir) = &generator.output_dir {
        table.insert("output_dir", value(output_dir));
    }
    table.insert(
        "naming_convention",
        value(generator.naming_convention.to_string()),
    );
    if let Some(sdk_import_path) = &generator.sdk_import_path {
        table.insert("sdk_import_path", value(sdk_import_path));
    }
    if let Some(max_typed_union_arity) = generator.max_typed_union_arity {
        let max_typed_union_arity = i64::try_from(max_typed_union_arity)
            .context("max_typed_union_arity exceeds TOML's integer range")?;
        table.insert("max_typed_union_arity", value(max_typed_union_arity));
    }
    generators.insert(&name, Item::Table(table));

    Ok((document.to_string(), name))
}

impl Command for GenerateArgs {
    type Output = GenerateReport;

    fn run(&self, cx: &CommandContext) -> Result<Outcome<GenerateReport>> {
        if self.command.is_some() && self.telemetry.embed_telemetry {
            anyhow::bail!(
                "`--embed-telemetry` applies to `baml generate`, which produces the SDK. Add the generator first, then run `baml generate --embed-telemetry`."
            );
        }
        match &self.command {
            Some(GenerateCommand::Add(args)) => args.run(cx),
            None => self.run_generate(cx),
        }
    }
}

impl GenerateArgs {
    fn run_generate(&self, cx: &CommandContext) -> Result<Outcome<GenerateReport>> {
        let failed = || {
            Ok(Outcome::new(
                GenerateReport::Generated {
                    generators: Vec::new(),
                },
                ExitCode::Failure,
            ))
        };
        let mut sdks = Vec::new();
        let reporter = Reporter::new();
        reporter.status(
            "Generating",
            format!("clients with CLI version: {}", release_version()),
        );
        // Codegen reads types across the whole project, so take the shared
        // read-only session: warm seeds where they are provably faithful and
        // the parallel index prime, same as describe.
        let mut session = crate::project_session::ProjectSession::open(
            cx.project_dir.as_deref(),
            crate::project_session::CacheUse::ReadOnly,
        )?;
        if session.is_empty() {
            crate::reporter::print_error(format_args!(
                "no .baml files found in {}",
                session.root().display()
            ));
            return failed();
        }
        let _ = session.warm_prep();
        session.prime();
        let (mut db, package, from) = (session.db, session.package, session.resolved.root);
        // `SourceFile` paths are canonicalized by `ProjectDatabase`. Canonicalize
        // the root too so Windows short paths and `\\?\` paths can be relativized.
        let from = from
            .canonicalize()
            .with_context(|| format!("failed to resolve project root {}", from.display()))?;
        // Compile-time diagnostics — same shape as run/pack: render the
        // diagnostic block after abandoning the spinner so the colored
        // source-snippet output doesn't fight with the lamb. No "Checking"
        // line here: the meaningful "Resolving" and "Compiling" phases below
        // carry the progress, and a "Checking N file(s)" would just duplicate
        // the "Compiling N file(s)" count.
        let source_files = package.files(&db).clone();
        let diagnostics = baml_db::collect_diagnostics(&db);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        if !errors.is_empty() {
            let rendered = crate::check_command::render_project_diagnostics(
                &db,
                &errors.iter().copied().cloned().collect::<Vec<_>>(),
            );
            eprintln!("{rendered}");
            return failed();
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
                &crate::output::diagnostic_render_config(),
            );
            eprintln!("{rendered}");
            return failed();
        }

        if generators.is_empty() {
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
            return failed();
        }

        let mut embedded_baml_toml = build_embedded_baml_toml(&from)?;
        let mut manifest: toml::Value = toml::from_str(&embedded_baml_toml)?;
        let mut telemetry: btel_settings::artifact::ArtifactTelemetry =
            manifest["__baml_codegen"]["telemetry"].clone().try_into()?;
        self.telemetry.apply(
            &mut telemetry,
            &manifest,
            btel_settings::artifact::ArtifactKind::Bridge,
        )?;

        // Tests were checked above, but generated clients never run them.
        // Exclude both test forms before building any persisted compiler data,
        // including package interfaces and the registrars' nested lambdas.
        db.exclude_tests(package);
        // Exclusion invalidates the test-inclusive indexes primed above.
        // Rebuild them in parallel before the symbol pool's aggregate queries.
        baml_db::prime_file_indexes_parallel(&db);

        // Build the codegen SymbolPool from the compiler database.
        let pool = baml_ide::build_symbol_pool(&db, package);

        reporter.spin("Compiling", format!("{} file(s)", source_files.len()));
        let program = db
            .get_bytecode(package)
            .map_err(|e| anyhow!("compilation failed: {e}"))?;
        let baml_bytecode = baml_artifact::encode(baml_artifact::ArtifactKind::Program, &program)
            .map_err(|e| anyhow!("failed to serialize BAML bytecode: {e}"))?;
        if self.telemetry.embed_telemetry {
            manifest["__baml_codegen"]["telemetry"] = toml::Value::try_from(telemetry)?;
            // Replace only owned metadata; preserve the publisher's original manifest bytes.
            let original = std::fs::read_to_string(from.join("baml.toml"))?;
            let owned = toml::Table::from_iter([(
                "__baml_codegen".to_owned(),
                manifest["__baml_codegen"].clone(),
            )]);
            embedded_baml_toml = format!("{original}\n{}", toml::to_string(&owned)?);
        }
        let source_root = from.join("baml_src");
        let user_baml_files = source_files
            .iter()
            .map(|source_file| {
                let path = source_file.path(&db);
                let relative_path = path
                    .strip_prefix(&source_root)
                    .or_else(|_| path.strip_prefix(&from))
                    .with_context(|| {
                        format!(
                            "BAML source {} is outside project root {}",
                            path.display(),
                            from.display()
                        )
                    })?;
                Ok((
                    relative_path.to_path_buf(),
                    source_file.text(&db).to_string(),
                ))
            })
            .collect::<Result<Vec<_>>>()?;

        let mut total_files = 0;

        // `--output-dir` names the final directory as given; nothing derived
        // from `package_name` is appended to it.
        let destinations = generators
            .iter()
            .map(|generator| {
                Ok(match &self.output {
                    Some(requested) if requested.is_absolute() => requested.clone(),
                    Some(requested) => std::env::current_dir()
                        .context("failed to resolve the current directory for generated output")?
                        .join(requested),
                    None => generator.output_dir.clone(),
                })
            })
            .collect::<Result<Vec<PathBuf>>>()?;
        // A named SDK only installs where it cannot clobber hand-written code,
        // wherever the destination came from. Every destination is checked
        // before any generator writes, so a refusal leaves no partial run.
        for (generator, output_dir) in generators.iter().zip(&destinations) {
            if generator.package_name.is_some() {
                package_name::ensure_claimable(&generator.name, output_dir)?;
            }
        }

        for (generator, output_dir) in generators.iter().zip(destinations.iter().cloned()) {
            reporter.spin("Generating", &generator.name);

            if generator.output_type == OutputType::CSharp {
                let report = sdkgen_csharp::generate_into(sdkgen_csharp::CSharpGenerateRequest {
                    symbols: &pool,
                    program_bytes: &baml_bytecode,
                    embedded_baml_toml: &embedded_baml_toml,
                    cli_version: release_version(),
                    required_bridge_version: baml_version::CANONICAL_VERSION,
                    program_identity: &generator.name,
                    output_directory: output_dir.clone(),
                })?;
                let count = report.written_files.len();
                reporter.status(
                    "Generated",
                    format!(
                        "{} ({count} file(s), 0 identifier renames → {})",
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
            let mut identifier_renames = Vec::new();
            let generated: Vec<(PathBuf, Vec<u8>)> = match generator.output_type {
                OutputType::PythonPydantic | OutputType::PythonPydanticV1 => {
                    let generated = sdkgen_python_pydantic2::generate_with_bytecode_and_metadata_and_source_files(
                        &pool,
                        &baml_bytecode,
                        &embedded_baml_toml,
                        &user_baml_files,
                        generator.naming_convention,
                    );
                    identifier_renames = generated.renames;
                    generated.files
                    .into_iter()
                    .map(|(path, content)| (path, content.into_bytes()))
                    .collect()
                }
                OutputType::TypescriptNode => {
                    sdkgen_typescript_shared::sdkgen_typescript::to_source_code_with_bytecode_and_metadata(
                        &pool,
                        &baml_bytecode,
                        &embedded_baml_toml,
                        generator.naming_convention,
                    )
                    .into_iter()
                    .map(|(path, content)| (path, content.into_bytes()))
                    .collect()
                }
                OutputType::TypescriptWeb => {
                    sdkgen_typescript_shared::sdkgen_typescript_web::to_source_code_with_bytecode_and_metadata(
                        &pool,
                        &baml_bytecode,
                        &embedded_baml_toml,
                        generator.naming_convention,
                    )
                    .into_iter()
                    .map(|(path, content)| (path, content.into_bytes()))
                    .collect()
                }
                OutputType::Rust => {
                    // The Cargo package and the library users import share
                    // the configured name.
                    let crate_name = generator
                        .package_name
                        .as_deref()
                        .unwrap_or(sdkgen_rust::DEFAULT_LIB_NAME);
                    let generated = sdkgen_rust::to_source_code_with_bytecode_and_metadata(
                        &pool,
                        &baml_bytecode,
                        &embedded_baml_toml,
                        &sdkgen_rust::RustGenOptions {
                            naming_convention: generator.naming_convention,
                            package_name: crate_name.to_string(),
                            lib_name: crate_name.to_string(),
                            // The runtime crate is not published yet; pin the
                            // matching version for when it is.
                            runtime_dep: format!("\"={}\"", baml_version::CANONICAL_VERSION),
                            manifest_extra: None,
                            edition: "2024".to_string(),
                        },
                    );
                    report_skipped(&reporter, "Rust", &generated.warnings, self.show_all_skips);
                    generated
                        .files
                        .into_iter()
                        .map(|(path, content)| (path, content.into_bytes()))
                        .collect()
                }
                OutputType::Go => {
                    let generated = sdkgen_go::try_to_source_code_with_bytecode_and_metadata_and_options(
                        &pool,
                        &baml_bytecode,
                        &embedded_baml_toml,
                        &sdkgen_go::GoGenOptions {
                            naming_convention: generator.naming_convention,
                            sdk_import_path: generator
                                .sdk_import_path
                                .as_deref()
                                .expect("validated Go generator must have sdk_import_path"),
                            package_name: generator
                                .package_name
                                .as_deref()
                                .unwrap_or(sdkgen_go::DEFAULT_PACKAGE_NAME),
                            max_typed_union_arity: generator.max_typed_union_arity,
                        },
                    )
                    .map_err(|error| anyhow!("failed to generate Go SDK: {error}"))?;
                    report_skipped(&reporter, "Go", &generated.warnings, self.show_all_skips);
                    generated
                        .files
                        .into_iter()
                        .map(|(path, content)| (path, content.into_bytes()))
                        .collect()
                }
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
                    sdkgen_cpp::to_source_code_with_bytecode_and_metadata(
                        &pool,
                        &source_paths,
                        &baml_bytecode,
                        &embedded_baml_toml,
                    )
                        .into_iter()
                        .map(|(path, content)| (path, content.into_bytes()))
                        .collect()
                }
                OutputType::Java => sdkgen_java::to_source_code_with_bytecode_and_metadata(
                    &pool,
                    &baml_bytecode,
                    &embedded_baml_toml,
                    generator.naming_convention,
                )
                .into_iter()
                .map(|(path, content)| (path, content.into_bytes()))
                .collect(),
                OutputType::Swift => sdkgen_swift::to_source_code_with_bytecode_and_metadata(
                    &pool,
                    &baml_bytecode,
                    &embedded_baml_toml,
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
            let sdk = GeneratedSdk {
                name: generator.name.clone(),
                output_dir: output_dir.clone(),
                files: report.written_files,
                renames: identifier_renames
                    .into_iter()
                    .map(|rename| IdentifierRename {
                        kind: rename.kind,
                        fqn: rename.fqn,
                        original: rename.original,
                        generated: rename.generated,
                        reason: rename.reason.to_string(),
                    })
                    .collect(),
            };
            // One persistent status line per generator, like cargo's
            // `   Compiling foo v0.1.0`.
            reporter.status("Generated", generated_status(&sdk));
            sdks.push(sdk);
            if self.output.is_none()
                && generator.package_name.is_some()
                && let Some(previous) =
                    package_name::superseded_default_output(&generator.output_root, &destinations)
            {
                reporter.warning(format!(
                    "generator `{}` now writes to {}; the earlier output in {} is no longer \
                     updated and can be deleted",
                    generator.name,
                    output_dir.display(),
                    previous.display()
                ));
            }
            total_files += count;
        }

        if total_files == 0 {
            crate::reporter::print_error("no files generated (no supported generators found)");
            return failed();
        }

        reporter.finish("Finished", format!("generated {total_files} file(s)"));
        Ok(Outcome::ok(GenerateReport::Generated { generators: sdks }))
    }
}

/// A generator left these symbols out of its SDK; say so instead of letting
/// them disappear. Each skip in the project's own package gets its own line.
/// Dependency skips, in practice hundreds from the standard library, collapse
/// into one line unless `show_all` asks for the list.
fn report_skipped(reporter: &Reporter, sdk: &str, warnings: &[SkipWarning], show_all: bool) {
    let mut dependency_skips = 0;
    for warning in warnings {
        if warning.local || show_all {
            reporter.warning(format!("skipped `{}`: {}", warning.fqn, warning.reason));
        } else {
            dependency_skips += 1;
        }
    }
    if dependency_skips > 0 {
        reporter.warning(format!(
            "skipped {dependency_skips} standard-library and dependency symbol(s) the {sdk} generator cannot represent yet (run with --show-all-skips to list them)"
        ));
    }
}

fn build_embedded_baml_toml(project_root: &Path) -> Result<String> {
    let path = project_root.join("baml.toml");
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let document = content
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {}", path.display()))?;
    if document.contains_key("__baml_codegen") {
        anyhow::bail!(
            "{} uses reserved table `[__baml_codegen]`; remove it because `baml generate` owns generated-bytecode metadata.",
            path.display()
        );
    }

    let mut embedded = content;
    if !embedded.ends_with('\n') {
        embedded.push('\n');
    }
    let manifest: toml::Value = toml::from_str(&embedded)?;
    let telemetry = btel_settings::artifact::ArtifactTelemetry::from_manifest(
        &manifest,
        btel_settings::artifact::ArtifactKind::Bridge,
    )?;
    let metadata = toml::Table::from_iter([
        ("metadata_version".to_owned(), toml::Value::Integer(1)),
        (
            "toolchain".to_owned(),
            toml::Value::Table(toml::Table::from_iter([(
                "version".to_owned(),
                toml::Value::String(baml_version::CANONICAL_VERSION.to_owned()),
            )])),
        ),
        ("telemetry".to_owned(), toml::Value::try_from(telemetry)?),
    ]);
    let owned =
        toml::Table::from_iter([("__baml_codegen".to_owned(), toml::Value::Table(metadata))]);
    embedded.push('\n');
    embedded.push_str(&toml::to_string(&owned)?);
    Ok(embedded)
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
    let Ok(manifest) = baml_db::manifest::parse(&content) else {
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
        let package_name = match (output_type, generator.package_name.as_ref()) {
            (Some(output_type), Some(value)) => {
                parse_package_name(name, output_type, value, &mut diags)
            }
            (_, None) => Ok(None),
            // An unknown `output_type` already has its diagnostic.
            (None, Some(_)) => Err(()),
        };

        // Skip codegen for sections that failed validation; their
        // diagnostics block the run upstream.
        let (Some(output_type), Some(naming_convention), Ok(package_name)) =
            (output_type, naming_convention, package_name)
        else {
            continue;
        };
        // `output_dir` is resolved relative to the project root and defaults
        // to the project root; the SDK's own directory goes beneath it.
        let output_root = root.join(generator.output_dir.as_deref().unwrap_or("."));
        let output_dir = output_root.join(package_name::relative_directory(
            output_type,
            package_name.as_deref(),
        ));
        if let NamingSupport::Only(required_naming_convention) = output_type.naming_support()
            && naming_convention != required_naming_convention
        {
            let range = generator
                .naming_convention
                .as_ref()
                .map(|value| to_text_range(value.span()))
                .unwrap_or(table_range);
            diags.push(
                Diagnostic::error(
                    DiagnosticId::InvalidGeneratorPropertyValue,
                    format!(
                        "generator `{name}` with `output_type = \"{output_type}\"` requires `naming_convention = \"{required_naming_convention}\"`"
                    ),
                )
                .with_primary(
                    Span {
                        file_id: manifest_file_id(),
                        range,
                    },
                    format!("use `{required_naming_convention}` for `{output_type}`"),
                )
                .with_phase(DiagnosticPhase::Validation),
            );
            continue;
        }
        if output_type == OutputType::Go && sdk_import_path.is_none() {
            continue;
        }
        if let (Some(import_path), Some(package_name), Some(value)) = (
            sdk_import_path.as_deref(),
            package_name.as_deref(),
            generator.sdk_import_path.as_ref(),
        ) && !go_import_path_names_package(import_path, package_name)
        {
            // Suggest keeping the module prefix and renaming the last segment.
            let expected = match import_path.rsplit_once('/') {
                Some((prefix, _)) => format!("{prefix}/{package_name}"),
                None => format!("example.com/{package_name}"),
            };
            diags.push(
                Diagnostic::error(
                    DiagnosticId::InvalidGeneratorPropertyValue,
                    format!(
                        "Go generator `{name}` sets `package_name = \"{package_name}\"`, so \
                         the last segment of `sdk_import_path` must be `{package_name}`, e.g. \
                         `{expected}`"
                    ),
                )
                .with_primary(
                    Span {
                        file_id: manifest_file_id(),
                        range: to_text_range(value.span()),
                    },
                    format!("expected `{expected}`"),
                )
                .with_phase(DiagnosticPhase::Validation),
            );
            continue;
        }

        generators.push(GeneratorDef {
            name: name.clone(),
            output_type,
            output_root,
            output_dir,
            package_name,
            naming_convention,
            sdk_import_path,
            max_typed_union_arity,
        });
    }

    (generators, diags)
}

/// Validate an explicit `package_name` for `output_type`. `Err` means a
/// diagnostic was pushed.
fn parse_package_name(
    generator_name: &str,
    output_type: OutputType,
    value: &Spanned<String>,
    diags: &mut Vec<Diagnostic>,
) -> Result<Option<String>, ()> {
    let package_name = value.get_ref();
    let (message, label) = match package_name::validate(output_type, package_name) {
        Ok(()) => return Ok(Some(package_name.clone())),
        Err(package_name::PackageNameError::Unsupported) => (
            format!("`package_name` is not supported for output_type `{output_type}` yet"),
            format!("remove `package_name` from generator `{generator_name}`"),
        ),
        Err(package_name::PackageNameError::Invalid(expected)) => (
            format!("invalid `package_name` `{package_name}` on generator `{generator_name}`"),
            expected.to_string(),
        ),
    };
    diags.push(
        Diagnostic::error(DiagnosticId::InvalidGeneratorPropertyValue, message)
            .with_primary(
                Span {
                    file_id: manifest_file_id(),
                    range: to_text_range(value.span()),
                },
                label,
            )
            .with_phase(DiagnosticPhase::Validation),
    );
    Err(())
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

/// By Go convention an import path's last segment is the package's name.
/// Requiring it keeps `import ".../alien_api"` and the `alien_api.` qualifier
/// in step. A bare `alien_api` is the root of a module named `alien_api`.
fn go_import_path_names_package(import_path: &str, package_name: &str) -> bool {
    import_path.rsplit('/').next() == Some(package_name)
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
    use std::path::PathBuf;

    use crate::output::CommandContext;
    use std::fs;

    use super::{
        AddGeneratorArgs, Diagnostic, GenerateArgs, GeneratedSdk, Generator, GeneratorDef,
        IdentifierRename, NamingConvention, NamingSupport, OutputType, add_generator_to_manifest,
        build_embedded_baml_toml, discover_generators, generated_status, is_valid_go_import_path,
        parse_add_output_type,
    };

    /// Test blocks and checkout paths must not affect persisted production bytecode.
    #[test]
    fn generation_bytecode_is_unchanged_by_test_blocks() {
        let directory = tempfile::tempdir().unwrap();
        let production = r#"
function Halve(n: int) -> int {
    let outer = (x: int) -> int {
        let inner = (y: int) -> int { y / 2 }
        inner(x)
    };
    outer(n)
}

function Recover() -> string {
    (throw "production throw") catch (e) {
        _ => "production recovery"
    }
}
"#;
        let tests = r#"
test "ONLY_TEST_5246_inline" { assert.equal(Halve(4), 2) }

testset "ONLY_TEST_5246_suite" {
    let setup = "ONLY_TEST_5246_setup";
    test "leaf" { assert.equal(setup, "ONLY_TEST_5246_setup") }
    testset "nested" {
        test "deep" {
            let outer = () -> int {
                let inner = () -> int { Halve(8) };
                inner()
            };
            let caught = (throw "ONLY_TEST_5246_throw") catch (e) {
                _ => outer()
            };
            assert.equal(caught, 4);
        }
    }
}
"#;
        let mut bytecodes = Vec::new();
        for (name, extra) in [("project1", ""), ("project2", tests)] {
            let project = directory.path().join(name);
            fs::create_dir_all(project.join("baml_src")).unwrap();
            fs::write(
                project.join("baml_src/file1.baml"),
                format!("{production}{extra}"),
            )
            .unwrap();
            fs::write(
                project.join("baml.toml"),
                r#"[package]
name = "generate_without_tests"
[generator.rust]
output_type = "rust"
output_dir = "generated"
naming_convention = "preserve-case"
"#,
            )
            .unwrap();

            let result = GenerateArgs {
                telemetry: Default::default(),
                command: None,
                compiler: Default::default(),
                output: None,
                show_all_skips: false,
            }
            .run_generate(&CommandContext::for_test(Some(&project)))
            .unwrap();
            assert!(matches!(result.status, crate::ExitCode::Success));
            bytecodes
                .push(fs::read(project.join("generated/baml_sdk/src/_inlinedbaml.bin")).unwrap());
        }
        assert!(
            bytecodes[0] == bytecodes[1],
            "test/testset blocks or checkout paths changed the generated bytecode"
        );
    }

    #[test]
    fn generated_status_counts_files_and_renames() {
        let sdk = GeneratedSdk {
            name: "py".to_string(),
            output_dir: PathBuf::from("generated"),
            files: vec![PathBuf::from("a.py"); 4],
            renames: vec![IdentifierRename {
                kind: "enum variant".to_string(),
                fqn: "user.Choice.None".to_string(),
                original: "None".to_string(),
                generated: "None_".to_string(),
                reason: "Python keyword".to_string(),
            }],
        };
        assert_eq!(
            generated_status(&sdk),
            "py (4 file(s), 1 identifier rename → generated)"
        );
    }

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
    fn generator_output_directories_use_the_shared_convention() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("baml.toml"),
            "[package]\nname = \"test\"\n\n[generator.csharp_default]\noutput_type = \"csharp\"\nnaming_convention = \"language\"\n\n[generator.csharp_explicit]\noutput_type = \"csharp\"\noutput_dir = \"generated\"\nnaming_convention = \"language\"\n\n[generator.python]\noutput_type = \"python/pydantic\"\nnaming_convention = \"preserve-case\"\n",
        )
        .unwrap();

        let (generators, diagnostics) = discover_generators(directory.path());

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let output_for = |name: &str| {
            &generators
                .iter()
                .find(|generator| generator.name == name)
                .unwrap()
                .output_dir
        };
        assert_eq!(
            output_for("csharp_default"),
            &directory.path().join("baml_sdk")
        );
        assert_eq!(
            output_for("csharp_explicit"),
            &directory.path().join("generated").join("baml_sdk")
        );
        assert_eq!(output_for("python"), &directory.path().join("baml_sdk"));
    }

    fn python_manifest(package_name: &str) -> String {
        format!(
            "[package]\nname = \"test\"\n\n[generator.py]\noutput_type = \"python/pydantic\"\nnaming_convention = \"language\"\noutput_dir = \"out\"\npackage_name = \"{package_name}\"\n"
        )
    }

    #[test]
    fn python_package_name_nests_the_sdk_under_output_dir() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("baml.toml"),
            python_manifest("alien_sdk._generated"),
        )
        .unwrap();

        let (generators, diagnostics) = discover_generators(directory.path());

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(
            generators[0].package_name.as_deref(),
            Some("alien_sdk._generated")
        );
        assert_eq!(generators[0].output_root, directory.path().join("out"));
        assert_eq!(
            generators[0].output_dir,
            directory.path().join("out/alien_sdk/_generated")
        );
    }

    #[test]
    fn invalid_python_package_names_are_rejected_at_their_value() {
        for (package_name, expected) in [
            ("alien-sdk", "dotted Python import path"),
            ("alien_sdk.class", "dotted Python import path"),
            ("alien_sdk..x", "dotted Python import path"),
            ("", "dotted Python import path"),
            ("baml_bridge.sdk", "BAML Python runtime"),
        ] {
            let manifest = python_manifest(package_name);
            let (generators, diagnostics) = discover_with_manifest(&manifest);
            assert!(generators.is_empty(), "{package_name}");
            assert_eq!(diagnostics.len(), 1, "{package_name}: {diagnostics:?}");
            let diagnostic = &diagnostics[0];
            assert_eq!(
                diagnostic.id,
                baml_db::baml_compiler_diagnostics::DiagnosticId::InvalidGeneratorPropertyValue
            );
            assert_eq!(
                diagnostic.message,
                format!("invalid `package_name` `{package_name}` on generator `py`")
            );
            assert!(
                format!("{diagnostic:?}").contains(expected),
                "{package_name}: {diagnostic:?}"
            );
            let value = format!("\"{package_name}\"");
            let start = manifest.find(&value).unwrap();
            assert_eq!(
                diagnostic.primary_span().unwrap().range,
                super::to_text_range(start..start + value.len())
            );
        }
    }

    #[test]
    fn package_name_is_rejected_on_targets_that_cannot_rename_their_sdk() {
        for output_type in [
            OutputType::Swift,
            OutputType::Java,
            OutputType::Cpp,
            OutputType::CSharp,
        ] {
            let sdk_import_path = "";
            let naming_convention = output_type.naming_support().default_convention();
            let (generators, diagnostics) = discover_with_manifest(&format!(
                "[package]\nname = \"test\"\n\n[generator.client]\noutput_type = \"{output_type}\"\nnaming_convention = \"{naming_convention}\"\n{sdk_import_path}package_name = \"my_sdk\"\n"
            ));
            assert!(generators.is_empty(), "{output_type}");
            assert_eq!(diagnostics.len(), 1, "{output_type}: {diagnostics:?}");
            assert_eq!(
                diagnostics[0].message,
                format!("`package_name` is not supported for output_type `{output_type}` yet")
            );
        }
    }

    fn manifest_with_package_name(output_type: OutputType, extra: &str) -> String {
        let naming_convention = output_type.naming_support().default_convention();
        format!(
            "[package]\nname = \"test\"\n\n[generator.client]\noutput_type = \"{output_type}\"\nnaming_convention = \"{naming_convention}\"\noutput_dir = \"out\"\npackage_name = \"alien_api\"\n{extra}"
        )
    }

    #[test]
    fn go_rust_and_typescript_package_names_name_the_sdk_directory() {
        for (output_type, extra) in [
            (
                OutputType::Go,
                "sdk_import_path = \"example.com/test/alien_api\"\n",
            ),
            (OutputType::Rust, ""),
            (OutputType::TypescriptNode, ""),
            (OutputType::TypescriptWeb, ""),
        ] {
            let directory = tempfile::tempdir().unwrap();
            fs::write(
                directory.path().join("baml.toml"),
                manifest_with_package_name(output_type, extra),
            )
            .unwrap();

            let (generators, diagnostics) = discover_generators(directory.path());

            assert!(diagnostics.is_empty(), "{output_type}: {diagnostics:?}");
            assert_eq!(
                generators[0].output_dir,
                directory.path().join("out/alien_api"),
                "{output_type}"
            );
        }
    }

    #[test]
    fn go_import_path_must_end_in_the_configured_package_name() {
        let manifest = manifest_with_package_name(
            OutputType::Go,
            "sdk_import_path = \"example.com/test/baml_sdk\"\n",
        );
        let (generators, diagnostics) = discover_with_manifest(&manifest);

        assert!(generators.is_empty());
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(
            diagnostics[0].message,
            "Go generator `client` sets `package_name = \"alien_api\"`, so the last segment of `sdk_import_path` must be `alien_api`, e.g. `example.com/test/alien_api`"
        );
        let value = "\"example.com/test/baml_sdk\"";
        let start = manifest.find(value).unwrap();
        assert_eq!(
            diagnostics[0].primary_span().unwrap().range,
            super::to_text_range(start..start + value.len())
        );

        // Without `package_name`, the import path's last segment is free.
        let (generators, diagnostics) = discover_with_manifest(
            &go_manifest(None).replace("example.com/test/baml_sdk", "example.com/test/client"),
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(generators.len(), 1);
    }

    #[test]
    fn go_import_path_compares_whole_segments() {
        assert!(super::go_import_path_names_package(
            "alien_api",
            "alien_api"
        ));
        assert!(super::go_import_path_names_package(
            "example.com/test/alien_api",
            "alien_api"
        ));
        assert!(!super::go_import_path_names_package(
            "example.com/test/my_alien_api",
            "alien_api"
        ));
        assert!(!super::go_import_path_names_package(
            "my_alien_api",
            "alien_api"
        ));
    }

    #[test]
    fn invalid_go_rust_and_typescript_package_names_are_rejected() {
        for (output_type, package_name, extra) in [
            (
                OutputType::Go,
                "Alien",
                "sdk_import_path = \"example.com/test/Alien\"\n",
            ),
            (OutputType::Rust, "baml_bridge", ""),
            (OutputType::TypescriptNode, "a/b", ""),
        ] {
            let manifest = manifest_with_package_name(output_type, extra)
                .replace("\"alien_api\"", &format!("\"{package_name}\""));
            let (generators, diagnostics) = discover_with_manifest(&manifest);
            assert!(generators.is_empty(), "{output_type}");
            assert_eq!(diagnostics.len(), 1, "{output_type}: {diagnostics:?}");
            assert_eq!(
                diagnostics[0].message,
                format!("invalid `package_name` `{package_name}` on generator `client`")
            );
        }
    }

    fn generate_project(project: &std::path::Path) -> anyhow::Result<crate::ExitCode> {
        GenerateArgs {
            telemetry: Default::default(),
            command: None,
            compiler: Default::default(),
            output: None,
            show_all_skips: false,
        }
        .run_generate(&CommandContext::for_test(Some(project)))
        .map(|outcome| outcome.status)
    }

    fn python_project(package_name: &str) -> tempfile::TempDir {
        let project = tempfile::tempdir().unwrap();
        fs::create_dir_all(project.path().join("baml_src")).unwrap();
        fs::write(
            project.path().join("baml_src/main.baml"),
            "class Person {\n  name string\n}\n\nfunction Echo(p: Person) -> Person {\n  p\n}\n",
        )
        .unwrap();
        fs::write(
            project.path().join("baml.toml"),
            python_manifest(package_name),
        )
        .unwrap();
        project
    }

    /// Every `.py`/`.pyi` file under `directory`, relative to it.
    fn python_files(directory: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut files = Vec::new();
        let mut pending = vec![directory.to_path_buf()];
        while let Some(next) = pending.pop() {
            for entry in fs::read_dir(next).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    pending.push(path);
                } else if path
                    .extension()
                    .is_some_and(|extension| extension == "py" || extension == "pyi")
                {
                    files.push(path.strip_prefix(directory).unwrap().to_path_buf());
                }
            }
        }
        files
    }

    #[test]
    fn python_sdk_generates_into_a_nested_package_without_naming_it() {
        let project = python_project("pkg.sub");

        let result = generate_project(project.path()).unwrap();

        assert!(matches!(result, crate::ExitCode::Success));
        let out = project.path().join("out");
        let sdk = out.join("pkg/sub");
        assert!(sdk.join("__init__.py").is_file());
        assert!(sdk.join("_typemap.py").is_file());
        // The application owns its parent packages.
        assert!(!out.join("pkg/__init__.py").exists());
        assert!(!out.join("pkg/.gitignore").exists());
        assert!(!out.join("baml_sdk").exists());
        let files = python_files(&sdk);
        assert!(files.len() > 10, "{files:?}");
        for file in files {
            let content = fs::read_to_string(sdk.join(&file)).unwrap();
            assert!(
                !content.contains("baml_sdk"),
                "{} names `baml_sdk`",
                file.display()
            );
        }
    }

    /// `baml describe python` and `baml describe typescript` show the call
    /// forms the generators emit: the plain name is async and must be
    /// awaited, and the blocking twin ends in `_sync`.
    #[test]
    fn describe_topics_match_the_generated_call_forms() {
        let project = python_project("unused");
        fs::write(
            project.path().join("baml_src/main.baml"),
            "class Ticket {\n  title string\n}\n\nfunction ClassifyTicket(text: string) -> Ticket {\n  Ticket { title: text }\n}\n",
        )
        .unwrap();
        fs::write(
            project.path().join("baml.toml"),
            "[package]\nname = \"test\"\n\n\
             [generator.py]\noutput_type = \"python/pydantic\"\nnaming_convention = \"language\"\noutput_dir = \"py\"\n\n\
             [generator.ts]\noutput_type = \"typescript/node\"\nnaming_convention = \"preserve-case\"\noutput_dir = \"ts\"\n",
        )
        .unwrap();

        let result = generate_project(project.path()).unwrap();

        assert!(matches!(result, crate::ExitCode::Success));
        for (topic, sdk_file, async_decl, sync_decl, name) in [
            (
                "python",
                "py/baml_sdk/__init__.pyi",
                "\nasync def classify_ticket(",
                "\ndef classify_ticket_sync(",
                "classify_ticket",
            ),
            (
                "typescript",
                "ts/baml_sdk/index.ts",
                "\nexport const ClassifyTicket = defineFunction(\"user.ClassifyTicket\", \"async\"",
                "\nexport const ClassifyTicket_sync = defineFunction(\"user.ClassifyTicket\", \"sync\"",
                "ClassifyTicket",
            ),
        ] {
            let sdk = fs::read_to_string(project.path().join(sdk_file)).unwrap();
            assert!(sdk.contains(async_decl), "{sdk_file}: {sdk}");
            assert!(sdk.contains(sync_decl), "{sdk_file}: {sdk}");
            let mut text = Vec::new();
            crate::describe_command::write_keyword(
                &mut text,
                &crate::paint::Painter::new(false, false),
                topic,
            )
            .unwrap();
            let text = String::from_utf8(text).unwrap();
            let calls = text.matches(&format!("{name}(")).count();
            assert!(calls > 0, "{topic}: {text}");
            assert_eq!(
                text.matches(&format!("await {name}(")).count(),
                calls,
                "`baml describe {topic}` calls the async `{name}` without `await`:\n{text}"
            );
            if topic == "python" {
                assert!(text.contains(&format!("{name}_sync(")), "{topic}: {text}");
            }
        }
    }

    #[test]
    fn named_rust_go_and_typescript_sdks_carry_their_name() {
        let project = python_project("unused");
        fs::write(
            project.path().join("baml.toml"),
            "[package]\nname = \"test\"\n\n\
             [generator.rs]\noutput_type = \"rust\"\nnaming_convention = \"language\"\noutput_dir = \"rust\"\npackage_name = \"alien_api\"\n\n\
             [generator.go]\noutput_type = \"go\"\nnaming_convention = \"language\"\noutput_dir = \"go\"\npackage_name = \"alien_api\"\nsdk_import_path = \"example.com/test/alien_api\"\n\n\
             [generator.ts]\noutput_type = \"typescript/node\"\nnaming_convention = \"preserve-case\"\noutput_dir = \"ts\"\npackage_name = \"alien-api\"\n",
        )
        .unwrap();

        let result = generate_project(project.path()).unwrap();

        assert!(matches!(result, crate::ExitCode::Success));
        let manifest =
            fs::read_to_string(project.path().join("rust/alien_api/Cargo.toml")).unwrap();
        assert!(
            manifest.contains("[package]\nname = \"alien_api\"\n"),
            "{manifest}"
        );
        assert!(
            manifest.contains("[lib]\nname = \"alien_api\"\n"),
            "{manifest}"
        );
        let invocation =
            fs::read_to_string(project.path().join("go/alien_api/invocation.go")).unwrap();
        assert!(invocation.contains("\npackage alien_api\n"), "{invocation}");
        assert!(project.path().join("ts/alien-api/index.ts").is_file());
        for default in ["rust", "go", "ts"] {
            assert!(!project.path().join(default).join("baml_sdk").exists());
        }
    }

    #[test]
    fn named_python_sdk_refuses_a_directory_of_hand_written_files() {
        let project = python_project("alien_sdk");
        let package = project.path().join("out/alien_sdk");
        fs::create_dir_all(&package).unwrap();
        fs::write(package.join("__init__.py"), "# hand-written\n").unwrap();

        let error = generate_project(project.path()).unwrap_err().to_string();

        assert!(error.contains("refusing to generate `py` into"), "{error}");
        assert!(error.contains("alien_sdk"), "{error}");
        assert_eq!(
            fs::read_to_string(package.join("__init__.py")).unwrap(),
            "# hand-written\n"
        );
        assert!(!package.join(".gitignore").exists());
    }

    #[test]
    fn named_python_sdk_generates_into_a_fresh_clone_of_its_committed_gitignore() {
        let project = python_project("alien_sdk._generated");
        let package = project.path().join("out/alien_sdk/_generated");
        fs::create_dir_all(&package).unwrap();
        fs::write(
            package.join(".gitignore"),
            baml_sdkgen_types::GENERATED_GITIGNORE,
        )
        .unwrap();

        let result = generate_project(project.path()).unwrap();

        assert!(matches!(result, crate::ExitCode::Success));
        assert!(package.join("__init__.py").is_file());
    }

    #[test]
    fn a_refused_named_sdk_stops_the_run_before_any_generator_writes() {
        let project = python_project("unused");
        fs::write(
            project.path().join("baml.toml"),
            "[package]\nname = \"test\"\n\n\
             [generator.a]\noutput_type = \"python/pydantic\"\nnaming_convention = \"language\"\noutput_dir = \"out\"\npackage_name = \"first_sdk\"\n\n\
             [generator.b]\noutput_type = \"python/pydantic\"\nnaming_convention = \"language\"\noutput_dir = \"out\"\npackage_name = \"alien_sdk\"\n",
        )
        .unwrap();
        let package = project.path().join("out/alien_sdk");
        fs::create_dir_all(&package).unwrap();
        fs::write(package.join("__init__.py"), "# hand-written\n").unwrap();

        let error = generate_project(project.path()).unwrap_err().to_string();

        assert!(error.contains("refusing to generate `b` into"), "{error}");
        assert!(!project.path().join("out/first_sdk").exists());
    }

    #[test]
    fn named_python_sdk_refuses_hand_written_files_under_an_output_override() {
        let project = python_project("alien_sdk._generated");
        let package = project.path().join("elsewhere/alien_sdk");
        fs::create_dir_all(&package).unwrap();
        fs::write(package.join("__init__.py"), "# hand-written\n").unwrap();

        let error = GenerateArgs {
            telemetry: Default::default(),
            command: None,
            compiler: Default::default(),
            output: Some(package.clone()),
            show_all_skips: false,
        }
        .run_generate(&CommandContext::for_test(Some(project.path())))
        .unwrap_err()
        .to_string();

        assert!(error.contains("refusing to generate `py` into"), "{error}");
        assert_eq!(
            fs::read_to_string(package.join("__init__.py")).unwrap(),
            "# hand-written\n"
        );
        assert!(!package.join(".gitignore").exists());
    }

    #[test]
    fn named_python_sdk_regenerates_its_own_output_and_keeps_the_superseded_default() {
        let project = python_project("alien_sdk._generated");
        // Output of an earlier run without `package_name`.
        let previous = project.path().join("out/baml_sdk");
        baml_sdkgen_types::write_generated_output(
            &previous,
            vec![baml_sdkgen_types::GeneratedOutputFile::new(
                "__init__.py",
                "x = 1\n",
            )],
        )
        .unwrap();

        for _ in 0..2 {
            let result = generate_project(project.path()).unwrap();
            assert!(matches!(result, crate::ExitCode::Success));
        }

        assert!(
            project
                .path()
                .join("out/alien_sdk/_generated/__init__.py")
                .is_file()
        );
        assert_eq!(
            fs::read_to_string(previous.join("__init__.py")).unwrap(),
            "x = 1\n"
        );
    }

    #[test]
    fn embedded_manifest_preserves_project_content_and_appends_owned_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let original = "# keep this byte-for-byte\n[package]\nname = \"test\"";
        fs::write(directory.path().join("baml.toml"), original).unwrap();

        let embedded = build_embedded_baml_toml(directory.path()).unwrap();

        assert!(embedded.starts_with(original));
        let parsed = embedded.parse::<toml_edit::DocumentMut>().unwrap();
        assert_eq!(
            parsed["__baml_codegen"]["metadata_version"].as_integer(),
            Some(1)
        );
        assert_eq!(
            parsed["__baml_codegen"]["toolchain"]["version"].as_str(),
            Some(baml_version::CANONICAL_VERSION)
        );
        assert_eq!(
            parsed["__baml_codegen"]["telemetry"]["recording_level"].as_str(),
            Some("medium")
        );
        assert_eq!(parsed["package"]["name"].as_str(), Some("test"));
        assert_eq!(
            fs::read_to_string(directory.path().join("baml.toml")).unwrap(),
            original
        );
    }

    #[test]
    fn embedded_manifest_rejects_the_reserved_codegen_table() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("baml.toml"),
            "[package]\nname = \"test\"\n\n[__baml_codegen]\nmetadata_version = 99\n",
        )
        .unwrap();

        let error = build_embedded_baml_toml(directory.path()).unwrap_err();

        assert!(error.to_string().contains("uses reserved table"));
    }

    #[test]
    fn embedded_manifest_bakes_the_bridge_policy_independently_of_pack() {
        use btel_settings::artifact::{
            ArtifactTelemetry, InitialFailureAction, InitialFailurePolicy,
        };

        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("baml.toml"),
            r#"
[package]
name = "test"
[pack]
on_initial_telemetry_failure = "abort"
[pack.env_var_names]
BAML_TELEMETRY = "PACK_TELEMETRY"
[bridge]
on_initial_telemetry_failure = "warn"
initial_telemetry_warning_message = "Telemetry unavailable; continuing."
http_bodies = false
[bridge.env_var_names]
BAML_TELEMETRY = "ACME_TELEMETRY"
"#,
        )
        .unwrap();
        let embedded: toml::Value = build_embedded_baml_toml(directory.path())
            .unwrap()
            .parse()
            .unwrap();
        let policy: ArtifactTelemetry = embedded["__baml_codegen"]["telemetry"]
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(
            policy,
            ArtifactTelemetry {
                recording_level_envvar: Some("ACME_TELEMETRY".into()),
                initial_failure: InitialFailurePolicy {
                    action: InitialFailureAction::Warn,
                    warning_message: Some("Telemetry unavailable; continuing.".into()),
                },
                omit_http_bodies: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn add_parser_accepts_every_output_type_name() {
        for &output_type in OutputType::all() {
            assert_eq!(
                parse_add_output_type(output_type.add_name()),
                Ok(output_type)
            );
        }
    }

    #[test]
    fn add_generator_preserves_manifest_and_uses_first_free_client_name() {
        let content = "# keep this comment\n[package]\nname = \"test\"\n\n[generator.client1]\noutput_type = \"rust\"\nnaming_convention = \"preserve-case\"\n";
        let generator = Generator::from(OutputType::PythonPydantic);

        let (updated, name) = add_generator_to_manifest(content, &generator).unwrap();

        assert_eq!(name, "client2");
        assert!(updated.contains("# keep this comment"));
        let manifest = baml_db::manifest::parse(&updated).unwrap();
        let added = manifest.generator["client2"].get_ref();
        assert_eq!(
            added.output_type.as_ref().unwrap().get_ref(),
            "python/pydantic"
        );
        assert_eq!(
            added.naming_convention.as_ref().unwrap().get_ref(),
            "language"
        );
    }

    #[test]
    fn add_go_generator_writes_required_import_path() {
        let mut generator = Generator::from(OutputType::Go);
        generator.sdk_import_path = Some("example.com/test/baml_sdk".to_string());

        let (updated, name) =
            add_generator_to_manifest("[package]\nname = \"test\"\n", &generator).unwrap();

        assert_eq!(name, "client1");
        let manifest = baml_db::manifest::parse(&updated).unwrap();
        let added = manifest.generator["client1"].get_ref();
        assert_eq!(added.output_type.as_ref().unwrap().get_ref(), "go");
        assert_eq!(
            added.naming_convention.as_ref().unwrap().get_ref(),
            "language"
        );
        assert_eq!(
            added.sdk_import_path.as_ref().unwrap().get_ref(),
            "example.com/test/baml_sdk"
        );
    }

    #[test]
    fn add_command_updates_project_manifest() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("baml.toml"),
            "[package]\nname = \"test\"\n",
        )
        .unwrap();

        let result = AddGeneratorArgs {
            output_type: OutputType::TypescriptNode,
            sdk_import_path: None,
        }
        .run(&CommandContext::for_test(Some(directory.path())))
        .unwrap();

        assert!(matches!(result.status, crate::ExitCode::Success));
        let updated = fs::read_to_string(directory.path().join("baml.toml")).unwrap();
        let manifest = baml_db::manifest::parse(&updated).unwrap();
        assert_eq!(
            manifest.generator["client1"]
                .get_ref()
                .output_type
                .as_ref()
                .unwrap()
                .get_ref(),
            "typescript/node"
        );
    }

    #[test]
    fn generator_output_defaults_to_project_root() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("baml.toml"),
            "[package]\nname = \"test\"\n\n[generator.rust]\noutput_type = \"rust\"\nnaming_convention = \"preserve-case\"\n",
        )
        .unwrap();

        let (generators, diagnostics) = discover_generators(directory.path());

        assert!(diagnostics.is_empty());
        assert_eq!(generators.len(), 1);
        assert_eq!(generators[0].output_dir, directory.path().join("baml_sdk"));
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
        let manifest = "[package]\nname = \"test\"\n\n[generator.ts]\noutput_type = \"typescript/node\"\nnaming_convention = \"preserve-case\"\nmax_typed_union_arity = 3\n";
        let (_, diagnostics) = discover_with_manifest(manifest);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(
            format!("{diagnostics:?}").contains("Go-only property"),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn naming_convention_is_validated_for_every_output_type() {
        let manifest = |output_type: OutputType, convention: NamingConvention| {
            let sdk_import_path = if output_type == OutputType::Go {
                "sdk_import_path = \"example.com/test/baml_sdk\"\n"
            } else {
                ""
            };
            format!(
                "[package]\nname = \"test\"\n\n[generator.client]\noutput_type = \"{output_type}\"\nnaming_convention = \"{convention}\"\n{sdk_import_path}"
            )
        };
        for &output_type in OutputType::all() {
            for convention in [NamingConvention::PreserveCase, NamingConvention::Language] {
                let (generators, diagnostics) =
                    discover_with_manifest(&manifest(output_type, convention));
                match output_type.naming_support() {
                    NamingSupport::Only(required) if required != convention => {
                        assert!(generators.is_empty(), "{output_type}");
                        assert_eq!(diagnostics.len(), 1, "{output_type}: {diagnostics:?}");
                        assert_eq!(
                            diagnostics[0].id,
                            baml_db::baml_compiler_diagnostics::DiagnosticId::InvalidGeneratorPropertyValue
                        );
                        assert_eq!(
                            diagnostics[0].message,
                            format!(
                                "generator `client` with `output_type = \"{output_type}\"` requires `naming_convention = \"{required}\"`"
                            )
                        );
                    }
                    NamingSupport::Only(_) | NamingSupport::Either { .. } => {
                        assert_eq!(generators.len(), 1, "{output_type}: {diagnostics:?}");
                        assert_eq!(generators[0].naming_convention, convention);
                        assert!(diagnostics.is_empty(), "{output_type}: {diagnostics:?}");
                    }
                }
            }
        }
    }
}
