// `baml pack` — compile any `baml run` target (except expression mode)
// into a single self-contained executable.
//
// Target resolution mirrors `baml run`'s shape minus two things:
//   - `-e` is not packageable (no persistent target to bake in).
//   - `[scripts]` are not packageable — scripts are a dev-time dispatch
//     mechanism, not an entry-point concept.
//
// The output is the host binary (baml-pack-host) with a `PackEnvelope`
// (bitcode-serialized) appended in an OS-native section. At runtime the
// host extracts the envelope, initializes the engine, and invokes the
// baked-in target with an auto-CLI parser driven by its signature.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{
    collections::HashMap,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow};
use baml_db::{baml_compiler_diagnostics::Severity, baml_compiler2_emit};
use baml_exec::{OutputFormat, PackEnvelope, validate_help_param};
use baml_project::ProjectDatabase;
use bex_engine::BexEngine;
use bex_vm_types::types::Program;
use clap::Args;
use sha2::{Digest, Sha256};
use sys_native::SysOpsExt;

use crate::{
    commands::release_version, project_load::load_project_from_reporting, reporter::Reporter,
};

/// Section name where the packed envelope lives inside the host binary.
/// Kept in sync with `baml_pack_host::SECTION_NAME`.
const PACK_SECTION_NAME: &str = "baml_pack";

/// `baml pack` — compile a target into a standalone executable.
#[derive(Args, Clone, Debug)]
pub struct PackArgs {
    /// Target: namespace name to pack its `main`, or a path to a `.baml`
    /// file for hermetic packaging. If omitted, packs the root `main`.
    #[arg(value_name = "TARGET")]
    pub target: Option<String>,

    /// Pack a specific function as the entry point (e.g. `llm.Summarize`).
    /// Replaces the positional target.
    #[arg(long)]
    pub function: Option<String>,

    /// Output path for the packaged executable.
    /// Defaults to `./<target-name>`.
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Target triple for the packaged executable.
    /// Defaults to the host platform. When this differs from the host,
    /// `baml pack` downloads the matching pack host from GitHub release
    /// artifacts.
    #[arg(long = "target", value_name = "TRIPLE")]
    pub target_triple: Option<String>,

    /// Output format baked into the binary. Defaults to `json`; packaged
    /// binaries are production tools whose primary reader is another
    /// program. Use `debug` for human-readable output.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output_format: OutputFormat,

    /// Project root directory. Ignored for hermetic `.baml` targets.
    #[arg(long, default_value = ".")]
    pub from: PathBuf,

    /// `-e/--expression` is recognized only so we can reject it with a
    /// targeted message — `baml pack` doesn't support expression mode
    /// (BEP-027 §"Expression mode is not packageable"). Without this
    /// flag declared, `baml pack -e '...'` falls through to clap's
    /// positional handling and surfaces a confusing parse error.
    #[arg(short = 'e', long = "expression", value_name = "EXPR")]
    pub expression: Option<String>,
}

/// Resolved entry point: everything needed to build a `PackEnvelope`.
#[derive(Debug)]
struct ResolvedPackTarget {
    qualified_name: String,
    identifier: String,
    default_basename: String,
}

impl PackArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let reporter = Reporter::new();
        self.run_with_reporter(&reporter)
    }

    fn run_with_reporter(&self, reporter: &Reporter) -> Result<crate::ExitCode> {
        if self.expression.is_some() {
            return Err(anyhow!(
                "expression mode (`-e` / `--expression`) is not packageable; \
                 `baml pack` requires a function or `.baml` file target. \
                 Pass `--function <name>` or a positional target instead.\n\
                 See BEP-027 §\"Expression mode is not packageable\"."
            ));
        }
        let (db, program, needs_format_hint) = self.load_and_compile(reporter)?;
        let _ = db;
        // Mirror `baml run`'s format advisory: if any source file
        // round-trips through `baml fmt` differently, surface a
        // non-fatal warning so users learn to keep packaged
        // projects formatted. Pack is a release-shaped operation,
        // so a clean tree matters at least as much as for run.
        if needs_format_hint {
            reporter.warning(crate::run_command::FORMAT_HINT);
        }

        // Signature info for target resolution / reserved `help` check.
        let engine = BexEngine::new(
            program.clone(),
            Arc::new(sys_native::SysOps::native()),
            None,
            vec![],
        )
        .map_err(|e| anyhow!("Failed to initialize engine for resolution: {e:?}"))?;

        let resolved = self.resolve_target(&engine)?;
        validate_help_param(&engine, &resolved.qualified_name)?;
        let display_name = resolved
            .qualified_name
            .strip_prefix("user.")
            .unwrap_or(&resolved.qualified_name)
            .to_string();
        reporter.spin("Packaging", &display_name);

        let envelope = PackEnvelope {
            program,
            target_name: resolved.qualified_name.clone(),
            target_identifier: resolved.identifier.clone(),
            output_format: self.output_format,
        };
        let serialized = bitcode::serialize(&envelope)
            .map_err(|e| anyhow!("Failed to serialize pack envelope: {e}"))?;

        let target_triple = self.resolved_target_triple()?;
        let host_bytes = read_host_binary(target_triple, reporter)?;
        let output_path = self
            .output
            .clone()
            .unwrap_or_else(|| default_output_path(&resolved.default_basename, target_triple));

        let mut output_file = std::fs::File::create(&output_path)
            .with_context(|| format!("Failed to create {}", output_path.display()))?;
        write_executable(&host_bytes, &serialized, &mut output_file, target_triple)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&output_path, std::fs::Permissions::from_mode(0o755))
                .with_context(|| {
                    format!("Failed to set permissions on {}", output_path.display())
                })?;
        }

        // Cargo's `Finished` is `<artifact-or-profile> [metadata]
        // in <elapsed>`. Mirror that: the artifact location is the
        // primary fact, with target name + triple in brackets so
        // the user can see what was packed and for which platform
        // without parsing an ambiguous arrow.
        reporter.finish(
            "Finished",
            format!(
                "{} [{}, {}]",
                output_path.display(),
                display_name,
                target_triple,
            ),
        );
        Ok(crate::ExitCode::Success)
    }

    fn resolved_target_triple(&self) -> Result<&str> {
        match self.target_triple.as_deref() {
            Some(target) => validate_release_target_triple(target),
            None => release_host_target_triple(),
        }
    }

    fn load_and_compile(&self, reporter: &Reporter) -> Result<(ProjectDatabase, Program, bool)> {
        // Per-file `Loading <path>` lines come from
        // `load_project_from_reporting` (cargo-shaped per-unit
        // progress) — no aggregate `Loading <project>` needed.
        let (db, needs_format_hint) = match self.target.as_deref() {
            Some(t) if t.ends_with(".baml") => {
                reporter.spin("Loading", t);
                self.load_standalone(t)?
            }
            _ => self.load_project(reporter)?,
        };

        // File count is the meaningful unit here — `self.from` is
        // usually `.` and reads as noise. Matches the `Checking N
        // file(s)` shape that `baml run` uses.
        let file_count = db.get_source_files().len();
        reporter.spin("Checking", format!("{file_count} file(s)"));
        check_diagnostics(&db, "Cannot pack: compilation errors found", reporter)?;

        reporter.spin("Compiling", format!("{file_count} file(s)"));
        let program = baml_compiler2_emit::generate_project_bytecode(
            &db,
            &baml_compiler2_emit::CompileOptions {
                emit_test_cases: false,
            },
        )
        .map_err(|e| anyhow!("Compilation failed: {e:?}"))?;

        Ok((db, program, needs_format_hint))
    }

    fn load_project(&self, reporter: &Reporter) -> Result<(ProjectDatabase, bool)> {
        let (db, from, baml_files) = load_project_from_reporting(&self.from, reporter)?;
        if baml_files.is_empty() {
            anyhow::bail!("No .baml files found in {}", from.display());
        }
        // Mirror `baml run`'s per-file format check: probe each source
        // through the formatter and emit a single advisory if any file
        // would change. Cheap enough at compile time (we already read
        // each file from disk during discovery) and matches the
        // warning surfaces `baml run` already provides.
        let needs_format_hint = baml_files.iter().any(|path| {
            std::fs::read_to_string(path)
                .map(|source| crate::run_command::source_needs_format_hint(&source))
                .unwrap_or(false)
        });
        Ok((db, needs_format_hint))
    }

    fn load_standalone(&self, file_path: &str) -> Result<(ProjectDatabase, bool)> {
        let canonical = std::fs::canonicalize(Path::new(file_path))
            .with_context(|| format!("File not found: {file_path}"))?;
        let content = std::fs::read_to_string(&canonical)
            .with_context(|| format!("Failed to read {}", canonical.display()))?;
        let needs_format_hint = crate::run_command::source_needs_format_hint(&content);
        let parent = canonical.parent().unwrap_or_else(|| Path::new("."));
        let mut db = ProjectDatabase::new();
        db.set_project_root(parent);
        db.add_or_update_file(&canonical, &content);
        Ok((db, needs_format_hint))
    }

    /// Resolve the pack target to `(qualified_name, identifier, default_basename)`.
    fn resolve_target(&self, engine: &BexEngine) -> Result<ResolvedPackTarget> {
        if self.function.is_some() && self.target.is_some() {
            anyhow::bail!("`--function` and a positional target are mutually exclusive.");
        }

        if let Some(func) = &self.function {
            if !engine.function_exists(func) {
                let suggestions = function_suggestions(engine, func);
                if suggestions.is_empty() {
                    anyhow::bail!(
                        "Function `{func}` not found. Use `baml run --list` to see \
                         available targets."
                    );
                } else {
                    anyhow::bail!(
                        "Function `{func}` not found. Did you mean one of:\n{}",
                        suggestions
                            .iter()
                            .map(|s| format!("  - {s}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    );
                }
            }
            // BEP-027 §"`baml.argv` in packaged binaries": `argv[1]` is
            // the *qualified function name* — i.e. the display form, with
            // the engine's `user.` prefix stripped. Use the canonical
            // form so `--function user.llm.X` and `--function llm.X` both
            // produce `argv[1] = "llm.X"`.
            let qualified_name = canonicalize_function_name(engine, func);
            let display = qualified_name
                .strip_prefix("user.")
                .unwrap_or(&qualified_name)
                .to_string();
            let basename = display.rsplit('.').next().unwrap_or(&display).to_string();
            return Ok(ResolvedPackTarget {
                qualified_name,
                identifier: display,
                default_basename: basename,
            });
        }

        match self.target.as_deref() {
            None => {
                if !engine.function_exists("main") {
                    anyhow::bail!(
                        "No `main` function found in the root namespace. \
                         Use `--function <name>` to pack a specific function."
                    );
                }
                Ok(ResolvedPackTarget {
                    qualified_name: canonicalize_function_name(engine, "main"),
                    identifier: "main".to_string(),
                    default_basename: "main".to_string(),
                })
            }
            Some(target) if target.ends_with(".baml") => {
                if !engine.function_exists("main") {
                    anyhow::bail!(
                        "Standalone file `{target}` has no `main` function. \
                         Use `--function <name>` to pack a specific function."
                    );
                }
                let basename = Path::new(target)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| target.to_string());
                let stem = Path::new(target)
                    .file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| basename.clone());
                Ok(ResolvedPackTarget {
                    qualified_name: canonicalize_function_name(engine, "main"),
                    identifier: basename,
                    default_basename: stem,
                })
            }
            Some(target) => {
                let ns_main = format!("{target}.main");
                if !engine.function_exists(&ns_main) {
                    anyhow::bail!(
                        "No namespace `{target}` with a `main` function. \
                         `baml pack` does not support `[scripts]`; use the \
                         resolved `--function` form directly."
                    );
                }
                Ok(ResolvedPackTarget {
                    qualified_name: canonicalize_function_name(engine, &ns_main),
                    identifier: target.to_string(),
                    default_basename: target.to_string(),
                })
            }
        }
    }
}

/// Collect diagnostics; render errors to stderr and bail with `ctx`.
///
/// When `reporter` has an active spinner, abandon it before printing so
/// the multi-line ariadne block lands cleanly instead of getting
/// interleaved with the tick character.
fn check_diagnostics(db: &ProjectDatabase, ctx: &str, reporter: &Reporter) -> Result<()> {
    use baml_db::baml_compiler_diagnostics::render;

    let project = db
        .get_project()
        .ok_or_else(|| anyhow!("No project context"))?;
    let source_files = db.get_source_files();
    let diagnostics = baml_project::collect_diagnostics(db, project, &source_files);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    if errors.is_empty() {
        return Ok(());
    }
    let mut sources = HashMap::new();
    let mut file_paths = HashMap::new();
    for sf in &source_files {
        let file_id = sf.file_id(db);
        sources.insert(file_id, sf.text(db).to_string());
        file_paths.insert(file_id, sf.path(db));
    }
    let rendered = render::render_diagnostics(
        &errors.iter().copied().cloned().collect::<Vec<_>>(),
        &sources,
        &file_paths,
        &render::RenderConfig::cli_auto(),
    );
    reporter.abandon();
    eprintln!("{rendered}");
    anyhow::bail!("{ctx}");
}

/// Return the qualified name the engine prefers when both `foo` and
/// `user.foo` resolve to the same function.
/// Suggest user functions whose name is similar to `query`. Ranked by
/// substring containment first, then jaro-winkler similarity. Returns up
/// to 5 display names, sorted.
fn function_suggestions(engine: &BexEngine, query: &str) -> Vec<String> {
    let mut hits: Vec<String> = engine
        .user_functions()
        .into_iter()
        .map(|f| f.display_name)
        .filter(|name| {
            name.contains(query)
                || query.contains(name.as_str())
                || strsim::jaro_winkler(name, query) > 0.7
        })
        .collect();
    hits.sort();
    hits.dedup();
    hits.truncate(5);
    hits
}

fn canonicalize_function_name(engine: &BexEngine, name: &str) -> String {
    engine
        .find_user_function(name)
        .map(|info| info.qualified_name)
        .unwrap_or_else(|| name.to_string())
}

fn read_host_binary(target_triple: &str, reporter: &Reporter) -> Result<Vec<u8>> {
    let exe = std::env::current_exe().context("Failed to locate current executable")?;
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow!("Cannot determine directory of current executable"))?;
    let host_name = host_binary_name(target_triple);
    let host_path = dir.join(&host_name);
    if target_triple == release_host_target_triple()? && host_path.exists() {
        return std::fs::read(&host_path)
            .with_context(|| format!("Failed to read {}", host_path.display()));
    }

    // Cargo emits `Downloading <crate>` when it has to fetch a missing
    // dependency from crates.io; same shape here so users see *why*
    // pack just paused. Routed through `reporter.spin` so the line
    // persists to scrollback above the still-ticking spinner.
    reporter.spin("Downloading", &host_name);
    download_host_binary_from_release(target_triple, &host_name)
}

fn host_binary_name(target_triple: &str) -> String {
    if target_triple.ends_with("windows-msvc") {
        "baml-pack-host.exe".to_string()
    } else {
        "baml-pack-host".to_string()
    }
}

fn download_host_binary_from_release(target: &str, host_name: &str) -> Result<Vec<u8>> {
    let version = release_version_for_download();
    let url = release_archive_url(&version, target);

    let archive_bytes = download_release_asset(&url)?;
    verify_release_archive_checksum(&archive_bytes, &url)?;

    extract_host_from_archive(&archive_bytes, &url, host_name)
}

fn download_release_asset(url: &str) -> Result<Vec<u8>> {
    let response = reqwest::blocking::get(url)
        .with_context(|| format!("Failed to download BAML release asset from {url}"))?
        .error_for_status()
        .with_context(|| format!("Failed to download BAML release asset from {url}"))?;
    let bytes = response
        .bytes()
        .with_context(|| format!("Failed to read BAML release asset from {url}"))?;
    Ok(bytes.to_vec())
}

fn verify_release_archive_checksum(archive_bytes: &[u8], archive_url: &str) -> Result<()> {
    let checksum_url = release_archive_checksum_url(archive_url);
    let checksum_text = download_release_asset(&checksum_url)?;
    let checksum_text = std::str::from_utf8(&checksum_text)
        .with_context(|| format!("Checksum asset {checksum_url} was not valid UTF-8"))?;
    verify_release_archive_checksum_text(archive_bytes, archive_url, checksum_text).with_context(
        || format!("Failed to verify BAML release archive checksum from {checksum_url}"),
    )
}

fn verify_release_archive_checksum_text(
    archive_bytes: &[u8],
    archive_url: &str,
    checksum_text: &str,
) -> Result<()> {
    let archive_name = archive_url
        .rsplit('/')
        .next()
        .ok_or_else(|| anyhow!("Archive URL did not contain a file name: {archive_url}"))?;
    let expected = parse_release_checksum(checksum_text, archive_name)?;
    let actual = format!("{:x}", Sha256::digest(archive_bytes));
    if actual != expected {
        anyhow::bail!(
            "Checksum mismatch for BAML release archive {archive_name}: expected {expected}, got {actual}"
        );
    }
    Ok(())
}

fn release_archive_checksum_url(archive_url: &str) -> String {
    format!("{archive_url}.sha256")
}

fn parse_release_checksum(checksum_text: &str, archive_name: &str) -> Result<String> {
    for line in checksum_text.lines() {
        let mut parts = line.split_whitespace();
        let Some(hash) = parts.next() else {
            continue;
        };
        let Some(name) = parts.next() else {
            continue;
        };
        if name == archive_name {
            return validate_sha256(hash);
        }
    }
    anyhow::bail!("Checksum file did not contain an entry for {archive_name}")
}

fn validate_sha256(hash: &str) -> Result<String> {
    if hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(hash.to_ascii_lowercase())
    } else {
        anyhow::bail!("Invalid SHA-256 checksum `{hash}`")
    }
}

fn release_version_for_download() -> String {
    std::env::var("BAML_PACK_HOST_RELEASE_VERSION")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| release_version().to_string())
}

fn release_archive_url(version: &str, target: &str) -> String {
    if let Ok(base_url) = std::env::var("BAML_PACK_HOST_RELEASE_BASE_URL") {
        let base = base_url.trim_end_matches('/');
        return format!("{base}/{}", release_archive_filename(version, target));
    }

    let repo = std::env::var("BAML_PACK_HOST_RELEASE_REPO")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "BoundaryML/baml".to_string());
    release_archive_url_for_repo(version, target, &repo)
}

fn release_archive_url_for_repo(version: &str, target: &str, repo: &str) -> String {
    format!(
        "https://github.com/{repo}/releases/download/baml-language-{version}/{}",
        release_archive_filename(version, target)
    )
}

fn release_archive_filename(version: &str, target: &str) -> String {
    let ext = if target.ends_with("windows-msvc") {
        "zip"
    } else {
        "tar.gz"
    };
    format!("baml-language-{version}-{target}.{ext}")
}

fn release_host_target_triple() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc"),
        (os, arch) => anyhow::bail!(
            "No released `baml-pack-host` artifact is available for {arch}-{os}. \
             Install `baml-pack-host` next to the `baml` binary to use `baml pack` on this platform."
        ),
    }
}

fn validate_release_target_triple(target: &str) -> Result<&str> {
    match target {
        "aarch64-apple-darwin"
        | "x86_64-apple-darwin"
        | "x86_64-unknown-linux-gnu"
        | "x86_64-pc-windows-msvc" => Ok(target),
        _ => anyhow::bail!(
            "Unsupported pack target `{target}`. Supported targets: {}",
            SUPPORTED_PACK_TARGETS.join(", ")
        ),
    }
}

const SUPPORTED_PACK_TARGETS: &[&str] = &[
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-gnu",
    "x86_64-pc-windows-msvc",
];

fn default_output_path(default_basename: &str, target_triple: &str) -> PathBuf {
    let mut path = PathBuf::from(default_basename);
    if target_triple.ends_with("windows-msvc")
        && path.extension().and_then(|ext| ext.to_str()) != Some("exe")
    {
        path.set_extension("exe");
    }
    path
}

fn extract_host_from_archive(archive_bytes: &[u8], url: &str, host_name: &str) -> Result<Vec<u8>> {
    if url.ends_with(".zip") {
        extract_host_from_zip(archive_bytes, host_name)
    } else {
        extract_host_from_tar_gz(archive_bytes, host_name)
    }
    .with_context(|| format!("Failed to extract `{host_name}` from release archive {url}"))
}

fn extract_host_from_tar_gz(archive_bytes: &[u8], host_name: &str) -> Result<Vec<u8>> {
    let decoder = flate2::read::GzDecoder::new(Cursor::new(archive_bytes));
    let mut archive = tar::Archive::new(decoder);
    for entry in archive.entries().context("Failed to read tar entries")? {
        let mut entry = entry.context("Failed to read tar entry")?;
        if entry
            .path()
            .ok()
            .and_then(|path| path.file_name().map(|name| name == host_name))
            .unwrap_or(false)
        {
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .context("Failed to read host binary from tar archive")?;
            return Ok(bytes);
        }
    }
    anyhow::bail!("Release archive did not contain `{host_name}`")
}

fn extract_host_from_zip(archive_bytes: &[u8], host_name: &str) -> Result<Vec<u8>> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(archive_bytes)).context("Failed to read zip archive")?;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .with_context(|| format!("Failed to read zip entry {i}"))?;
        if Path::new(file.name())
            .file_name()
            .map(|name| name == host_name)
            .unwrap_or(false)
        {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .context("Failed to read host binary from zip archive")?;
            return Ok(bytes);
        }
    }
    anyhow::bail!("Release archive did not contain `{host_name}`")
}

fn write_executable(
    host_bytes: &[u8],
    data: &[u8],
    writer: &mut std::fs::File,
    target_triple: &str,
) -> Result<()> {
    if target_triple.contains("linux") {
        libsui::Elf::new(host_bytes)
            .append(PACK_SECTION_NAME, data, writer)
            .context("Failed to write ELF binary")?;
    } else if target_triple.contains("windows") {
        libsui::PortableExecutable::from(host_bytes)
            .context("Failed to parse PE binary")?
            .write_resource(PACK_SECTION_NAME, data.to_vec())
            .context("Failed to write PE resource")?
            .build(writer)
            .context("Failed to build PE binary")?;
    } else if target_triple.contains("apple-darwin") {
        libsui::Macho::from(host_bytes.to_vec())
            .context("Failed to parse Mach-O binary")?
            .write_section(PACK_SECTION_NAME, data.to_vec())
            .context("Failed to write Mach-O section")?
            .build_and_sign(writer)
            .context("Failed to build Mach-O binary")?;
    } else {
        anyhow::bail!("Unsupported pack target `{target_triple}`");
    }
    Ok(())
}
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn engine_from_source(source: &str) -> BexEngine {
        let snapshot = baml_tests::engine::compile_source(source);
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("BexEngine::new should succeed")
    }

    fn pack_args() -> PackArgs {
        PackArgs {
            target: None,
            function: None,
            output: None,
            target_triple: None,
            output_format: OutputFormat::Json,
            from: PathBuf::from("."),
            expression: None,
        }
    }

    // ── Target resolution — BEP-027 §"What `baml pack` inherits" ───

    /// No target + root `main` exists → packs the root main with
    /// `argv[1] == "main"` and the default basename `"main"`.
    #[test]
    fn test_pack_resolve_no_target_packs_root_main() {
        let engine = engine_from_source("function main() -> int { 42 }");
        let resolved = pack_args().resolve_target(&engine).unwrap();
        assert_eq!(resolved.qualified_name, "user.main");
        assert_eq!(resolved.identifier, "main");
        assert_eq!(resolved.default_basename, "main");
    }

    /// `baml pack -e '<expr>'` must surface a clean "expression mode is
    /// not packageable" message (BEP-027 §"Expression mode is not
    /// packageable") rather than a confusing clap parse error. The
    /// `-e/--expression` flag exists on `PackArgs` solely so we can
    /// intercept it and reject; otherwise the token falls through to
    /// the positional `<TARGET>` slot and surfaces a misleading
    /// "unexpected argument" error.
    #[test]
    fn test_pack_rejects_expression_mode() {
        let mut args = pack_args();
        args.expression = Some("2 + 2".to_string());
        let err = args.run().unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("not packageable"),
            "error should mention non-packageable; got: {msg}"
        );
        assert!(
            msg.contains("-e") || msg.contains("--expression"),
            "error should name the rejected flag; got: {msg}"
        );
        assert!(
            msg.contains("--function") || msg.contains(".baml"),
            "error should point at the supported alternatives; got: {msg}"
        );
    }

    /// No target + no root `main` → error pointing at `--function`.
    #[test]
    fn test_pack_resolve_no_target_no_main_errors() {
        let engine = engine_from_source("function Other() -> int { 1 }");
        let err = pack_args().resolve_target(&engine).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("No `main`"), "got: {msg}");
        assert!(msg.contains("--function"), "got: {msg}");
    }

    /// `--function <name>` → direct function target. BEP: "`argv[1]` is
    /// the fully qualified function name for `--function` packages".
    #[test]
    fn test_pack_resolve_function_flag() {
        let engine = engine_from_source(
            r#"
                function main() -> int { 1 }
                function Summarize(text: string) -> string { text }
            "#,
        );
        let mut args = pack_args();
        args.function = Some("Summarize".to_string());
        let resolved = args.resolve_target(&engine).unwrap();
        assert_eq!(resolved.identifier, "Summarize");
        assert_eq!(resolved.default_basename, "Summarize");
        // Qualified name canonicalizes to whatever form the engine stores.
        assert!(
            resolved.qualified_name == "Summarize" || resolved.qualified_name == "user.Summarize"
        );
    }

    /// BEP-027 §"`baml.argv` in packaged binaries": `argv[1]` is the
    /// *qualified* function name — `user.` prefix stripped — regardless
    /// of how the user spelled the `--function` argument.
    #[test]
    fn test_pack_resolve_function_flag_canonicalizes_user_prefix() {
        let engine = engine_from_source("function Summarize(text: string) -> string { text }");
        let mut args = pack_args();
        args.function = Some("user.Summarize".to_string());
        let resolved = args.resolve_target(&engine).unwrap();
        // identifier (becomes argv[1]) must drop the `user.` prefix.
        assert_eq!(resolved.identifier, "Summarize");
        // Default output basename uses the last `.`-segment of the
        // display name, not the user's raw input.
        assert_eq!(resolved.default_basename, "Summarize");
    }

    /// `--function` with an unknown name → error.
    #[test]
    fn test_pack_resolve_function_flag_unknown_errors() {
        let engine = engine_from_source("function main() -> int { 1 }");
        let mut args = pack_args();
        args.function = Some("DoesNotExist".to_string());
        let err = args.resolve_target(&engine).unwrap_err();
        assert!(format!("{err}").contains("not found"));
    }

    /// `--function` + positional target → error. Dispatch modes are
    /// mutually exclusive.
    #[test]
    fn test_pack_resolve_function_and_positional_errors() {
        let engine = engine_from_source("function main() -> int { 1 }");
        let mut args = pack_args();
        args.function = Some("main".to_string());
        args.target = Some("some_namespace".to_string());
        let err = args.resolve_target(&engine).unwrap_err();
        assert!(format!("{err}").contains("mutually exclusive"));
    }

    /// `.baml` positional → hermetic mode; `argv[1]` is the file basename,
    /// default output uses the file stem.
    #[test]
    fn test_pack_resolve_baml_file_target() {
        let engine = engine_from_source("function main() -> int { 1 }");
        let mut args = pack_args();
        args.target = Some("scripts/hello.baml".to_string());
        let resolved = args.resolve_target(&engine).unwrap();
        assert_eq!(resolved.identifier, "hello.baml");
        assert_eq!(resolved.default_basename, "hello");
    }

    /// `.baml` positional when the file has no `main` → error.
    #[test]
    fn test_pack_resolve_baml_file_without_main_errors() {
        let engine = engine_from_source("function Other() -> int { 1 }");
        let mut args = pack_args();
        args.target = Some("hello.baml".to_string());
        let err = args.resolve_target(&engine).unwrap_err();
        assert!(format!("{err}").contains("no `main`"));
    }

    // NOTE on namespace resolution: BAML namespaces are folder-based
    // (`ns_eval/*.baml` or `ns_eval.baml`), not an inline syntax, so
    // `compile_source` can't construct a multi-namespace engine in-process.
    // The namespace branch of `resolve_target` is exercised end-to-end in
    // the packaging smoke test in the `baml_pack_host` crate (TODO once a
    // cargo-build harness exists); the only logic it contains beyond the
    // "function exists?" check is string formatting (`{target}.main`).

    /// Unknown positional → error that explicitly points out scripts
    /// aren't packable.
    #[test]
    fn test_pack_resolve_unknown_target_errors() {
        let engine = engine_from_source("function main() -> int { 1 }");
        let mut args = pack_args();
        args.target = Some("nonexistent".to_string());
        let err = args.resolve_target(&engine).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("nonexistent"), "got: {msg}");
        assert!(msg.contains("[scripts]"), "got: {msg}");
    }

    // ── Reserved `help` parameter — BEP-027 §"Auto-CLI conventions" ───

    /// A target whose signature declares `help` is rejected at pack time.
    #[test]
    fn test_validate_help_param_rejects_reserved_name() {
        let engine = engine_from_source(r#"function Entry(help: string) -> string { help }"#);
        let err = validate_help_param(&engine, "user.Entry").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("`help`"), "got: {msg}");
        assert!(msg.to_lowercase().contains("rename"), "got: {msg}");
    }

    /// A target without a `help` parameter passes the check.
    #[test]
    fn test_validate_help_param_allows_unrelated_names() {
        let engine =
            engine_from_source(r#"function Entry(text: string, verbose: bool) -> string { text }"#);
        validate_help_param(&engine, "user.Entry").unwrap();
    }

    /// Parameterless `main()` has no params at all → passes trivially.
    #[test]
    fn test_validate_help_param_parameterless_ok() {
        let engine = engine_from_source("function main() -> int { 1 }");
        validate_help_param(&engine, "user.main").unwrap();
    }

    // ── Default flag values — BEP-027 §"What `baml pack` changes" ─────

    /// Per BEP: "Default output format is `json`. `baml run` defaults to
    /// `debug` because its primary reader is a human. Packaged binaries
    /// default to `json` because they are production tools."
    #[test]
    fn test_pack_default_output_format_is_json() {
        use clap::Parser;
        #[derive(Parser)]
        struct Wrapper {
            #[command(flatten)]
            args: PackArgs,
        }
        let parsed = Wrapper::try_parse_from(["baml-pack"]).unwrap();
        assert!(matches!(parsed.args.output_format, OutputFormat::Json));
    }

    #[test]
    fn test_pack_target_triple_flag_parses() {
        use clap::Parser;
        #[derive(Parser)]
        struct Wrapper {
            #[command(flatten)]
            args: PackArgs,
        }
        let parsed =
            Wrapper::try_parse_from(["baml-pack", "--target", "x86_64-pc-windows-msvc"]).unwrap();
        assert_eq!(
            parsed.args.target_triple.as_deref(),
            Some("x86_64-pc-windows-msvc")
        );
    }

    // ── GitHub release fallback ───────────────────────────────────────

    #[test]
    fn test_release_archive_filename_uses_platform_extension() {
        assert_eq!(
            release_archive_filename("1.2.3-alpha.4", "x86_64-unknown-linux-gnu"),
            "baml-language-1.2.3-alpha.4-x86_64-unknown-linux-gnu.tar.gz"
        );
        assert_eq!(
            release_archive_filename("1.2.3-alpha.4", "x86_64-pc-windows-msvc"),
            "baml-language-1.2.3-alpha.4-x86_64-pc-windows-msvc.zip"
        );
    }

    #[test]
    fn test_release_archive_url_defaults_to_github_release_asset() {
        let url = release_archive_url_for_repo(
            "1.2.3-alpha.4",
            "x86_64-unknown-linux-gnu",
            "BoundaryML/baml",
        );
        assert_eq!(
            url,
            "https://github.com/BoundaryML/baml/releases/download/baml-language-1.2.3-alpha.4/baml-language-1.2.3-alpha.4-x86_64-unknown-linux-gnu.tar.gz"
        );
    }

    #[test]
    fn test_release_archive_checksum_url_matches_uploaded_asset() {
        assert_eq!(
            release_archive_checksum_url(
                "https://github.com/BoundaryML/baml/releases/download/baml-language-1.2.3-alpha.4/baml-language-1.2.3-alpha.4-x86_64-unknown-linux-gnu.tar.gz"
            ),
            "https://github.com/BoundaryML/baml/releases/download/baml-language-1.2.3-alpha.4/baml-language-1.2.3-alpha.4-x86_64-unknown-linux-gnu.tar.gz.sha256"
        );
    }

    #[test]
    fn test_verify_release_archive_checksum_text() {
        let archive_name = "baml-language-1.2.3-alpha.4-x86_64-unknown-linux-gnu.tar.gz";
        let archive_url = format!("https://example.com/releases/{archive_name}");
        let archive_bytes = b"fake archive bytes";
        let digest = format!("{:x}", Sha256::digest(archive_bytes));
        let checksum_text = format!("{digest}  {archive_name}\n");

        verify_release_archive_checksum_text(archive_bytes, &archive_url, &checksum_text).unwrap();

        let err =
            verify_release_archive_checksum_text(b"different bytes", &archive_url, &checksum_text)
                .unwrap_err();
        assert!(format!("{err}").contains("Checksum mismatch"));
    }

    #[test]
    fn test_validate_release_target_triple_accepts_supported_targets() {
        for target in SUPPORTED_PACK_TARGETS {
            assert_eq!(validate_release_target_triple(target).unwrap(), *target);
        }
    }

    #[test]
    fn test_validate_release_target_triple_rejects_unknown_target() {
        let err = validate_release_target_triple("wasm32-unknown-unknown").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Unsupported pack target"), "got: {msg}");
        assert!(msg.contains("x86_64-unknown-linux-gnu"), "got: {msg}");
    }

    #[test]
    fn test_host_binary_name_follows_target_triple() {
        assert_eq!(
            host_binary_name("x86_64-unknown-linux-gnu"),
            "baml-pack-host"
        );
        assert_eq!(
            host_binary_name("x86_64-pc-windows-msvc"),
            "baml-pack-host.exe"
        );
    }

    #[test]
    fn test_default_output_path_adds_exe_for_windows_target() {
        assert_eq!(
            default_output_path("main", "x86_64-pc-windows-msvc"),
            PathBuf::from("main.exe")
        );
        assert_eq!(
            default_output_path("main.exe", "x86_64-pc-windows-msvc"),
            PathBuf::from("main.exe")
        );
        assert_eq!(
            default_output_path("main", "x86_64-unknown-linux-gnu"),
            PathBuf::from("main")
        );
    }

    #[test]
    fn test_extract_host_from_tar_gz() {
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        {
            let mut builder = tar::Builder::new(&mut gzip);
            let bytes = b"fake host";
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, "nested/baml-pack-host", &bytes[..])
                .unwrap();
            builder.finish().unwrap();
        }
        let archive = gzip.finish().unwrap();
        assert_eq!(
            extract_host_from_tar_gz(&archive, "baml-pack-host").unwrap(),
            b"fake host"
        );
    }

    #[test]
    fn test_extract_host_from_zip() {
        use std::io::Write;

        let mut archive = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut archive);
            zip.start_file(
                "nested/baml-pack-host.exe",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            zip.write_all(b"fake windows host").unwrap();
            zip.finish().unwrap();
        }
        assert_eq!(
            extract_host_from_zip(&archive.into_inner(), "baml-pack-host.exe").unwrap(),
            b"fake windows host"
        );
    }

    // ── Envelope roundtrip ────────────────────────────────────────────

    /// The PackEnvelope bitcode roundtrip is the wire contract between
    /// pack and the host. A regression here breaks every packaged binary,
    /// so it gets its own test.
    #[test]
    fn test_pack_envelope_roundtrip() {
        let snapshot = baml_tests::engine::compile_source("function main() -> int { 1 }");
        let envelope = PackEnvelope {
            program: snapshot,
            target_name: "user.main".to_string(),
            target_identifier: "main".to_string(),
            output_format: OutputFormat::Json,
        };
        let bytes = bitcode::serialize(&envelope).unwrap();
        let decoded: PackEnvelope = bitcode::deserialize(&bytes).unwrap();
        assert_eq!(decoded.target_name, "user.main");
        assert_eq!(decoded.target_identifier, "main");
        assert!(matches!(decoded.output_format, OutputFormat::Json));
    }

    // ── canonicalize_function_name ────────────────────────────────────

    /// Bare name resolves to whatever qualified form the engine stores.
    /// The engine uses the `user.` prefix for user functions, so lookup
    /// by either form should produce the same canonical qualified name.
    #[test]
    fn test_canonicalize_function_name_resolves_bare_and_qualified() {
        let engine = engine_from_source("function Greet(x: string) -> string { x }");
        let canonical_bare = canonicalize_function_name(&engine, "Greet");
        let canonical_qualified = canonicalize_function_name(&engine, "user.Greet");
        assert_eq!(
            canonical_bare, canonical_qualified,
            "both spellings must canonicalize to the same name",
        );
    }

    /// Unknown names pass through unchanged; callers surface the error
    /// elsewhere (`function_exists` check in `resolve_target`).
    #[test]
    fn test_canonicalize_function_name_unknown_passes_through() {
        let engine = engine_from_source("function main() -> int { 1 }");
        let name = canonicalize_function_name(&engine, "DoesNotExist");
        assert_eq!(name, "DoesNotExist");
    }

    // ── load_project / load_standalone error paths ────────────────────

    /// An empty directory has no `.baml` files → `load_project` errors
    /// before ever reaching compilation.
    #[test]
    fn test_load_project_empty_dir_errors() {
        let tmp = tempfile::tempdir().unwrap();
        // Write a `baml.toml` so the project root is recognized, but
        // no `.baml` files in `baml_src/`.
        std::fs::write(tmp.path().join("baml.toml"), "").unwrap();
        std::fs::create_dir(tmp.path().join("baml_src")).unwrap();

        let mut args = pack_args();
        args.from = tmp.path().to_path_buf();
        let reporter = Reporter::new();
        let err = args.load_project(&reporter).unwrap_err();
        assert!(
            format!("{err}").contains("No .baml files"),
            "expected no-files error; got: {err}",
        );
    }

    /// A nonexistent `.baml` file surfaces the filesystem error rather
    /// than silently returning an empty project.
    #[test]
    fn test_load_standalone_missing_file_errors() {
        let args = pack_args();
        let err = args
            .load_standalone("/nonexistent/ghost/path.baml")
            .unwrap_err();
        let msg = format!("{err:?}"); // use debug to capture the full context chain
        assert!(
            msg.contains("File not found") || msg.contains("nonexistent"),
            "expected file-not-found error; got: {msg}",
        );
    }

    // ── Did-you-mean suggestions for unknown `--function` ────────────

    /// A typo close to an existing function name yields a suggestion.
    #[test]
    fn test_pack_resolve_function_flag_unknown_with_suggestion() {
        let engine = engine_from_source(
            r#"
                function Summarize(text: string) -> string { text }
                function Categorize(text: string) -> string { text }
            "#,
        );
        let mut args = pack_args();
        args.function = Some("Summarise".to_string()); // British spelling
        let err = args.resolve_target(&engine).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("Did you mean"),
            "expected suggestion; got: {msg}"
        );
        assert!(
            msg.contains("Summarize"),
            "expected Summarize in list; got: {msg}"
        );
    }

    /// A truly unrelated name falls back to the no-suggestions message
    /// (which still points at `baml run --list`).
    #[test]
    fn test_pack_resolve_function_flag_unknown_with_no_suggestions() {
        let engine = engine_from_source("function Greet() -> int { 1 }");
        let mut args = pack_args();
        args.function = Some("totally_unrelated_xyz".to_string());
        let err = args.resolve_target(&engine).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("not found"), "got: {msg}");
        assert!(
            msg.contains("--list"),
            "expected --list pointer; got: {msg}"
        );
    }

    /// `function_suggestions` returns substring-containing candidates
    /// first, then jaro-winkler matches, deduplicated.
    #[test]
    fn test_pack_function_suggestions_matches_substrings_and_similar() {
        let engine = engine_from_source(
            r#"
                function Summarize(text: string) -> string { text }
                function ParseSummary() -> string { "x" }
                function Categorize(text: string) -> string { text }
            "#,
        );
        let hits = function_suggestions(&engine, "Summary");
        assert!(
            hits.iter().any(|n| n == "Summarize"),
            "expected Summarize (similar); got: {hits:?}"
        );
        assert!(
            hits.iter().any(|n| n == "ParseSummary"),
            "expected ParseSummary (contains query); got: {hits:?}"
        );
    }
}
