// `baml pack` — compile any `baml run` target (except expression mode)
// into a single self-contained executable. See BEP-027 §"Packaging".
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
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow};
use baml_db::baml_compiler2_emit;
use baml_exec::{OutputFormat, PackEnvelope};
use baml_project::ProjectDatabase;
use bex_engine::BexEngine;
use bex_vm_types::types::Program;
use clap::Args;
use sys_native::SysOpsExt;

use crate::project_load::{check_project_diagnostics, load_project_from};

/// Section name where the packed envelope lives inside the host binary.
/// Kept in sync with `baml_pack_host::SECTION_NAME`.
const PACK_SECTION_NAME: &str = "baml_pack";

/// `baml pack` — compile a target into a standalone executable.
///
/// Accepts the same target shapes as `baml run` (positional namespace,
/// `.baml` file for hermetic mode, or `--function` for a named function),
/// minus expression mode.
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

    /// Output format baked into the binary. Defaults to `json`; packaged
    /// binaries are production tools whose primary reader is another
    /// program. Use `debug` for human-readable output.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub output_format: OutputFormat,

    /// Project root directory. Ignored for hermetic `.baml` targets.
    #[arg(long, default_value = ".")]
    pub from: PathBuf,
}

/// Resolved entry point: everything needed to build a `PackEnvelope`.
#[derive(Debug)]
struct ResolvedPackTarget {
    /// Qualified function name the engine will dispatch against.
    qualified_name: String,
    /// `argv[1]` the packaged binary should expose at runtime
    /// (BEP-027 §"baml.argv in packaged binaries").
    identifier: String,
    /// Default binary filename when `--output` is not supplied.
    default_basename: String,
}

impl PackArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        // Compile the target's enclosing project (or hermetic file).
        let (db, program) = self.load_and_compile()?;
        let _ = db;

        // We need signature info for target resolution and the reserved
        // `help` check; an engine is the only surface exposing that.
        let engine = BexEngine::new(
            program.clone(),
            Arc::new(sys_native::SysOps::native()),
            None,
            // argv is baked in at run time by the host; a placeholder is
            // fine here because we only introspect signatures.
            vec![],
        )
        .map_err(|e| anyhow!("Failed to initialize engine for resolution: {e:?}"))?;

        let resolved = self.resolve_target(&engine)?;
        validate_help_param(&engine, &resolved.qualified_name)?;

        let envelope = PackEnvelope {
            program,
            target_name: resolved.qualified_name.clone(),
            target_identifier: resolved.identifier.clone(),
            output_format: self.output_format,
        };
        let serialized = bitcode::serialize(&envelope)
            .map_err(|e| anyhow!("Failed to serialize pack envelope: {e}"))?;

        let host_bytes = read_host_binary()?;
        let output_path = self
            .output
            .clone()
            .unwrap_or_else(|| PathBuf::from(&resolved.default_basename));

        let mut output_file = std::fs::File::create(&output_path)
            .with_context(|| format!("Failed to create {}", output_path.display()))?;
        write_executable(&host_bytes, &serialized, &mut output_file)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&output_path, std::fs::Permissions::from_mode(0o755))
                .with_context(|| {
                    format!("Failed to set permissions on {}", output_path.display())
                })?;
        }

        eprintln!(
            "Packaged {} → {}",
            resolved
                .qualified_name
                .strip_prefix("user.")
                .unwrap_or(&resolved.qualified_name),
            output_path.display()
        );
        Ok(crate::ExitCode::Success)
    }

    /// Load and compile either the enclosing project or, for a `.baml`
    /// positional target, the single file in hermetic mode.
    fn load_and_compile(&self) -> Result<(ProjectDatabase, Program)> {
        let db = match self.target.as_deref() {
            Some(t) if t.ends_with(".baml") => self.load_standalone(t)?,
            _ => self.load_project()?,
        };

        check_project_diagnostics(&db, "Cannot pack: compilation errors found")?;

        let program = baml_compiler2_emit::generate_project_bytecode(
            &db,
            &baml_compiler2_emit::CompileOptions {
                emit_test_cases: false,
            },
        )
        .map_err(|e| anyhow!("Compilation failed: {e:?}"))?;

        Ok((db, program))
    }

    fn load_project(&self) -> Result<ProjectDatabase> {
        let (db, from, baml_files) = load_project_from(&self.from)?;
        if baml_files.is_empty() {
            anyhow::bail!("No .baml files found in {}", from.display());
        }
        Ok(db)
    }

    fn load_standalone(&self, file_path: &str) -> Result<ProjectDatabase> {
        let canonical = std::fs::canonicalize(Path::new(file_path))
            .with_context(|| format!("File not found: {file_path}"))?;
        let content = std::fs::read_to_string(&canonical)
            .with_context(|| format!("Failed to read {}", canonical.display()))?;
        let parent = canonical.parent().unwrap_or_else(|| Path::new("."));
        let mut db = ProjectDatabase::new();
        db.set_project_root(parent);
        db.add_or_update_file(&canonical, &content);
        Ok(db)
    }

    /// Resolve the pack target to `(qualified_name, identifier, default_basename)`.
    ///
    /// `identifier` is what BEP-027 says `baml.argv[1]` should be in the
    /// resulting binary:
    ///   - `--function llm.Summarize` → `"llm.Summarize"`
    ///   - namespace `eval`           → `"eval"`
    ///   - root `main`                → `"main"`
    ///   - hermetic file `hello.baml` → `"hello.baml"` (basename)
    fn resolve_target(&self, engine: &BexEngine) -> Result<ResolvedPackTarget> {
        if self.function.is_some() && self.target.is_some() {
            anyhow::bail!("`--function` and a positional target are mutually exclusive.");
        }

        if let Some(func) = &self.function {
            if !engine.function_exists(func) {
                anyhow::bail!("Function `{func}` not found.");
            }
            let basename = func.rsplit('.').next().unwrap_or(func).to_string();
            return Ok(ResolvedPackTarget {
                qualified_name: canonicalize_function_name(engine, func),
                identifier: func.clone(),
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

/// Reject targets whose signature declares a parameter named `help`
/// (BEP-027 §"Auto-CLI conventions" — "One reserved parameter name: help").
///
/// The restriction is an entry-point check, not a function-declaration
/// check: the same function remains callable from library code.
fn validate_help_param(engine: &BexEngine, function_name: &str) -> Result<()> {
    if let Ok(params) = engine.function_params(function_name) {
        if params.iter().any(|(name, _)| *name == "help") {
            anyhow::bail!(
                "Target `{function_name}` declares a parameter named `help`, \
                 which collides with the packaged binary's `--help` flag. \
                 Rename this parameter to be used as an entry point."
            );
        }
    }
    Ok(())
}

/// Return the qualified name the engine prefers when both `foo` and
/// `user.foo` resolve to the same function. Prefers the engine-qualified
/// form so dispatch at runtime hits the same path the signature lookup did.
fn canonicalize_function_name(engine: &BexEngine, name: &str) -> String {
    for info in engine.user_functions() {
        if info.qualified_name == name || info.display_name == name {
            return info.qualified_name;
        }
    }
    name.to_string()
}

fn read_host_binary() -> Result<Vec<u8>> {
    let exe = std::env::current_exe().context("Failed to locate current executable")?;
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow!("Cannot determine directory of current executable"))?;
    let host_name = if cfg!(windows) {
        "baml-pack-host.exe"
    } else {
        "baml-pack-host"
    };
    let host_path = dir.join(host_name);
    if !host_path.exists() {
        anyhow::bail!(
            "Could not find `{host_name}` next to the current binary at {}",
            dir.display()
        );
    }
    std::fs::read(&host_path).with_context(|| format!("Failed to read {}", host_path.display()))
}

fn write_executable(host_bytes: &[u8], data: &[u8], writer: &mut std::fs::File) -> Result<()> {
    let target = std::env::consts::OS;
    if target.contains("linux") {
        libsui::Elf::new(host_bytes)
            .append(PACK_SECTION_NAME, data, writer)
            .context("Failed to write ELF binary")?;
    } else if target.contains("windows") {
        libsui::PortableExecutable::from(host_bytes)
            .context("Failed to parse PE binary")?
            .write_resource(PACK_SECTION_NAME, data.to_vec())
            .context("Failed to write PE resource")?
            .build(writer)
            .context("Failed to build PE binary")?;
    } else {
        libsui::Macho::from(host_bytes.to_vec())
            .context("Failed to parse Mach-O binary")?
            .write_section(PACK_SECTION_NAME, data.to_vec())
            .context("Failed to write Mach-O section")?
            .build_and_sign(writer)
            .context("Failed to build Mach-O binary")?;
    }
    Ok(())
}

// ============================================================================
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
            output_format: OutputFormat::Json,
            from: PathBuf::from("."),
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
        let err = args.load_project().unwrap_err();
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
}
