#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::File,
    path::{Component, Path, PathBuf},
    str::FromStr,
};

use anyhow::{Context, Result, anyhow};
use baml_db::{
    FileId, Span,
    baml_compiler_diagnostics::{Diagnostic, DiagnosticId, DiagnosticPhase, Severity, render},
};
use clap::Args;
use sdkgen_python_pydantic2::{NamingConvention, OutputType};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use text_size::{TextRange, TextSize};
use toml::Spanned;
use uuid::Uuid;

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
    /// Required for Go so generated packages can import the SDK root and one
    /// another. Other generators leave this unset.
    sdk_import_path: Option<String>,
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
        // Compile-time diagnostics — same shape as run/pack: render the
        // ariadne block after abandoning the spinner so the colored
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

        let mut resolved_generators = Vec::with_capacity(generators.len());
        let mut output_owners = BTreeMap::new();
        for generator in &generators {
            let output_dir = self
                .output
                .clone()
                .unwrap_or_else(|| generator.output_dir.clone());
            std::fs::create_dir_all(&output_dir).with_context(|| {
                format!(
                    "Failed to create output directory: {}",
                    output_dir.display()
                )
            })?;
            let output_dir = std::fs::canonicalize(&output_dir).with_context(|| {
                format!(
                    "Failed to resolve output directory: {}",
                    output_dir.display()
                )
            })?;
            if let Some(previous) =
                output_owners.insert(output_directory_key(&output_dir), &generator.name)
            {
                return Err(anyhow!(
                    "generators `{previous}` and `{}` resolve to the same output directory {}; each generator must own a distinct directory",
                    generator.name,
                    output_dir.display()
                ));
            }
            resolved_generators.push((generator, output_dir));
        }

        let mut total_files = 0;
        for (generator, output_dir) in resolved_generators {
            reporter.spin("Generating", &generator.name);

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
                OutputType::Go => sdkgen_go::to_source_code_with_bytecode(
                    &pool,
                    &baml_bytecode,
                    generator.naming_convention,
                    generator
                        .sdk_import_path
                        .as_deref()
                        .expect("validated Go generator must have sdk_import_path"),
                )
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
                OutputType::CSharp => sdkgen_csharp::try_to_source_code_with_bytecode(
                    &pool,
                    &baml_bytecode,
                    generator.naming_convention,
                )
                .map_err(|error| anyhow!(error))?
                .into_iter()
                .map(|(path, content)| (path, content.into_bytes()))
                .collect(),
            };

            let count = if generator.output_type == OutputType::CSharp {
                write_generated_files(
                    &output_dir,
                    &generated,
                    output_type_name(generator.output_type),
                )?
            } else {
                write_generated_files_in_place(&output_dir, &generated)?
            };

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

const GENERATED_MANIFEST_FILE: &str = ".baml-generated-files.json";
const GENERATED_LOCK_FILE: &str = ".baml-generation-lock";
const GENERATED_STAGING_PREFIX: &str = ".baml-generation-";
const GENERATED_MANIFEST_SCHEMA: u32 = 1;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GeneratedFilesManifest {
    schema: u32,
    generator: String,
    files: Vec<GeneratedFileEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GeneratedFileEntry {
    path: String,
    sha256: String,
}

fn output_type_name(output_type: OutputType) -> &'static str {
    match output_type {
        OutputType::PythonPydantic => "python/pydantic",
        OutputType::PythonPydanticV1 => "python/pydantic/v1",
        OutputType::TypescriptNode => "typescript/node",
        OutputType::TypescriptWeb => "typescript/web",
        OutputType::Go => "go",
        OutputType::Rust => "rust",
        OutputType::Cpp => "cpp",
        OutputType::CSharp => "csharp",
    }
}

fn write_generated_files(
    output_dir: &Path,
    generated: &[(PathBuf, Vec<u8>)],
    generator: &str,
) -> Result<usize> {
    ensure_output_directory(output_dir)?;
    let new_files = normalize_generated_files(generated)?;
    if new_files.is_empty() {
        return Err(anyhow!("C# generator produced no files"));
    }
    let mut generation_lock = GenerationLock::acquire(output_dir)?;
    ensure_no_interrupted_staging(output_dir)?;
    let manifest_path = output_dir.join(GENERATED_MANIFEST_FILE);
    let previous = load_generated_manifest(&manifest_path)?;
    if let Some(previous) = &previous
        && previous.generator != generator
    {
        return Err(anyhow!(
            "generated-file manifest belongs to `{}`, not `{generator}`: {}",
            previous.generator,
            manifest_path.display()
        ));
    }
    let old_files = previous
        .as_ref()
        .map(|manifest| {
            manifest
                .files
                .iter()
                .map(|entry| (entry.path.clone(), entry.sha256.clone()))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();

    for (relative, content) in &new_files {
        preflight_generated_destination(
            output_dir,
            relative,
            old_files.get(relative).map(String::as_str),
            previous.is_some(),
        )?;
        if content.contains(&0) {
            return Err(anyhow!("generated file `{relative}` contains a NUL byte"));
        }
    }
    for (relative, expected_hash) in &old_files {
        if !new_files.contains_key(relative) {
            preflight_owned_file(output_dir, relative, expected_hash)?;
        }
    }

    let staging = output_dir.join(format!("{GENERATED_STAGING_PREFIX}{}", Uuid::new_v4()));
    let staged_new = staging.join("new");
    let staged_backup = staging.join("backup");
    std::fs::create_dir(&staging).with_context(|| {
        format!(
            "Failed to create generation staging directory {}",
            staging.display()
        )
    })?;

    let staged_manifest = staging.join("next-manifest.json");
    let stage_result = (|| -> Result<()> {
        std::fs::create_dir(&staged_new)?;
        let mut manifest_entries = Vec::with_capacity(new_files.len());
        for (relative, content) in &new_files {
            let staged = staged_new.join(Path::new(relative));
            if let Some(parent) = staged.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&staged, content)
                .with_context(|| format!("Failed to stage generated file `{relative}`"))?;
            manifest_entries.push(GeneratedFileEntry {
                path: relative.clone(),
                sha256: sha256_hex(content),
            });
        }
        let next_manifest = GeneratedFilesManifest {
            schema: GENERATED_MANIFEST_SCHEMA,
            generator: generator.to_string(),
            files: manifest_entries,
        };
        let mut manifest_bytes = serde_json::to_vec_pretty(&next_manifest)?;
        manifest_bytes.push(b'\n');
        std::fs::write(&staged_manifest, manifest_bytes)?;
        Ok(())
    })();
    if let Err(error) = stage_result {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error.context("Failed to stage generated output"));
    }

    let mut affected = BTreeSet::new();
    for relative in new_files.keys().chain(old_files.keys()) {
        match std::fs::symlink_metadata(output_dir.join(Path::new(relative))) {
            Ok(_) => {
                affected.insert(relative.clone());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                let _ = std::fs::remove_dir_all(&staging);
                return Err(error.into());
            }
        }
    }

    let mut backed_up = Vec::new();
    let mut installed = Vec::new();
    let mut manifest_backed_up = false;
    let mut manifest_installed = false;
    let transaction = (|| -> Result<()> {
        for relative in &affected {
            let source = output_dir.join(Path::new(relative));
            let backup = staged_backup.join(Path::new(relative));
            if let Some(parent) = backup.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(&source, &backup).with_context(|| {
                format!("Failed to back up generated file {}", source.display())
            })?;
            backed_up.push(relative.clone());
        }

        if manifest_path.exists() {
            std::fs::create_dir_all(&staged_backup)?;
            std::fs::rename(&manifest_path, staged_backup.join(GENERATED_MANIFEST_FILE))?;
            manifest_backed_up = true;
        }

        for relative in new_files.keys() {
            let source = staged_new.join(Path::new(relative));
            let destination = output_dir.join(Path::new(relative));
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(&source, &destination).with_context(|| {
                format!("Failed to install generated file {}", destination.display())
            })?;
            installed.push(relative.clone());
        }

        std::fs::rename(&staged_manifest, &manifest_path)
            .context("Failed to install generated-file manifest")?;
        manifest_installed = true;
        Ok(())
    })();

    if let Err(error) = transaction {
        if manifest_installed {
            let _ = std::fs::remove_file(&manifest_path);
        }
        for relative in installed.iter().rev() {
            let _ = std::fs::remove_file(output_dir.join(Path::new(relative)));
        }
        if manifest_backed_up {
            let _ = std::fs::rename(staged_backup.join(GENERATED_MANIFEST_FILE), &manifest_path);
        }
        for relative in backed_up.iter().rev() {
            let backup = staged_backup.join(Path::new(relative));
            let destination = output_dir.join(Path::new(relative));
            if let Some(parent) = destination.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::rename(backup, destination);
        }
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error.context("Generated output was rolled back"));
    }

    std::fs::remove_dir_all(&staging).with_context(|| {
        format!(
            "Generated output was committed, but the staging directory could not be removed: {}",
            staging.display()
        )
    })?;
    for relative in old_files
        .keys()
        .filter(|path| !new_files.contains_key(*path))
    {
        let _ = remove_empty_generated_parents(output_dir, relative);
    }
    generation_lock.release()?;
    Ok(new_files.len())
}

fn write_generated_files_in_place(
    output_dir: &Path,
    generated: &[(PathBuf, Vec<u8>)],
) -> Result<usize> {
    let mut count = 0;
    for (relative, content) in generated {
        let destination = output_dir.join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&destination, content)
            .with_context(|| format!("Failed to write {}", destination.display()))?;
        count += 1;
    }
    Ok(count)
}

fn normalize_generated_files(generated: &[(PathBuf, Vec<u8>)]) -> Result<BTreeMap<String, &[u8]>> {
    let mut normalized = BTreeMap::new();
    let mut portable_prefixes = BTreeMap::new();
    for (path, content) in generated {
        let relative = normalize_relative_path(path)?;
        if relative.eq_ignore_ascii_case(GENERATED_MANIFEST_FILE)
            || relative.eq_ignore_ascii_case(GENERATED_LOCK_FILE)
            || relative.split('/').next().is_some_and(|segment| {
                segment
                    .to_ascii_lowercase()
                    .starts_with(GENERATED_STAGING_PREFIX)
            })
        {
            return Err(anyhow!(
                "generator output cannot own reserved path `{relative}`"
            ));
        }
        insert_portable_path(&mut portable_prefixes, &relative, "generator produced")?;
        normalized.insert(relative, content.as_slice());
    }
    Ok(normalized)
}

fn load_generated_manifest(path: &Path) -> Result<Option<GeneratedFilesManifest>> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(anyhow!(
            "generated-file manifest is not a regular file: {}",
            path.display()
        ));
    }
    let bytes = std::fs::read(path)?;
    let manifest: GeneratedFilesManifest = serde_json::from_slice(&bytes)
        .with_context(|| format!("Invalid generated-file manifest {}", path.display()))?;
    if manifest.schema != GENERATED_MANIFEST_SCHEMA {
        return Err(anyhow!(
            "unsupported generated-file manifest schema {} in {}",
            manifest.schema,
            path.display()
        ));
    }
    let mut paths = BTreeSet::new();
    let mut portable_prefixes = BTreeMap::new();
    for entry in &manifest.files {
        let normalized = normalize_relative_path(Path::new(&entry.path))?;
        if normalized != entry.path
            || !paths.insert(entry.path.clone())
            || entry.sha256.len() != 64
            || !entry
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(anyhow!(
                "invalid generated-file entry `{}` in {}",
                entry.path,
                path.display()
            ));
        }
        insert_portable_path(
            &mut portable_prefixes,
            &entry.path,
            "generated-file manifest contains",
        )?;
    }
    Ok(Some(manifest))
}

fn preflight_generated_destination(
    output_dir: &Path,
    relative: &str,
    expected_hash: Option<&str>,
    has_manifest: bool,
) -> Result<()> {
    ensure_safe_ancestors(output_dir, relative)?;
    let destination = output_dir.join(Path::new(relative));
    let metadata = match std::fs::symlink_metadata(&destination) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(anyhow!(
            "generated destination is not a regular file: {}",
            destination.display()
        ));
    }
    if let Some(expected_hash) = expected_hash {
        verify_owned_hash(&destination, expected_hash)?;
        return Ok(());
    }
    if has_manifest || !has_generated_banner(&destination)? {
        return Err(anyhow!(
            "refusing to overwrite user-owned file {}",
            destination.display()
        ));
    }
    Ok(())
}

fn preflight_owned_file(output_dir: &Path, relative: &str, expected_hash: &str) -> Result<()> {
    ensure_safe_ancestors(output_dir, relative)?;
    let destination = output_dir.join(Path::new(relative));
    let metadata = match std::fs::symlink_metadata(&destination) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(anyhow!(
            "owned generated path is not a regular file: {}",
            destination.display()
        ));
    }
    verify_owned_hash(&destination, expected_hash)
}

fn verify_owned_hash(path: &Path, expected_hash: &str) -> Result<()> {
    let actual = sha256_hex(&std::fs::read(path)?);
    if actual != expected_hash {
        return Err(anyhow!(
            "generated file {} was modified; refusing to overwrite or delete it",
            path.display()
        ));
    }
    Ok(())
}

fn has_generated_banner(path: &Path) -> Result<bool> {
    let bytes = std::fs::read(path)?;
    Ok(bytes.starts_with(b"// This file was generated by BAML")
        || bytes.starts_with(b"# This file was generated by BAML"))
}

fn ensure_safe_ancestors(output_dir: &Path, relative: &str) -> Result<()> {
    let relative = Path::new(relative);
    let mut current = output_dir.to_path_buf();
    if let Some(parent) = relative.parent() {
        for component in parent.components() {
            current.push(component.as_os_str());
            match std::fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(anyhow!(
                        "generated path traverses symbolic link {}",
                        current.display()
                    ));
                }
                Ok(metadata) if !metadata.is_dir() => {
                    return Err(anyhow!(
                        "generated path parent is not a directory: {}",
                        current.display()
                    ));
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(())
}

fn normalize_relative_path(path: &Path) -> Result<String> {
    let raw = path
        .to_str()
        .ok_or_else(|| anyhow!("generated path is not UTF-8: {}", path.display()))?;
    let mut segments = Vec::new();
    for component in path.components() {
        let Component::Normal(segment) = component else {
            return Err(anyhow!(
                "generated path must be relative and normalized: {}",
                path.display()
            ));
        };
        let segment = segment
            .to_str()
            .ok_or_else(|| anyhow!("generated path is not UTF-8: {}", path.display()))?;
        if segment.is_empty()
            || segment
                .chars()
                .any(|character| character == '\\' || character.is_control())
            || segment.contains(['<', '>', ':', '"', '|', '?', '*'])
            || segment.ends_with(['.', ' '])
            || is_windows_device_name(segment)
        {
            return Err(anyhow!(
                "generated path contains an unsafe component `{segment}`"
            ));
        }
        segments.push(segment);
    }
    if segments.is_empty() {
        return Err(anyhow!("generated path cannot be empty"));
    }
    let normalized = segments.join("/");
    #[cfg(windows)]
    let portable_raw = raw.replace('\\', "/");
    #[cfg(not(windows))]
    let portable_raw = raw.to_string();
    if portable_raw != normalized {
        return Err(anyhow!(
            "generated path must be relative and normalized: {}",
            path.display()
        ));
    }
    Ok(normalized)
}

fn insert_portable_path(
    prefixes: &mut BTreeMap<String, String>,
    relative: &str,
    subject: &str,
) -> Result<()> {
    let mut prefix = String::new();
    for segment in relative.split('/') {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(segment);
        let folded = prefix.to_lowercase();
        if let Some(previous) = prefixes.get(&folded)
            && previous != &prefix
        {
            return Err(anyhow!(
                "{subject} case-insensitively colliding paths `{previous}` and `{prefix}`"
            ));
        }
        prefixes.insert(folded, prefix.clone());
    }
    Ok(())
}

fn is_windows_device_name(segment: &str) -> bool {
    let stem = segment
        .split('.')
        .next()
        .unwrap_or(segment)
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem.strip_prefix("COM").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
        || stem.strip_prefix("LPT").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
}

fn remove_empty_generated_parents(output_dir: &Path, relative: &str) -> Result<()> {
    let mut parent = output_dir.join(Path::new(relative));
    while parent.pop() && parent != output_dir {
        match std::fs::remove_dir(&parent) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::DirectoryNotEmpty | std::io::ErrorKind::NotFound
                ) =>
            {
                break;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn ensure_output_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("Failed to inspect output directory {}", path.display()))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(anyhow!(
            "generated output root is not a regular directory: {}",
            path.display()
        ));
    }
    Ok(())
}

struct GenerationLock {
    path: PathBuf,
    file: Option<File>,
}

impl GenerationLock {
    fn acquire(output_dir: &Path) -> Result<Self> {
        let path = output_dir.join(GENERATED_LOCK_FILE);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        let file = match options.open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(anyhow!(
                    "another or interrupted C# generation owns {}; remove it only after confirming no generator is running",
                    path.display()
                ));
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("Failed to lock generated output {}", path.display())
                });
            }
        };
        Ok(Self {
            path,
            file: Some(file),
        })
    }

    fn release(&mut self) -> Result<()> {
        drop(self.file.take());
        std::fs::remove_file(&self.path)
            .with_context(|| format!("Failed to release generation lock {}", self.path.display()))
    }
}

impl Drop for GenerationLock {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = std::fs::remove_file(&self.path);
    }
}

fn ensure_no_interrupted_staging(output_dir: &Path) -> Result<()> {
    for entry in std::fs::read_dir(output_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name
            .to_ascii_lowercase()
            .starts_with(GENERATED_STAGING_PREFIX)
            && !name.eq_ignore_ascii_case(GENERATED_LOCK_FILE)
        {
            return Err(anyhow!(
                "interrupted C# generation staging state exists at {}; inspect it before removing it",
                entry.path().display()
            ));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn output_directory_key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

#[cfg(not(windows))]
fn output_directory_key(path: &Path) -> PathBuf {
    path.to_path_buf()
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
            r#"one of: "python/pydantic", "python/pydantic/v1", "typescript/node", "typescript/web", "go", "rust", "cpp", "csharp""#,
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
mod generated_output_tests {
    use super::*;

    const BANNER: &str = "// This file was generated by BAML. Do not edit it directly.\n";

    fn generated(entries: &[(&str, &str)]) -> Vec<(PathBuf, Vec<u8>)> {
        entries
            .iter()
            .map(|(path, content)| (PathBuf::from(path), content.as_bytes().to_vec()))
            .collect()
    }

    fn create_output() -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().expect("create temporary directory");
        let output = temp.path().join("baml_sdk");
        std::fs::create_dir(&output).expect("create output directory");
        (temp, output)
    }

    fn manifest(output: &Path) -> GeneratedFilesManifest {
        load_generated_manifest(&output.join(GENERATED_MANIFEST_FILE))
            .expect("read generated manifest")
            .expect("manifest exists")
    }

    fn assert_no_staging_directories(output: &Path) {
        let staging = std::fs::read_dir(output)
            .expect("read output directory")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".baml-generation-"))
            .collect::<Vec<_>>();
        assert!(
            staging.is_empty(),
            "staging directories remain: {staging:?}"
        );
    }

    #[test]
    fn generation_is_deterministic_and_removes_only_stale_owned_files() {
        let (_temp, output) = create_output();
        std::fs::write(output.join("User.cs"), "public class User {}\n").expect("write user file");

        let first = generated(&[
            ("Root.g.cs", &format!("{BANNER}root-v1\n")),
            ("Nested/Stale.g.cs", &format!("{BANNER}stale\n")),
        ]);
        assert_eq!(write_generated_files(&output, &first, "csharp").unwrap(), 2);
        let first_manifest = std::fs::read(output.join(GENERATED_MANIFEST_FILE)).unwrap();
        assert_eq!(
            manifest(&output)
                .files
                .into_iter()
                .map(|entry| entry.path)
                .collect::<Vec<_>>(),
            ["Nested/Stale.g.cs", "Root.g.cs"]
        );

        assert_eq!(write_generated_files(&output, &first, "csharp").unwrap(), 2);
        assert_eq!(
            std::fs::read(output.join(GENERATED_MANIFEST_FILE)).unwrap(),
            first_manifest
        );

        let second = generated(&[
            ("Root.g.cs", &format!("{BANNER}root-v2\n")),
            ("Next.g.cs", &format!("{BANNER}next\n")),
        ]);
        assert_eq!(
            write_generated_files(&output, &second, "csharp").unwrap(),
            2
        );
        assert_eq!(
            std::fs::read_to_string(output.join("Root.g.cs")).unwrap(),
            format!("{BANNER}root-v2\n")
        );
        assert!(output.join("Next.g.cs").is_file());
        assert!(!output.join("Nested/Stale.g.cs").exists());
        assert!(!output.join("Nested").exists());
        assert_eq!(
            std::fs::read_to_string(output.join("User.cs")).unwrap(),
            "public class User {}\n"
        );
        assert_no_staging_directories(&output);
    }

    #[test]
    fn modified_owned_file_aborts_before_any_output_changes() {
        let (_temp, output) = create_output();
        let first = generated(&[
            ("A.g.cs", &format!("{BANNER}a-v1\n")),
            ("B.g.cs", &format!("{BANNER}b-v1\n")),
        ]);
        write_generated_files(&output, &first, "csharp").unwrap();
        let manifest_before = std::fs::read(output.join(GENERATED_MANIFEST_FILE)).unwrap();
        std::fs::write(output.join("A.g.cs"), "user edit\n").unwrap();

        let second = generated(&[
            ("A.g.cs", &format!("{BANNER}a-v2\n")),
            ("B.g.cs", &format!("{BANNER}b-v2\n")),
        ]);
        let error = write_generated_files(&output, &second, "csharp").unwrap_err();
        assert!(error.to_string().contains("was modified"));
        assert_eq!(
            std::fs::read_to_string(output.join("A.g.cs")).unwrap(),
            "user edit\n"
        );
        assert_eq!(
            std::fs::read_to_string(output.join("B.g.cs")).unwrap(),
            format!("{BANNER}b-v1\n")
        );
        assert_eq!(
            std::fs::read(output.join(GENERATED_MANIFEST_FILE)).unwrap(),
            manifest_before
        );
        assert_no_staging_directories(&output);
    }

    #[test]
    fn user_owned_collision_without_manifest_is_never_adopted() {
        let (_temp, output) = create_output();
        std::fs::write(output.join("Types.g.cs"), "user owned\n").unwrap();
        let files = generated(&[("Types.g.cs", &format!("{BANNER}generated\n"))]);

        let error = write_generated_files(&output, &files, "csharp").unwrap_err();
        assert!(error.to_string().contains("user-owned file"));
        assert_eq!(
            std::fs::read_to_string(output.join("Types.g.cs")).unwrap(),
            "user owned\n"
        );
        assert!(!output.join(GENERATED_MANIFEST_FILE).exists());
        assert_no_staging_directories(&output);
    }

    #[test]
    fn legacy_banner_file_can_be_adopted_without_a_manifest() {
        let (_temp, output) = create_output();
        std::fs::write(output.join("Functions.g.cs"), format!("{BANNER}legacy\n")).unwrap();
        let files = generated(&[("Functions.g.cs", &format!("{BANNER}current\n"))]);

        assert_eq!(write_generated_files(&output, &files, "csharp").unwrap(), 1);
        assert_eq!(
            std::fs::read_to_string(output.join("Functions.g.cs")).unwrap(),
            format!("{BANNER}current\n")
        );
        assert_eq!(manifest(&output).files[0].path, "Functions.g.cs");
        assert_no_staging_directories(&output);
    }

    #[test]
    fn unsafe_and_case_colliding_paths_fail_without_writes() {
        let (temp, output) = create_output();
        let escaping = generated(&[("../Escape.g.cs", &format!("{BANNER}escape\n"))]);
        assert!(
            write_generated_files(&output, &escaping, "csharp")
                .unwrap_err()
                .to_string()
                .contains("relative and normalized")
        );
        assert!(!temp.path().join("Escape.g.cs").exists());

        let colliding = generated(&[
            ("Types.g.cs", &format!("{BANNER}one\n")),
            ("types.g.cs", &format!("{BANNER}two\n")),
        ]);
        assert!(
            write_generated_files(&output, &colliding, "csharp")
                .unwrap_err()
                .to_string()
                .contains("case-insensitively colliding")
        );

        let parent_colliding = generated(&[
            ("Models/One.g.cs", &format!("{BANNER}one\n")),
            ("models/Two.g.cs", &format!("{BANNER}two\n")),
        ]);
        assert!(
            write_generated_files(&output, &parent_colliding, "csharp")
                .unwrap_err()
                .to_string()
                .contains("case-insensitively colliding")
        );

        let reserved = generated(&[(".BAML-GENERATED-FILES.JSON", &format!("{BANNER}reserved\n"))]);
        assert!(
            write_generated_files(&output, &reserved, "csharp")
                .unwrap_err()
                .to_string()
                .contains("reserved path")
        );
        assert!(std::fs::read_dir(&output).unwrap().next().is_none());
    }

    #[test]
    fn corrupt_manifest_fails_closed() {
        let (_temp, output) = create_output();
        std::fs::write(output.join(GENERATED_MANIFEST_FILE), b"{not-json\n").unwrap();
        std::fs::write(output.join("Functions.g.cs"), "existing\n").unwrap();
        let files = generated(&[("Functions.g.cs", &format!("{BANNER}new\n"))]);

        let error = write_generated_files(&output, &files, "csharp").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Invalid generated-file manifest")
        );
        assert_eq!(
            std::fs::read(output.join(GENERATED_MANIFEST_FILE)).unwrap(),
            b"{not-json\n"
        );
        assert_eq!(
            std::fs::read_to_string(output.join("Functions.g.cs")).unwrap(),
            "existing\n"
        );
        assert_no_staging_directories(&output);
    }

    #[test]
    fn wrong_generator_manifest_and_interrupted_state_fail_closed() {
        let (_temp, output) = create_output();
        let files = generated(&[("Functions.g.cs", &format!("{BANNER}current\n"))]);
        write_generated_files(&output, &files, "csharp").unwrap();

        let manifest_path = output.join(GENERATED_MANIFEST_FILE);
        let mut wrong_owner: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        wrong_owner["generator"] = serde_json::Value::String("typescript/node".to_string());
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&wrong_owner).unwrap(),
        )
        .unwrap();
        let error = write_generated_files(&output, &files, "csharp").unwrap_err();
        assert!(error.to_string().contains("belongs to `typescript/node`"));
        assert_no_staging_directories(&output);

        wrong_owner["generator"] = serde_json::Value::String("csharp".to_string());
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&wrong_owner).unwrap(),
        )
        .unwrap();
        let interrupted = output.join(format!("{GENERATED_STAGING_PREFIX}interrupted"));
        std::fs::create_dir(&interrupted).unwrap();
        let manifest_before = std::fs::read(&manifest_path).unwrap();
        let generated_before = std::fs::read(output.join("Functions.g.cs")).unwrap();
        let error = write_generated_files(&output, &files, "csharp").unwrap_err();
        assert!(error.to_string().contains("interrupted C# generation"));
        assert_eq!(std::fs::read(&manifest_path).unwrap(), manifest_before);
        assert_eq!(
            std::fs::read(output.join("Functions.g.cs")).unwrap(),
            generated_before
        );
        assert!(!output.join(GENERATED_LOCK_FILE).exists());
    }

    #[test]
    fn existing_generation_lock_is_not_stolen() {
        let (_temp, output) = create_output();
        std::fs::write(output.join(GENERATED_LOCK_FILE), "active or interrupted\n").unwrap();
        let files = generated(&[("Functions.g.cs", &format!("{BANNER}current\n"))]);

        let error = write_generated_files(&output, &files, "csharp").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("another or interrupted C# generation")
        );
        assert_eq!(
            std::fs::read_to_string(output.join(GENERATED_LOCK_FILE)).unwrap(),
            "active or interrupted\n"
        );
        assert!(!output.join(GENERATED_MANIFEST_FILE).exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_ancestor_is_rejected() {
        use std::os::unix::fs::symlink;

        let (temp, output) = create_output();
        let outside = temp.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        symlink(&outside, output.join("Linked")).unwrap();
        let files = generated(&[("Linked/Escape.g.cs", &format!("{BANNER}escape\n"))]);

        let error = write_generated_files(&output, &files, "csharp").unwrap_err();
        assert!(error.to_string().contains("traverses symbolic link"));
        assert!(!outside.join("Escape.g.cs").exists());
        assert!(!output.join(GENERATED_MANIFEST_FILE).exists());
        assert_no_staging_directories(&output);
    }
}

#[cfg(test)]
mod tests {
    use super::is_valid_go_import_path;

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
}
