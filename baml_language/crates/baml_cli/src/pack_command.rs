// `baml pack` — compile one or more BAML functions into a single
// self-contained executable.
//
// Target resolution mirrors `baml run`'s shape minus two things:
//   - `-e` is not packageable (no persistent target to bake in).
//   - `[scripts]` are not packageable — scripts are a dev-time dispatch
//     mechanism, not an entry-point concept.
//
// Targets are picked via repeatable `-f/--function` flags. With multiple
// `-f` the packed binary becomes a multi-subcommand CLI (`./cli foo …`,
// `./cli bar …`); with a single `-f` it's the same shape with one
// subcommand. A bare positional `<TARGET>` runs a single function as the
// binary's only entry point (no subcommand layer).
//
// Standalone single-file sources are loaded via `--file <PATH>`, which
// is mutually exclusive with `--from <DIR>`. Without either, the project
// at the current directory is loaded.
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
use baml_exec::{OutputFormat, PACK_SECTION_NAME, PackEnvelope, validate_help_param};
use baml_project::ProjectDatabase;
use bex_engine::BexEngine;
use bex_vm_types::types::Program;
use clap::Args;
use sha2::{Digest, Sha256};
use sys_native::SysOpsExt;

use crate::{
    commands::release_version, project_load::load_project_from_reporting, reporter::Reporter,
};

/// `baml pack` — compile one or more targets into a standalone executable.
#[derive(Args, Clone, Debug)]
pub struct PackArgs {
    /// Positional target: a function name to pack as the binary's only
    /// entry point. Mutually exclusive with `-f/--function`.
    #[arg(value_name = "TARGET")]
    pub target: Option<String>,

    /// Pack a specific function as a subcommand of the produced binary.
    /// Repeatable: each `-f` adds another subcommand. With one `-f` the
    /// binary still has a subcommand layer (vs. a bare positional, which
    /// produces a single-entry binary with no subcommand).
    #[arg(short = 'f', long = "function", value_name = "NAME")]
    pub functions: Vec<String>,

    /// Standalone single-file source. Loads only this file (no project
    /// discovery). Mutually exclusive with `--from`.
    #[arg(long, value_name = "PATH")]
    pub file: Option<PathBuf>,

    /// Output path for the packaged executable. Defaults to the
    /// `[package].name` from `baml.toml`; for a single target with no
    /// `[package].name`, falls back to the function name.
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

    /// Project root directory. Ignored when `--file` is set.
    #[arg(long, default_value = ".")]
    pub from: PathBuf,

    /// `-e/--expression` is recognized only so we can reject it with a
    /// targeted message — `baml pack` doesn't support expression mode.
    /// Without this flag declared, `baml pack -e '...'` falls through to
    /// clap's positional handling and surfaces a confusing parse error.
    #[arg(short = 'e', long = "expression", value_name = "EXPR")]
    pub expression: Option<String>,
}

/// One resolved entry point baked into a packed binary.
#[derive(Debug, Clone)]
struct ResolvedPackTarget {
    qualified_name: String,
    /// Display name (qualified, `user.` prefix stripped).
    display_name: String,
    /// CLI subcommand name (last `.`-segment of `display_name`).
    subcommand_name: String,
}

impl PackArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let reporter = Reporter::new();
        self.run_with_reporter(&reporter)
    }

    fn run_with_reporter(&self, reporter: &Reporter) -> Result<crate::ExitCode> {
        self.validate_flags()?;

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

        let (mode, targets) = self.resolve_targets(&engine)?;
        for t in &targets {
            validate_help_param(&engine, &t.qualified_name)?;
        }
        let label = label_for(&targets);
        reporter.spin("Packaging", &label);

        let envelope = PackEnvelope {
            program,
            mode: mode.clone(),
            targets: targets
                .iter()
                .map(|t| baml_exec::TargetEntry {
                    qualified_name: t.qualified_name.clone(),
                    display_name: t.display_name.clone(),
                    subcommand_name: t.subcommand_name.clone(),
                })
                .collect(),
            output_format: self.output_format,
        };
        let serialized = bitcode::serialize(&envelope)
            .map_err(|e| anyhow!("Failed to serialize pack envelope: {e}"))?;

        let target_triple = self.resolved_target_triple()?;
        let host_bytes = read_host_binary(target_triple, reporter)?;
        let basename = self.resolve_output_basename()?;
        let output_path = self
            .output
            .clone()
            .unwrap_or_else(|| default_output_path(&basename, target_triple));

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
            format!("{} [{label}, {}]", output_path.display(), target_triple),
        );
        Ok(crate::ExitCode::Success)
    }

    /// Validate flag combinations the clap-side `Args` derive can't catch.
    fn validate_flags(&self) -> Result<()> {
        if self.expression.is_some() {
            anyhow::bail!(
                "expression mode (`-e` / `--expression`) is not packageable; \
                 pass a positional `<TARGET>` or `-f <NAME>` instead."
            );
        }
        // `--file` and `--from` both name a source location. Reject the
        // combination up front instead of silently preferring one. Same
        // rule as `baml run`. The check is gated on `--from != "."`
        // because clap can't tell "user passed `.`" from "user passed
        // nothing"; `--file` alongside the default `--from` is fine.
        if self.file.is_some() && self.from != Path::new(".") {
            anyhow::bail!(
                "`--file` and `--from` are mutually exclusive — `--file` already names \
                 the single source to load."
            );
        }
        if let Some(target) = self.target.as_deref() {
            if looks_like_path(target) {
                anyhow::bail!(
                    "positional `<TARGET>` is a function name, not a file path. \
                     For a single-file source, use `--file {target}` and pass the \
                     function via `-f <NAME>`. For example:\n\
                     \n    baml pack --file {target} -f <NAME>\n",
                );
            }
            if !self.functions.is_empty() {
                anyhow::bail!(
                    "positional `<TARGET>` and `-f/--function` are mutually exclusive — \
                     use one or the other (positional packs a single-entry binary; \
                     `-f` produces a subcommand binary)."
                );
            }
        }
        if self.target.is_none() && self.functions.is_empty() {
            anyhow::bail!(
                "no target specified. Pass a positional `<TARGET>` to pack one \
                 function as the binary's only entry point, or one or more \
                 `-f <NAME>` flags to pack a subcommand binary."
            );
        }
        Ok(())
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
        let (db, needs_format_hint) = if let Some(file) = self.file.as_deref() {
            let display = file.display().to_string();
            reporter.spin("Loading", &display);
            self.load_standalone(file)?
        } else {
            self.load_project(reporter)?
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

    fn load_standalone(&self, file_path: &Path) -> Result<(ProjectDatabase, bool)> {
        let canonical = std::fs::canonicalize(file_path)
            .with_context(|| format!("File not found: {}", file_path.display()))?;
        let content = std::fs::read_to_string(&canonical)
            .with_context(|| format!("Failed to read {}", canonical.display()))?;
        let needs_format_hint = crate::run_command::source_needs_format_hint(&content);
        let parent = canonical.parent().unwrap_or_else(|| Path::new("."));
        let mut db = ProjectDatabase::new();
        db.set_project_root(parent);
        db.add_or_update_file(&canonical, &content);
        Ok((db, needs_format_hint))
    }

    /// Resolve into `(mode, targets)`. Positional `<TARGET>` →
    /// [`PackMode::Single`] with one entry; one-or-more `-f` →
    /// [`PackMode::Subcommand`] (a single `-f` still gets the subcommand
    /// layer per the spec).
    fn resolve_targets(
        &self,
        engine: &BexEngine,
    ) -> Result<(baml_exec::PackMode, Vec<ResolvedPackTarget>)> {
        if let Some(target) = self.target.as_deref() {
            let resolved = resolve_one(engine, target)?;
            return Ok((baml_exec::PackMode::Single, vec![resolved]));
        }

        // Subcommand mode. Resolve each `-f` and reject duplicate subcommand names.
        let mut resolved: Vec<ResolvedPackTarget> = Vec::with_capacity(self.functions.len());
        for func in &self.functions {
            resolved.push(resolve_one(engine, func)?);
        }
        let mut seen: HashMap<&str, &str> = HashMap::new();
        for r in &resolved {
            if let Some(prev) = seen.insert(r.subcommand_name.as_str(), r.display_name.as_str()) {
                anyhow::bail!(
                    "two targets share subcommand name `{}` (`{}` and `{}`). \
                     Subcommand names come from the last `.`-segment of the function name; \
                     rename one of them so the binary's subcommands stay unambiguous.",
                    r.subcommand_name,
                    prev,
                    r.display_name,
                );
            }
        }
        Ok((baml_exec::PackMode::Subcommand, resolved))
    }

    /// Pick the default output basename. With `[package].name` mandatory
    /// in `baml.toml` (validated up front by `project_load`), project-mode
    /// output naming has exactly one source of truth.
    ///
    /// - `--file <PATH>` single-file mode: file stem (e.g. `foo.baml` →
    ///   `foo`). `baml.toml` isn't consulted — single-file packs are
    ///   intentionally hermetic.
    /// - Project mode: `[package].name` from `<from>/baml.toml`. Guaranteed
    ///   present (manifest validation happened at load time).
    fn resolve_output_basename(&self) -> Result<String> {
        if let Some(file) = self.file.as_deref() {
            if let Some(stem) = file.file_stem().and_then(|s| s.to_str()) {
                return Ok(stem.to_string());
            }
            // Pathologically nameless file (e.g. `.baml`); fall through to
            // the manifest lookup. In `--file` mode that may still fail
            // (no project to consult); the user can always pass `-o`.
        }
        crate::project_load::read_package_name(&self.from)
    }
}

/// Resolve a single function-name string against the engine; returns
/// canonical qualified/display/subcommand-name triple.
fn resolve_one(engine: &BexEngine, func: &str) -> Result<ResolvedPackTarget> {
    if !engine.function_exists(func) {
        let suggestions = function_suggestions(engine, func);
        if suggestions.is_empty() {
            anyhow::bail!(
                "Function `{func}` not found. Use `baml run --list` to see \
                 available targets."
            );
        }
        anyhow::bail!(
            "Function `{func}` not found. Did you mean one of:\n{}",
            suggestions
                .iter()
                .map(|s| format!("  - {s}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    let qualified_name = canonicalize_function_name(engine, func);
    let display_name = qualified_name
        .strip_prefix("user.")
        .unwrap_or(&qualified_name)
        .to_string();
    let subcommand_name = display_name
        .rsplit('.')
        .next()
        .unwrap_or(&display_name)
        .to_string();
    Ok(ResolvedPackTarget {
        qualified_name,
        display_name,
        subcommand_name,
    })
}

/// Label for status output: `display_name` for single targets,
/// `a,b,c` for subcommand packs.
fn label_for(targets: &[ResolvedPackTarget]) -> String {
    match targets {
        [single] => single.display_name.clone(),
        _ => targets
            .iter()
            .map(|t| t.subcommand_name.as_str())
            .collect::<Vec<_>>()
            .join(","),
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
    #[cfg(target_env = "musl")]
    const IS_MUSL: bool = true;
    #[cfg(not(target_env = "musl"))]
    const IS_MUSL: bool = false;

    match (std::env::consts::OS, std::env::consts::ARCH, IS_MUSL) {
        ("macos", "aarch64", _) => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64", _) => Ok("x86_64-apple-darwin"),
        ("linux", "aarch64", true) => Ok("aarch64-unknown-linux-musl"),
        ("linux", "aarch64", false) => Ok("aarch64-unknown-linux-gnu"),
        ("linux", "x86_64", true) => Ok("x86_64-unknown-linux-musl"),
        ("linux", "x86_64", false) => Ok("x86_64-unknown-linux-gnu"),
        ("windows", "x86_64", _) => Ok("x86_64-pc-windows-msvc"),
        (os, arch, _) => anyhow::bail!(
            "No released `baml-pack-host` artifact is available for {arch}-{os}. \
             Install `baml-pack-host` next to the `baml` binary to use `baml pack` on this platform."
        ),
    }
}

fn validate_release_target_triple(target: &str) -> Result<&str> {
    if SUPPORTED_PACK_TARGETS.contains(&target) {
        Ok(target)
    } else {
        anyhow::bail!(
            "Unsupported pack target `{target}`. Supported targets: {}",
            SUPPORTED_PACK_TARGETS.join(", ")
        )
    }
}

const SUPPORTED_PACK_TARGETS: &[&str] = &[
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "aarch64-unknown-linux-gnu",
    "aarch64-unknown-linux-musl",
    "x86_64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
    "x86_64-pc-windows-msvc",
];

/// Heuristic: does this positional `<TARGET>` look like a filesystem
/// path rather than a function name? Triggers when the user typed
/// something like `baml_src/main.baml` and we want to redirect them to
/// `--file`. Function names can't contain `/` or `\`, and the `.baml`
/// suffix is the strong signal — namespaced functions like `llm.Foo`
/// use `.` but never end in `.baml`.
fn looks_like_path(target: &str) -> bool {
    target.contains('/') || target.contains('\\') || target.ends_with(".baml")
}

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

    /// Build an engine from a multi-file project so we can exercise
    /// namespaced functions (`ns_<name>/foo.baml` → `<name>.foo`). Single
    /// `engine_from_source` can't express folder-based namespaces.
    fn engine_from_files(files: &[(&str, &str)]) -> BexEngine {
        let snapshot = baml_project::testing::compile_multi_file(files);
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
            functions: Vec::new(),
            file: None,
            output: None,
            target_triple: None,
            output_format: OutputFormat::Json,
            from: PathBuf::from("."),
            expression: None,
        }
    }

    // ── Target resolution ─────────────────────────────────────────────

    /// Positional `<TARGET>` → single-target pack with the function
    /// name as both display and subcommand.
    #[test]
    fn test_pack_positional_single_target() {
        let engine = engine_from_source("function main() -> int { 42 }");
        let mut args = pack_args();
        args.target = Some("main".to_string());
        let (mode, targets) = args.resolve_targets(&engine).unwrap();
        assert!(matches!(mode, baml_exec::PackMode::Single));
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].qualified_name, "user.main");
        assert_eq!(targets[0].display_name, "main");
        assert_eq!(targets[0].subcommand_name, "main");
    }

    /// `--file` + `--from` rejected (both name a source).
    #[test]
    fn test_pack_rejects_file_plus_explicit_from() {
        let mut args = pack_args();
        args.functions = vec!["main".into()];
        args.file = Some(PathBuf::from("a.baml"));
        args.from = PathBuf::from("./project");
        let err = args.validate_flags().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("mutually exclusive"), "got: {msg}");
        assert!(msg.contains("--file"), "got: {msg}");
    }

    /// `-e` is rejected before any other validation runs.
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
    }

    /// No positional and no `-f` → user gets a clean "no target" error.
    #[test]
    fn test_pack_no_target_errors() {
        let args = pack_args();
        let err = args.validate_flags().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("no target"), "got: {msg}");
        assert!(msg.contains("-f"), "got: {msg}");
    }

    /// Positional + `-f` is rejected.
    #[test]
    fn test_pack_positional_plus_function_errors() {
        let mut args = pack_args();
        args.target = Some("main".to_string());
        args.functions = vec!["Summarize".to_string()];
        let err = args.validate_flags().unwrap_err();
        assert!(format!("{err}").contains("mutually exclusive"));
    }

    /// Path-shaped positional → redirect to `--file`, not the generic
    /// "mutually exclusive" message. Covers the common confusion of
    /// `baml pack baml_src/main.baml -f main`.
    #[test]
    fn test_pack_path_positional_suggests_file_flag() {
        let mut args = pack_args();
        args.target = Some("baml_src/main.baml".to_string());
        args.functions = vec!["main".to_string()];
        let err = args.validate_flags().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("--file"), "expected --file hint; got: {msg}");
        assert!(
            msg.contains("baml_src/main.baml"),
            "expected the path echoed back; got: {msg}",
        );
    }

    /// Same redirect when the path-shaped positional appears alone (no `-f`).
    /// The hint should win over the generic "no target specified" path.
    #[test]
    fn test_pack_bare_path_positional_suggests_file_flag() {
        let mut args = pack_args();
        args.target = Some("./hello.baml".to_string());
        let err = args.validate_flags().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("--file"), "got: {msg}");
    }

    #[test]
    fn test_looks_like_path_signal() {
        assert!(looks_like_path("baml_src/main.baml"));
        assert!(looks_like_path("hello.baml"));
        assert!(looks_like_path("./x"));
        assert!(looks_like_path(r"C:\foo"));
        // Function names don't look like paths.
        assert!(!looks_like_path("main"));
        assert!(!looks_like_path("llm.Summarize"));
        assert!(!looks_like_path("user.llm.Summarize"));
    }

    /// Single `-f` → subcommand mode with one entry (subcommand layer
    /// retained, per the spec).
    #[test]
    fn test_pack_single_function_uses_subcommand_mode() {
        let engine = engine_from_source(
            r#"
                function main() -> int { 1 }
                function Summarize(text: string) -> string { text }
            "#,
        );
        let mut args = pack_args();
        args.functions = vec!["Summarize".to_string()];
        let (mode, targets) = args.resolve_targets(&engine).unwrap();
        assert!(matches!(mode, baml_exec::PackMode::Subcommand));
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].subcommand_name, "Summarize");
    }

    /// Multiple `-f` → subcommand mode, one entry per `-f`.
    #[test]
    fn test_pack_multi_function_subcommand_mode() {
        let engine = engine_from_source(
            r#"
                function Summarize(text: string) -> string { text }
                function Categorize(text: string) -> string { text }
            "#,
        );
        let mut args = pack_args();
        args.functions = vec!["Summarize".to_string(), "Categorize".to_string()];
        let (mode, targets) = args.resolve_targets(&engine).unwrap();
        assert!(matches!(mode, baml_exec::PackMode::Subcommand));
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].subcommand_name, "Summarize");
        assert_eq!(targets[1].subcommand_name, "Categorize");
    }

    /// Subcommand name canonicalizes to the last `.`-segment of the
    /// display name regardless of how the user spelled the `-f` flag.
    #[test]
    fn test_pack_function_flag_strips_user_prefix() {
        let engine = engine_from_source("function Summarize(text: string) -> string { text }");
        let mut args = pack_args();
        args.functions = vec!["user.Summarize".to_string()];
        let (_mode, targets) = args.resolve_targets(&engine).unwrap();
        assert_eq!(targets[0].display_name, "Summarize");
        assert_eq!(targets[0].subcommand_name, "Summarize");
    }

    /// Unknown function → error.
    #[test]
    fn test_pack_function_unknown_errors() {
        let engine = engine_from_source("function main() -> int { 1 }");
        let mut args = pack_args();
        args.functions = vec!["DoesNotExist".to_string()];
        let err = args.resolve_targets(&engine).unwrap_err();
        assert!(format!("{err}").contains("not found"));
    }

    // ── Namespaced targets (folder-based, `ns_<name>/`) ──────────────

    /// Positional `<TARGET>` resolves a namespaced function by display
    /// name. The packed binary's `argv[1]` (identifier / subcommand
    /// name) is the *last* `.`-segment — `llm.Summarize` → `Summarize`.
    #[test]
    fn test_pack_namespaced_positional() {
        let engine = engine_from_files(&[(
            "ns_llm/summarize.baml",
            "function Summarize(text: string) -> string { text }",
        )]);
        let mut args = pack_args();
        args.target = Some("llm.Summarize".to_string());
        let (mode, targets) = args.resolve_targets(&engine).unwrap();
        assert!(matches!(mode, baml_exec::PackMode::Single));
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].qualified_name, "user.llm.Summarize");
        assert_eq!(targets[0].display_name, "llm.Summarize");
        assert_eq!(targets[0].subcommand_name, "Summarize");
    }

    /// `-f` with namespaced names produces subcommand-mode targets keyed
    /// on each function's last `.`-segment.
    #[test]
    fn test_pack_namespaced_multi_function() {
        let engine = engine_from_files(&[
            (
                "ns_llm/summarize.baml",
                "function Summarize(text: string) -> string { text }",
            ),
            (
                "ns_util/greet.baml",
                "function Greet(name: string) -> string { name }",
            ),
        ]);
        let mut args = pack_args();
        args.functions = vec!["llm.Summarize".into(), "util.Greet".into()];
        let (mode, targets) = args.resolve_targets(&engine).unwrap();
        assert!(matches!(mode, baml_exec::PackMode::Subcommand));
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].display_name, "llm.Summarize");
        assert_eq!(targets[0].subcommand_name, "Summarize");
        assert_eq!(targets[1].display_name, "util.Greet");
        assert_eq!(targets[1].subcommand_name, "Greet");
    }

    /// Two namespaces exporting the same leaf name → subcommand-name
    /// collision is rejected with a clear message that names both
    /// fully-qualified targets so the user can rename one.
    #[test]
    fn test_pack_namespaced_subcommand_name_collision_errors() {
        let engine = engine_from_files(&[
            ("ns_llm/foo.baml", "function Foo() -> int { 1 }"),
            ("ns_util/foo.baml", "function Foo() -> int { 2 }"),
        ]);
        let mut args = pack_args();
        args.functions = vec!["llm.Foo".into(), "util.Foo".into()];
        let err = args.resolve_targets(&engine).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("share subcommand name `Foo`"), "got: {msg}");
        assert!(msg.contains("llm.Foo"), "got: {msg}");
        assert!(msg.contains("util.Foo"), "got: {msg}");
    }

    /// Bare-name lookup also resolves namespaced targets via the
    /// shared resolver's suffix scan — `Summarize` finds `llm.Summarize`
    /// when it's the unique match.
    #[test]
    fn test_pack_namespaced_resolves_via_bare_name() {
        let engine = engine_from_files(&[(
            "ns_llm/summarize.baml",
            "function Summarize(text: string) -> string { text }",
        )]);
        let mut args = pack_args();
        args.functions = vec!["Summarize".into()];
        let (_, targets) = args.resolve_targets(&engine).unwrap();
        assert_eq!(targets[0].qualified_name, "user.llm.Summarize");
        assert_eq!(targets[0].subcommand_name, "Summarize");
    }

    /// Two `-f`s with the same trailing segment → error.
    #[test]
    fn test_pack_duplicate_subcommand_names_error() {
        // Without namespaces in `compile_source` we can only exercise the
        // duplicate path by repeating the same function. Engine accepts
        // duplicates in the input list — `resolve_targets` is what rejects.
        let engine = engine_from_source("function Foo() -> int { 1 }");
        let mut args = pack_args();
        args.functions = vec!["Foo".to_string(), "Foo".to_string()];
        let err = args.resolve_targets(&engine).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("share subcommand name"), "got: {msg}");
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

    /// `-f` / `--function` are repeatable through the clap derive — same
    /// regression coverage as `RunArgs`.
    #[test]
    fn test_pack_dash_f_is_repeatable() {
        use clap::Parser;
        #[derive(Parser)]
        struct Wrapper {
            #[command(flatten)]
            args: PackArgs,
        }
        let parsed =
            Wrapper::try_parse_from(["baml-pack", "-f", "a", "-f", "b", "--function", "c"])
                .unwrap();
        assert_eq!(parsed.args.functions, vec!["a", "b", "c"]);
        assert!(parsed.args.target.is_none());
    }

    /// `--file` binds to the PathBuf field on `PackArgs`.
    #[test]
    fn test_pack_file_flag_parses() {
        use clap::Parser;
        #[derive(Parser)]
        struct Wrapper {
            #[command(flatten)]
            args: PackArgs,
        }
        let parsed =
            Wrapper::try_parse_from(["baml-pack", "-f", "foo", "--file", "x.baml"]).unwrap();
        assert_eq!(parsed.args.functions, vec!["foo"]);
        assert_eq!(parsed.args.file.as_deref(), Some(Path::new("x.baml")));
    }

    // ── Output basename resolution ────────────────────────────────────

    /// Project mode (`baml.toml` with `[package].name`) → use that name
    /// directly. No fallback to function name; manifest validation up
    /// front guarantees the field is there.
    #[test]
    fn test_pack_resolve_basename_uses_package_name() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("baml.toml"),
            "[package]\nname = \"my-app\"\n",
        )
        .unwrap();
        let mut args = pack_args();
        args.from = tmp.path().to_path_buf();
        assert_eq!(args.resolve_output_basename().unwrap(), "my-app");
    }

    /// `--file foo.baml` → file stem.
    #[test]
    fn test_pack_file_mode_defaults_to_file_stem() {
        let mut args = pack_args();
        args.file = Some(PathBuf::from("scripts/foo.baml"));
        assert_eq!(args.resolve_output_basename().unwrap(), "foo");
    }

    /// `--file` ignores adjacent `baml.toml` — single-file packs are
    /// hermetic by intent.
    #[test]
    fn test_pack_file_mode_ignores_adjacent_baml_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let baml_file = tmp.path().join("hello.baml");
        std::fs::write(&baml_file, "function describe() -> int { 1 }").unwrap();
        std::fs::write(
            tmp.path().join("baml.toml"),
            "[package]\nname = \"unrelated\"\n",
        )
        .unwrap();

        let mut args = pack_args();
        args.file = Some(baml_file);
        // `--from` defaults to `.`, but file mode short-circuits before
        // touching it. Stem wins.
        assert_eq!(args.resolve_output_basename().unwrap(), "hello");
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
            mode: baml_exec::PackMode::Single,
            targets: vec![baml_exec::TargetEntry {
                qualified_name: "user.main".to_string(),
                display_name: "main".to_string(),
                subcommand_name: "main".to_string(),
            }],
            output_format: OutputFormat::Json,
        };
        let bytes = bitcode::serialize(&envelope).unwrap();
        let decoded: PackEnvelope = bitcode::deserialize(&bytes).unwrap();
        assert!(matches!(decoded.mode, baml_exec::PackMode::Single));
        assert_eq!(decoded.targets.len(), 1);
        assert_eq!(decoded.targets[0].qualified_name, "user.main");
        assert_eq!(decoded.targets[0].subcommand_name, "main");
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
        // Write a valid manifest so the project root is recognized, but
        // no `.baml` files in `baml_src/`.
        std::fs::write(
            tmp.path().join("baml.toml"),
            "[package]\nname = \"empty\"\n",
        )
        .unwrap();
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
            .load_standalone(Path::new("/nonexistent/ghost/path.baml"))
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
        args.functions = vec!["Summarise".to_string()]; // British spelling
        let err = args.resolve_targets(&engine).unwrap_err();
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
        args.functions = vec!["totally_unrelated_xyz".to_string()];
        let err = args.resolve_targets(&engine).unwrap_err();
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
