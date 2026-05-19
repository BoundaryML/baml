#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow};
use baml_db::{
    baml_compiler_diagnostics::{Severity, render},
    baml_compiler2_emit,
};
use baml_project::ProjectDatabase;
use baml_workspace::discover_baml_files;
use bex_engine::{BexEngine, FunctionCallContextBuilder, UserFunctionInfo};
// For --log-file event sink.
use clap::Args;
use sys_native::{CallId, SysOpsExt};

use crate::project_load::load_project_from;

// ============================================================================
// Script expansion types
// ============================================================================

/// Parsed script body from `[scripts]` in `baml.toml`.
#[derive(Debug, Clone)]
struct ScriptExpansion {
    /// --function value, if present in the script body.
    function: Option<String>,
    /// Arguments after `--` in the script body.
    extra_args: Vec<String>,
}

/// Result of target resolution.
#[derive(Debug)]
enum ResolvedTarget {
    /// Direct function call (from --function, namespace main, etc.)
    Function(String),
    /// Script expansion from [scripts] in baml.toml.
    Script(ScriptExpansion),
}

impl ResolvedTarget {
    fn function(name: String) -> Self {
        Self::Function(name)
    }
}

/// Parse a script body (as pre-split tokens) into its components.
///
/// Tokens are `baml run` arguments without the `baml run` prefix.
/// They come from the `[scripts]` section of `baml.toml`, which supports two
/// forms (following the same convention as Cargo's `[alias]`):
///
/// **String form** — tokenized via `split_whitespace` (no shell-style quoting):
/// ```toml
/// [scripts]
/// Backfill = "--function scripts.Backfill -- --verbose=true"
/// ```
///
/// **Array form** — each element is one argument, so values with spaces work:
/// ```toml
/// [scripts]
/// Backfill = ["--function", "scripts.Backfill", "--", "--name", "Ada Lovelace"]
/// ```
fn parse_script_body(tokens: &[String]) -> Result<ScriptExpansion> {
    let mut function = None;
    let mut extra_args = Vec::new();
    let mut after_separator = false;
    let mut i = 0;

    while i < tokens.len() {
        let token = tokens[i].as_str();
        // BEP-027 §"Scripts in `baml.toml`": only the *first* `--` is the
        // separator; subsequent literal `--` tokens pass through as
        // ordinary post-`--` content so a script can legitimately pass a
        // `--` token to the target if it wants.
        if token == "--" && !after_separator {
            after_separator = true;
            i += 1;
            continue;
        }
        if after_separator {
            extra_args.push(token.to_string());
            i += 1;
            continue;
        }

        // Pre-separator: only `--function <value>` is recognized as a
        // run-verb flag inside a script body in v1. Other run-verb flags
        // (`--json-args`, `--verbose`, etc.) are deliberately rejected
        // here rather than silently dropped — the toml loader is the
        // last chance to surface a typo before the script runs.
        if token == "--function" {
            i += 1;
            if i < tokens.len() {
                function = Some(tokens[i].clone());
                i += 1;
            } else {
                anyhow::bail!("Script body has --function without a value");
            }
            continue;
        }

        if let Some(stripped) = token.strip_prefix("--") {
            anyhow::bail!(
                "Unknown run-verb flag `--{stripped}` in script body. \
                 Only `--function <name>` is recognized before `--`; \
                 put target arguments after `--`."
            );
        }

        anyhow::bail!(
            "Unexpected token `{token}` in script body. \
             Script bodies have the shape `[--function <name>] [-- <target-args>...]`."
        );
    }

    Ok(ScriptExpansion {
        function,
        extra_args,
    })
}

// ============================================================================
// CLI Args
// ============================================================================

#[derive(Args, Clone, Debug)]
pub struct RunArgs {
    /// Target: namespace name to run its `main`.
    /// If omitted, runs `main` in the root namespace.
    #[arg(value_name = "TARGET")]
    pub target: Option<String>,

    /// Call a specific function directly (e.g. llm.Summarize or just Summarize).
    #[arg(long)]
    pub function: Option<String>,

    /// Evaluate a BAML expression. Use -e @file to read from a file, -e - for stdin.
    #[arg(short = 'e')]
    pub expression: Option<String>,

    /// JSON arguments: inline JSON string, @file, or - for stdin.
    #[arg(long)]
    pub json_args: Option<String>,

    /// List runnable targets (scripts, namespace mains, functions).
    #[arg(long)]
    pub list: bool,

    /// Output format: debug (default) or json.
    #[arg(long = "output-format", default_value = "debug")]
    pub output_format: OutputFormat,

    /// Write run logs to a file.
    #[arg(long)]
    pub log_file: Option<PathBuf>,

    /// Verbose output.
    #[arg(long)]
    pub verbose: bool,

    /// Show help for the run verb, or auto-derived help for the target.
    #[arg(long, short = 'h')]
    pub help: bool,

    /// Project root directory.
    #[arg(long, default_value = ".")]
    pub from: PathBuf,

    /// Arguments passed to the target function (after `--` separator).
    /// These are parsed as auto-CLI flags derived from the function signature.
    #[arg(last = true)]
    pub target_args: Vec<String>,
}

pub use baml_exec::OutputFormat;

// ============================================================================
// Main entry point
// ============================================================================

impl RunArgs {
    fn emit_format_hint_if_needed(needs_format_hint: bool) {
        if needs_format_hint {
            eprintln!("{FORMAT_HINT}");
        }
    }

    /// Print a `[verbose]`-prefixed line when `--verbose` is set; no-op otherwise.
    fn vlog(&self, args: std::fmt::Arguments<'_>) {
        if self.verbose {
            eprintln!("[verbose] {args}");
        }
    }

    /// Collect diagnostics on `db` and bail with `bail_context` if any are errors.
    /// Warnings are surfaced only in verbose mode.
    fn check_project_diagnostics(&self, db: &ProjectDatabase, bail_context: &str) -> Result<()> {
        let project = db
            .get_project()
            .ok_or_else(|| anyhow!("No project context"))?;
        let source_files = db.get_source_files();
        let diagnostics = baml_project::collect_diagnostics(db, project, &source_files);

        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        let warnings: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .collect();

        let needs_sources = !errors.is_empty() || (self.verbose && !warnings.is_empty());
        let (sources, file_paths) = if needs_sources {
            let mut sources = HashMap::new();
            let mut file_paths = HashMap::new();
            for sf in &source_files {
                let file_id = sf.file_id(db);
                sources.insert(file_id, sf.text(db).to_string());
                file_paths.insert(file_id, sf.path(db));
            }
            (sources, file_paths)
        } else {
            (HashMap::new(), HashMap::new())
        };

        if self.verbose && !warnings.is_empty() {
            let rendered = render::render_diagnostics(
                &warnings.iter().copied().cloned().collect::<Vec<_>>(),
                &sources,
                &file_paths,
                &render::RenderConfig::cli(),
            );
            eprintln!("{rendered}");
        }

        if !errors.is_empty() {
            let rendered = render::render_diagnostics(
                &errors.iter().copied().cloned().collect::<Vec<_>>(),
                &sources,
                &file_paths,
                &render::RenderConfig::cli(),
            );
            eprintln!("{rendered}");
            anyhow::bail!("{bail_context}");
        }
        Ok(())
    }

    /// Compile `db` to bytecode and build a `BexEngine`.
    fn compile_to_engine(
        &self,
        db: &ProjectDatabase,
        event_sink: Option<Arc<dyn bex_events::EventSink>>,
        argv: Vec<String>,
    ) -> Result<BexEngine> {
        let bytecode = baml_compiler2_emit::generate_project_bytecode(
            db,
            &baml_compiler2_emit::CompileOptions {
                emit_test_cases: false,
            },
        )
        .map_err(|e| anyhow!("Compilation failed: {e:?}"))?;
        BexEngine::new(
            bytecode,
            Arc::new(sys_native::SysOps::native()),
            event_sink,
            argv,
        )
        .map_err(|e| anyhow!("Failed to create engine: {e:?}"))
    }

    pub fn run(&self) -> Result<crate::ExitCode> {
        // BEP-027 §"Target resolution": `--function <ns>.<name>` and
        // `-e '<expr>'` are alternative dispatch modes that "replace the
        // positional target entirely." Either of them is mutually
        // exclusive with both the other and with a positional target.
        // Reject the conflict up front instead of silently preferring
        // one and ignoring the rest.
        let dispatch_modes: &[(&str, bool)] = &[
            ("`<target>`", self.target.is_some()),
            ("`--function`", self.function.is_some()),
            ("`-e`", self.expression.is_some()),
        ];
        let used: Vec<&str> = dispatch_modes
            .iter()
            .filter_map(|(name, given)| given.then_some(*name))
            .collect();
        if used.len() > 1 {
            anyhow::bail!(
                "{} are mutually exclusive dispatch modes — pick one.",
                used.join(" and ")
            );
        }

        // `--json-args` feeds typed parameters of the resolved function.
        // Expression mode has no function signature to bind against, so
        // `--json-args` is meaningless there. Reject the combination
        // rather than silently dropping the JSON, which would be a
        // particularly painful footgun in a CI pipeline.
        if self.expression.is_some() && self.json_args.is_some() {
            anyhow::bail!(
                "`-e` is not compatible with `--json-args`. \
                 Expression mode has no function signature to bind JSON keys \
                 to — inline the values in the expression instead."
            );
        }

        let event_sink: Option<Arc<dyn bex_events::EventSink>> =
            Some(if let Some(path) = self.log_file.as_ref() {
                self.vlog(format_args!("Writing logs to {}", path.display()));
                bex_events_native::start(path.clone())
            } else {
                bex_events_native::start_stderr()
            });

        if self.help && (self.function.is_none() && self.target.is_none()) {
            Self::print_run_help();
            return Ok(crate::ExitCode::Success);
        }

        if let Some(expr_source) = &self.expression {
            // BEP-027 §"`baml.argv`": `argv[1]` for `-e` is "the
            // expression source". We load the text up front (only once —
            // `-e -` reads stdin and the engine compile re-uses the same
            // string) and use the loaded body for both `argv[1]` and the
            // synthetic compilation unit.
            let expr_body = load_expression_source(expr_source)?;
            return self.run_expression(&expr_body, event_sink);
        }

        let argv = self.build_argv();

        let (db, mut engine, needs_format_hint) =
            if self.target.as_ref().is_some_and(|t| t.ends_with(".baml")) {
                self.load_and_compile_standalone(
                    self.target.as_ref().unwrap(),
                    event_sink.clone(),
                    argv.clone(),
                )?
            } else {
                self.load_and_compile(event_sink.clone(), argv.clone())?
            };
        let _ = db; // keep db alive for engine lifetime
        Self::emit_format_hint_if_needed(needs_format_hint);

        // BEP-027 §"`baml.argv`": patch `argv[1]` post-compile so it
        // matches the spec's "however the user named the target" rule
        // using engine-canonical names rather than the user's raw input.
        //
        //   - Root `main` (no positional/function/expr) → filepath of
        //     the file containing `main`.
        //   - `--function` → the qualified function name with the
        //     `user.` package prefix stripped, so `--function
        //     user.llm.X` and `--function llm.X` both surface as
        //     `argv[1] = "llm.X"`.
        if self.target.is_none() && self.function.is_none() && self.expression.is_none() {
            if let Some(main_info) = engine.find_user_function("main") {
                if !main_info.source_file.is_empty() {
                    let mut patched = argv.clone();
                    if patched.len() >= 2 {
                        patched[1] = main_info.source_file.clone();
                        engine.set_argv(patched);
                    }
                }
            }
        } else if let Some(func) = &self.function {
            if let Some(info) = engine.find_user_function(func) {
                let display = info.display_name;
                if display != *func && argv.len() >= 2 {
                    let mut patched = argv.clone();
                    patched[1] = display;
                    engine.set_argv(patched);
                }
            }
        }

        let toml_content = std::fs::read_to_string(self.from.join("baml.toml")).unwrap_or_default();
        let scripts = Self::parse_scripts(&toml_content);
        let namespaces = collect_namespaces(&engine);

        if self.list {
            return Self::print_list(&engine, &scripts, &namespaces, &self.output_format);
        }

        self.validate_scripts(&engine, &scripts, &namespaces, &toml_content)?;

        let resolved = self.resolve_target(&engine, &scripts)?;

        let (function_name, effective_target_args, was_script) = match resolved {
            ResolvedTarget::Function(name) => (name, self.target_args.clone(), false),
            ResolvedTarget::Script(expansion) => {
                let func = expansion.function.unwrap_or_else(|| "main".to_string());
                if engine.find_user_function(&func).is_none() {
                    // Script body's --function target is unresolvable.
                    // `validate_scripts` already catches this at load time;
                    // this is defensive in case validation is bypassed.
                    return Err(Self::function_not_found_error(&engine, &func));
                }
                let mut merged_args = expansion.extra_args;
                merged_args.extend(self.target_args.iter().cloned());
                (func, merged_args, true)
            }
        };

        // BEP-027 §"`baml.argv`": for script-alias dispatch, `argv[1]`
        // surfaces the *post-expansion* canonical name rather than the
        // script alias. Script aliases are transparent indirection — the
        // program reports what actually ran, using the same rules the
        // direct (non-aliased) dispatch path uses:
        //
        //   - Script expands to root `main` (no `--function` in body) →
        //     filepath of the file containing `main`, matching `baml
        //     run` (no target).
        //   - Script expands to a named function → the function's
        //     display name (`user.` prefix stripped).
        if was_script {
            if let Some(info) = engine.find_user_function(&function_name) {
                let resolved_label = if info.display_name == "main" && !info.source_file.is_empty()
                {
                    // Root main case: use the file path so script-aliased
                    // and direct invocations of root main produce the
                    // same `argv[1]`.
                    info.source_file
                } else {
                    info.display_name
                };
                let mut patched: Vec<String> = engine.argv().to_vec();
                if patched.len() >= 2 {
                    patched[1] = resolved_label;
                    engine.set_argv(patched);
                }
            }
        }

        let func_info = engine
            .find_user_function(&function_name)
            .ok_or_else(|| anyhow!("Function `{function_name}` not found"))?;

        // BEP-027 §"Auto-CLI conventions": `help` is reserved at entry-point
        // resolution. Reject before printing help so a target declaring a
        // `help` parameter doesn't get its `--help <string>` rendered as if
        // valid. The same check runs again inside dispatch_target as a
        // safety net for non-help invocations.
        baml_exec::validate_help_param(&engine, &function_name)?;

        // BEP-027 §"What `baml pack` changes" reserves `--help` only on typed
        // targets — parameterless `main()` owns its full argv. Mirror that
        // here so `baml run -- --help` reaches a parameterless `main()` via
        // `baml.argv` instead of being intercepted as auto-derived help.
        // Run-verb `--help` (before `--`) always wins, since it's a verb flag.
        let target_is_typed = !func_info.param_names.is_empty();
        if self.help
            || (target_is_typed
                && effective_target_args.len() == 1
                && effective_target_args[0] == "--help")
        {
            Self::print_target_help(&function_name, &func_info);
            return Ok(crate::ExitCode::Success);
        }

        let json_args = match &self.json_args {
            Some(source) => Some(baml_exec::load_json_source(source)?),
            None => None,
        };

        self.vlog(format_args!("Calling {function_name}"));

        let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;
        let engine = Arc::new(engine);
        let output_format = self.output_format;
        let start = std::time::Instant::now();
        let dispatch_result = rt.block_on(baml_exec::dispatch_target(
            Arc::clone(&engine),
            &function_name,
            &effective_target_args,
            json_args,
            output_format,
        ));

        self.vlog(format_args!("Completed in {:.2?}", start.elapsed()));

        if let Some(sink) = &event_sink {
            sink.flush();
        }

        match dispatch_result {
            Ok(baml_exec::DispatchResult::Ok) => Ok(crate::ExitCode::Success),
            Ok(baml_exec::DispatchResult::TargetError) => Ok(crate::ExitCode::TargetError),
            // `baml.sys.exit(code)` short-circuits the engine; honor the user's
            // code as the process exit code, clamped to the C `int` range.
            Ok(baml_exec::DispatchResult::Exit(code)) => {
                std::process::exit(baml_exec::clamp_exit_code(code));
            }
            Err(e) => {
                eprintln!("Error: {e:#}");
                Ok(crate::ExitCode::TargetError)
            }
        }
    }

    /// Build the argv vector exposed to BAML via `baml.sys.argv()`.
    ///
    /// Per BEP-027 §"`baml.argv`":
    ///   [0] = path to the `baml` executable
    ///   [1] = entry path (resolved below)
    ///   [2+] = user tokens after `--`, verbatim
    ///
    /// `argv[1]` resolution by dispatch mode:
    ///
    /// - `-e`            → placeholder (the raw `-e` argument). Callers
    ///   should use [`Self::build_argv_for_expression`] with the already-
    ///   loaded source text so `argv[1]` is the **expression source** per
    ///   spec, not the `@path` or `-` reference.
    /// - `<file.baml>`   → the canonicalized absolute path.
    /// - `<namespace>`   → the namespace name verbatim (e.g. `"eval"`).
    /// - `<script>`      → the script name as placeholder; patched
    ///   **post-resolution** to the canonical function name the alias
    ///   expanded to (e.g. `baml run dev` where `dev = "--function
    ///   llm.Foo"` → `argv[1] = "llm.Foo"`). This treats script aliases
    ///   as transparent indirection — the program sees what actually ran.
    /// - `--function`    → placeholder; patched post-compile to the
    ///   engine-canonical display name (drops any `user.` prefix the
    ///   user spelled).
    /// - no target       → placeholder `"main"`; patched post-compile to
    ///   the filepath of the file containing `function main`.
    fn build_argv(&self) -> Vec<String> {
        let executable = std::env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "baml".to_string());

        let entry = if let Some(expr) = &self.expression {
            // Expression mode placeholder. Callers that have already
            // resolved the expression body (post-`load_expression_source`)
            // should use `build_argv_for_expression` instead so `argv[1]`
            // carries the actual source text rather than the `@path` or
            // `-` reference.
            expr.clone()
        } else if let Some(func) = &self.function {
            func.clone()
        } else if let Some(target) = &self.target {
            // For `.baml` file targets, resolve to an absolute path so argv[1]
            // is stable regardless of the caller's cwd.
            if target.ends_with(".baml") {
                std::fs::canonicalize(target)
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| target.clone())
            } else {
                target.clone()
            }
        } else {
            "main".to_string()
        };

        let mut argv = vec![executable, entry];
        argv.extend(self.target_args.iter().cloned());
        argv
    }

    /// Variant of [`Self::build_argv`] that uses the resolved expression
    /// body (already de-referenced from `@file` / stdin) for `argv[1]`.
    /// See BEP-027 §"`baml.argv`": `argv[1]` for `-e` is "the expression
    /// source" — the contents the user actually evaluates.
    fn build_argv_for_expression(&self, expr_body: &str) -> Vec<String> {
        let executable = std::env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "baml".to_string());

        let mut argv = vec![executable, expr_body.to_string()];
        argv.extend(self.target_args.iter().cloned());
        argv
    }

    /// Load the project, check diagnostics, compile to bytecode, create engine.
    fn load_and_compile(
        &self,
        event_sink: Option<Arc<dyn bex_events::EventSink>>,
        argv: Vec<String>,
    ) -> Result<(ProjectDatabase, BexEngine, bool)> {
        let (db, from, baml_files) = load_project_from(&self.from)?;
        self.vlog(format_args!("Loading project from {}", from.display()));
        if baml_files.is_empty() {
            anyhow::bail!("No .baml files found in {}", from.display());
        }
        self.vlog(format_args!("Found {} .baml file(s)", baml_files.len()));
        let needs_format_hint = baml_files.iter().any(|path| {
            std::fs::read_to_string(path)
                .map(|source| source_needs_format_hint(&source))
                .unwrap_or(false)
        });

        self.check_project_diagnostics(&db, "Cannot run: compilation errors found")?;
        self.vlog(format_args!("Compiling..."));
        let engine = self.compile_to_engine(&db, event_sink, argv)?;
        self.vlog(format_args!(
            "Compiled {} user function(s)",
            engine.user_functions().len()
        ));
        Ok((db, engine, needs_format_hint))
    }

    /// Load a single .baml file in hermetic standalone mode.
    ///
    /// Does NOT load the surrounding project — only the specified file.
    /// The same file runs identically on any machine regardless of project context.
    fn load_and_compile_standalone(
        &self,
        file_path: &str,
        event_sink: Option<Arc<dyn bex_events::EventSink>>,
        argv: Vec<String>,
    ) -> Result<(ProjectDatabase, BexEngine, bool)> {
        let canonical = std::fs::canonicalize(Path::new(file_path))
            .with_context(|| format!("File not found: {file_path}"))?;
        self.vlog(format_args!(
            "Standalone mode: loading {}",
            canonical.display()
        ));

        let content = std::fs::read_to_string(&canonical)
            .with_context(|| format!("Failed to read {}", canonical.display()))?;
        let needs_format_hint = source_needs_format_hint(&content);

        // Project root is the file's parent so relative imports resolve.
        let parent = canonical.parent().unwrap_or_else(|| Path::new("."));

        let mut db = ProjectDatabase::new();
        db.set_project_root(parent);
        db.add_or_update_file(&canonical, &content);

        self.check_project_diagnostics(
            &db,
            &format!("Cannot run: compilation errors in {file_path}"),
        )?;
        let engine = self.compile_to_engine(&db, event_sink, argv)?;
        self.vlog(format_args!(
            "Compiled {} function(s) from standalone file",
            engine.user_functions().len()
        ));
        Ok((db, engine, needs_format_hint))
    }

    // ========================================================================
    // Expression mode (-e)
    // ========================================================================

    /// Evaluate a BAML expression.
    ///
    /// Wraps the expression in a synthetic `function $expr_main() { <body> }`
    /// and compiles/runs it. If inside a project, project context is available.
    ///
    /// `expr_body` is the resolved expression text — already de-referenced
    /// from inline / `@file` / stdin by the caller. We avoid re-reading
    /// because `-e -` reads stdin once.
    fn run_expression(
        &self,
        expr_body: &str,
        event_sink: Option<Arc<dyn bex_events::EventSink>>,
    ) -> Result<crate::ExitCode> {
        self.vlog(format_args!(
            "Expression mode: evaluating {} byte(s)",
            expr_body.len()
        ));

        // `-> unknown` lets any return type through.
        let synthetic = format!("function baml_run_expr_main__() -> unknown {{\n{expr_body}\n}}");

        // Default --from is "." (cwd) which always exists. If user passed an
        // explicit path that can't be resolved, that's a hard error.
        let from = match std::fs::canonicalize(&self.from) {
            Ok(path) => Some(path),
            Err(_) if self.from == Path::new(".") => None,
            Err(e) => anyhow::bail!("Cannot resolve --from path `{}`: {e}", self.from.display()),
        };

        let mut db = ProjectDatabase::new();
        let has_explicit_project = from
            .as_ref()
            .is_some_and(|f| f.join("baml.toml").exists() || f.join("baml_src").exists());

        let project_root = if has_explicit_project {
            let root = from.as_ref().unwrap();
            db.set_project_root(root);
            let src_dir = if root.join("baml_src").exists() {
                root.join("baml_src")
            } else {
                root.clone()
            };
            let baml_files = discover_baml_files(&src_dir);
            for file_path in &baml_files {
                if let Ok(content) = std::fs::read_to_string(file_path) {
                    db.add_or_update_file(file_path, &content);
                }
            }
            self.vlog(format_args!(
                "Project context: loaded {} file(s)",
                baml_files.len()
            ));
            root.clone()
        } else {
            let tmp = std::env::temp_dir().join("baml_expr");
            std::fs::create_dir_all(&tmp).ok();
            db.set_project_root(&tmp);
            self.vlog(format_args!(
                "Project context: none (standalone expression)"
            ));
            tmp
        };

        db.add_or_update_file(&project_root.join("__expr__.baml"), &synthetic);

        self.check_project_diagnostics(&db, "Cannot evaluate expression: compilation errors")?;
        // BEP-027 §"`baml.argv`": `argv[1]` for `-e` is "the expression
        // source" — the loaded body text, not the `@path` reference. This
        // matches the inline case: `-e '2 + 2'` and `-e @file` (with
        // `file` containing `2 + 2`) produce the same argv.
        let engine = self.compile_to_engine(
            &db,
            event_sink.clone(),
            self.build_argv_for_expression(expr_body),
        )?;

        let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;
        let engine = Arc::new(engine);
        let return_type = engine
            .function_return_type("baml_run_expr_main__")
            .unwrap_or(bex_engine::Ty::Null {
                attr: baml_type::TyAttr::default(),
            });
        let output_format = self.output_format;
        let result: std::result::Result<(), bex_engine::EngineError> = rt.block_on(async {
            let engine_for_call = Arc::clone(&engine);
            let value = engine_for_call
                .call_function(
                    "baml_run_expr_main__",
                    vec![],
                    FunctionCallContextBuilder::new(CallId::next()).build(),
                    true,
                )
                .await?;
            if !matches!(return_type, bex_engine::Ty::Void { .. }) {
                if let Err(e) =
                    baml_exec::write_output(&engine, value, &return_type, output_format).await
                {
                    eprintln!("Error: failed to serialize output: {e}");
                }
            }
            Ok(())
        });

        if let Some(sink) = &event_sink {
            sink.flush();
        }

        match result {
            Ok(()) => Ok(crate::ExitCode::Success),
            Err(bex_engine::EngineError::Exit { code }) => {
                std::process::exit(baml_exec::clamp_exit_code(code));
            }
            Err(e) => {
                eprintln!("Error: {e:#}");
                Ok(crate::ExitCode::TargetError)
            }
        }
    }

    // ========================================================================
    // Target resolution
    // ========================================================================

    /// Resolve the run target to a qualified function name the engine understands.
    ///
    /// Resolution cascade:
    /// 1. `--function` flag → direct function call
    /// 2. No target → root namespace's `main`
    /// 3. Target ends in `.baml` → hermetic standalone
    /// 4. Target matches a `[scripts]` entry in `baml.toml` → expand alias
    /// 5. Target matches a namespace with a `main` → that namespace's main
    /// 6. Otherwise → error with suggestions
    fn resolve_target(
        &self,
        engine: &BexEngine,
        scripts: &HashMap<String, Vec<String>>,
    ) -> Result<ResolvedTarget> {
        if let Some(func) = &self.function {
            if engine.find_user_function(func).is_some() {
                return Ok(ResolvedTarget::function(func.clone()));
            }
            // `--function` only dispatches to functions, so the
            // suggestion set is functions — not scripts/namespaces/files.
            return Err(Self::function_not_found_error(engine, func));
        }

        match &self.target {
            None => {
                if engine.function_exists("main") {
                    Ok(ResolvedTarget::function("main".to_string()))
                } else {
                    anyhow::bail!(
                        "No `main` function found in the root namespace.\n\
                         Use `baml run --function <name>` to call a specific function,\n\
                         or `baml run --list` to see available targets."
                    );
                }
            }
            Some(target) => {
                if target.ends_with(".baml") {
                    if engine.function_exists("main") {
                        return Ok(ResolvedTarget::function("main".to_string()));
                    }
                    anyhow::bail!(
                        "Standalone file `{target}` has no `main` function.\n\
                         Use `baml run --function <name>` to call a specific function."
                    );
                }

                if let Some(script_tokens) = scripts.get(target.as_str()) {
                    self.vlog(format_args!(
                        "Expanding script `{target}`: {script_tokens:?}"
                    ));
                    return Ok(ResolvedTarget::Script(parse_script_body(script_tokens)?));
                }

                let ns_main = format!("{target}.main");
                if engine.function_exists(&ns_main) {
                    return Ok(ResolvedTarget::function(ns_main));
                }

                Err(Self::target_not_found_error_in(
                    scripts, engine, target, &self.from,
                ))
            }
        }
    }

    /// Parse `[scripts]` from raw `baml.toml` content.
    ///
    /// Emits a stderr warning on TOML parse failures so a typo in
    /// `baml.toml` doesn't silently disable every script. A missing
    /// `baml.toml` (empty content) is normal and stays quiet.
    fn parse_scripts(content: &str) -> HashMap<String, Vec<String>> {
        let trimmed = content.trim();
        let table = match content.parse::<toml::Table>() {
            Ok(t) => t,
            Err(e) => {
                if !trimmed.is_empty() {
                    eprintln!(
                        "warning: failed to parse `baml.toml` ({e}); [scripts] entries will be ignored"
                    );
                }
                return HashMap::new();
            }
        };
        let Some(scripts) = table.get("scripts").and_then(|v| v.as_table()) else {
            return HashMap::new();
        };
        scripts
            .iter()
            .filter_map(|(k, v)| {
                // String form: tokenized via split_whitespace (like Cargo aliases).
                if let Some(s) = v.as_str() {
                    return Some((
                        k.clone(),
                        s.split_whitespace().map(|t| t.to_string()).collect(),
                    ));
                }
                // Array form: each element is one argument verbatim.
                if let Some(arr) = v.as_array() {
                    let tokens: Vec<String> = arr
                        .iter()
                        .filter_map(|item| item.as_str().map(|s| s.to_string()))
                        .collect();
                    if !tokens.is_empty() {
                        return Some((k.clone(), tokens));
                    }
                }
                None
            })
            .collect()
    }

    /// Find the 1-based line number of a `[scripts]` key in `baml.toml`.
    fn find_script_line(content: &str, key: &str) -> Option<usize> {
        for (i, line) in content.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with(key) && trimmed[key.len()..].trim_start().starts_with('=') {
                return Some(i + 1);
            }
            if trimmed.starts_with(&format!("\"{key}\""))
                && trimmed[key.len() + 2..].trim_start().starts_with('=')
            {
                return Some(i + 1);
            }
        }
        None
    }

    /// Format a script error with file:line reference when available.
    fn script_error(toml_path: &Path, content: &str, name: &str, msg: &str) -> String {
        match Self::find_script_line(content, name) {
            Some(line) => format!("{}:{line}: [scripts] `{name}`: {msg}", toml_path.display()),
            None => format!("{}: [scripts] `{name}`: {msg}", toml_path.display()),
        }
    }

    /// Validate `[scripts]` entries at load time per BEP-027.
    fn validate_scripts(
        &self,
        engine: &BexEngine,
        scripts: &HashMap<String, Vec<String>>,
        namespaces: &HashSet<String>,
        toml_content: &str,
    ) -> Result<()> {
        if scripts.is_empty() {
            return Ok(());
        }

        let toml_path = self.from.join("baml.toml");
        let mut errors: Vec<String> = Vec::new();

        for (name, tokens) in scripts {
            if RESERVED_VERBS.contains(&name.as_str()) {
                errors.push(Self::script_error(
                    &toml_path,
                    toml_content,
                    name,
                    "name is a reserved verb and cannot be used as a script name",
                ));
                continue;
            }

            if namespaces.contains(name) {
                let loc = Self::find_script_line(toml_content, name)
                    .map(|l| format!("{}:{l}", toml_path.display()))
                    .unwrap_or_else(|| toml_path.display().to_string());
                eprintln!(
                    "warning: {loc}: [scripts] `{name}` shadows namespace `{name}` — \
                     the script takes precedence"
                );
            }

            match parse_script_body(tokens) {
                Ok(expansion) => {
                    let target_func = if let Some(func) = &expansion.function {
                        if engine.find_user_function(func).is_none() {
                            errors.push(Self::script_error(
                                &toml_path,
                                toml_content,
                                name,
                                &format!("--function target `{func}` not found"),
                            ));
                            None
                        } else {
                            Some(func.as_str())
                        }
                    } else if engine.function_exists("main") {
                        Some("main")
                    } else {
                        None
                    };

                    if let Some(func_name) = target_func
                        && let Ok(params) = engine.function_params(func_name)
                    {
                        let param_names: Vec<String> =
                            params.iter().map(|(n, _, _)| (*n).to_string()).collect();
                        let flag_keys = extract_flag_keys(&expansion.extra_args);
                        for flag in flag_keys.iter().filter(|k| !param_names.contains(k)) {
                            errors.push(Self::script_error(
                                &toml_path,
                                toml_content,
                                name,
                                &format!(
                                    "unknown parameter `--{flag}` for function `{func_name}` \
                                     (available: {})",
                                    param_names.join(", ")
                                ),
                            ));
                        }
                    }
                }
                Err(e) => {
                    errors.push(Self::script_error(
                        &toml_path,
                        toml_content,
                        name,
                        &e.to_string(),
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            let joined = errors.join("\n  ");
            anyhow::bail!("Invalid [scripts] in baml.toml:\n  {joined}");
        }
    }

    fn target_not_found_error(
        scripts: &HashMap<String, Vec<String>>,
        engine: &BexEngine,
        name: &str,
    ) -> anyhow::Error {
        Self::target_not_found_error_in(scripts, engine, name, Path::new("."))
    }

    /// Error for an unknown `--function` argument. Suggestion candidates
    /// are **functions only** — scripts and namespaces aren't reachable
    /// via `--function`, so suggesting them is misleading. Spec §"Target
    /// resolution"'s did-you-mean rule mixes three sets, but that's for
    /// *positional* targets (script / namespace / file). The `--function`
    /// dispatch path has its own one-set candidate space.
    fn function_not_found_error(engine: &BexEngine, name: &str) -> anyhow::Error {
        let mut candidates: Vec<String> = engine
            .user_functions()
            .into_iter()
            .map(|f| f.display_name)
            .collect();
        candidates.sort();
        candidates.dedup();

        let suggestions: Vec<&str> = candidates
            .iter()
            .filter(|c| {
                c.contains(name) || name.contains(c.as_str()) || strsim::jaro_winkler(c, name) > 0.7
            })
            .take(5)
            .map(String::as_str)
            .collect();

        if suggestions.is_empty() {
            anyhow!(
                "Function `{name}` not found.\n\
                 Use `baml run --list` to see available targets."
            )
        } else {
            anyhow!(
                "Function `{name}` not found. Did you mean one of:\n{}",
                suggestions
                    .iter()
                    .map(|s| format!("  - {s}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        }
    }

    fn target_not_found_error_in(
        scripts: &HashMap<String, Vec<String>>,
        engine: &BexEngine,
        name: &str,
        cwd: &Path,
    ) -> anyhow::Error {
        let namespaces = collect_namespaces(engine);
        struct Candidate {
            name: String,
            label: String,
        }

        let mut candidates: Vec<Candidate> = Vec::new();
        for script_name in scripts.keys() {
            candidates.push(Candidate {
                name: script_name.clone(),
                label: format!("{script_name} (script)"),
            });
        }
        for ns in namespaces {
            candidates.push(Candidate {
                name: ns.clone(),
                label: format!("{ns} (namespace)"),
            });
        }
        for f in &engine.user_functions() {
            candidates.push(Candidate {
                name: f.display_name.clone(),
                label: f.display_name.clone(),
            });
        }
        // BEP-027 §"Target resolution": did-you-mean also matches against
        // `.baml` files resolvable from the current directory.
        for entry in std::fs::read_dir(cwd).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("baml")
                && let Some(file_name) = path.file_name().and_then(|n| n.to_str())
            {
                candidates.push(Candidate {
                    name: file_name.to_string(),
                    label: format!("{file_name} (file)"),
                });
            }
        }
        candidates.sort_by(|a, b| a.name.cmp(&b.name));

        let suggestions: Vec<&str> = candidates
            .iter()
            .filter(|c| {
                c.name.contains(name)
                    || name.contains(&c.name)
                    || strsim::jaro_winkler(&c.name, name) > 0.7
            })
            .take(5)
            .map(|c| c.label.as_str())
            .collect();

        if suggestions.is_empty() {
            anyhow!(
                "No runnable target `{name}` found.\n\
                 Use `baml run --list` to see available targets."
            )
        } else {
            anyhow!(
                "No runnable target `{name}` found. Did you mean one of:\n{}",
                suggestions
                    .iter()
                    .map(|s| format!("  - {s}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        }
    }

    // ========================================================================
    // --list
    // ========================================================================

    fn print_list(
        engine: &BexEngine,
        scripts: &HashMap<String, Vec<String>>,
        namespaces: &HashSet<String>,
        output: &OutputFormat,
    ) -> Result<crate::ExitCode> {
        let mut functions = engine.user_functions();
        functions.sort_by(|a, b| a.display_name.cmp(&b.display_name));

        // Empty-target case must still honor `--output-format json` so
        // scripted consumers always receive a parseable document. The
        // human-readable "No runnable targets found." message is for
        // Debug mode only; JSON mode falls through to emit an empty
        // shape via `print_list_json`.
        if functions.is_empty() && scripts.is_empty() && namespaces.is_empty() {
            if matches!(output, OutputFormat::Debug) {
                println!("No runnable targets found.");
                return Ok(crate::ExitCode::Success);
            }
        }

        match output {
            OutputFormat::Debug => Self::print_list_debug(&functions, scripts, namespaces),
            OutputFormat::Json => Self::print_list_json(&functions, scripts, namespaces),
        }

        Ok(crate::ExitCode::Success)
    }

    fn print_list_debug(
        functions: &[UserFunctionInfo],
        scripts: &HashMap<String, Vec<String>>,
        namespaces: &HashSet<String>,
    ) {
        println!("Available targets:\n");

        if !scripts.is_empty() {
            println!("  Scripts:");
            let mut names: Vec<&String> = scripts.keys().collect();
            names.sort();
            for name in names {
                println!("    {name}");
            }
            println!();
        }

        // BEP-027 §"Auto-CLI conventions" / `--list` describes three
        // categories: scripts, *namespace mains*, and functions. A
        // namespace appears here only when it has a `main` — that's
        // the contract for `baml run <namespace>`. Plain namespaces
        // (no `main`) aren't directly runnable, so we don't list them
        // as a separate category.
        let namespace_mains: Vec<&str> = functions
            .iter()
            .filter(|f| f.display_name.ends_with(".main"))
            .filter_map(|f| {
                let display = &f.display_name;
                display
                    .strip_suffix(".main")
                    .filter(|ns| namespaces.contains(*ns))
            })
            .collect();

        if !namespace_mains.is_empty() {
            println!("  Namespace mains (run via `baml run <name>`):");
            let mut sorted: Vec<&&str> = namespace_mains.iter().collect();
            sorted.sort();
            sorted.dedup();
            for ns in sorted {
                println!("    {ns}");
            }
            println!();
        }

        let mut grouped: std::collections::BTreeMap<String, Vec<&UserFunctionInfo>> =
            std::collections::BTreeMap::new();
        for func in functions {
            let ns = if let Some(dot) = func.display_name.rfind('.') {
                func.display_name[..dot].to_string()
            } else {
                String::new()
            };
            grouped.entry(ns).or_default().push(func);
        }

        if !functions.is_empty() {
            println!("  Functions (call via `baml run --function <name>`):");
            for (ns, funcs) in &grouped {
                let label = if ns.is_empty() { "(root)" } else { ns.as_str() };
                println!("    {label}:");
                for func in funcs {
                    let short_name = if let Some(dot) = func.display_name.rfind('.') {
                        &func.display_name[dot + 1..]
                    } else {
                        &func.display_name
                    };

                    let params: Vec<String> = func
                        .param_names
                        .iter()
                        .zip(func.param_types.iter())
                        .map(|(name, ty)| format!("{name}: {ty}"))
                        .collect();

                    println!(
                        "      {short_name}({}) -> {}",
                        params.join(", "),
                        func.return_type
                    );
                }
            }
            println!();
        }
    }

    fn print_list_json(
        functions: &[UserFunctionInfo],
        scripts: &HashMap<String, Vec<String>>,
        namespaces: &HashSet<String>,
    ) {
        let output = Self::build_list_json_value(functions, scripts, namespaces);
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
        );
    }

    /// Pure value-builder behind [`print_list_json`], split out so the
    /// JSON shape (including the empty-targets case, per BEP-027 §`--list`)
    /// can be unit-tested without capturing stdout. Always returns the
    /// same three keys (`scripts`, `namespace_mains`, `functions`); when
    /// every input is empty the arrays are empty — scripted callers must
    /// still get a parseable document, never the human-readable "No
    /// runnable targets found." text.
    fn build_list_json_value(
        functions: &[UserFunctionInfo],
        scripts: &HashMap<String, Vec<String>>,
        namespaces: &HashSet<String>,
    ) -> serde_json::Value {
        let function_items: Vec<serde_json::Value> = functions
            .iter()
            .map(|f| {
                let params: Vec<serde_json::Value> = f
                    .param_names
                    .iter()
                    .zip(f.param_types.iter())
                    .map(|(name, ty)| {
                        serde_json::json!({
                            "name": name,
                            "type": ty.to_string(),
                        })
                    })
                    .collect();

                serde_json::json!({
                    "name": f.display_name,
                    "qualified_name": f.qualified_name,
                    "params": params,
                    "return_type": f.return_type.to_string(),
                })
            })
            .collect();

        let script_items: Vec<serde_json::Value> = {
            let mut names: Vec<&String> = scripts.keys().collect();
            names.sort();
            names
                .into_iter()
                .map(|name| serde_json::json!({ "name": name }))
                .collect()
        };

        // BEP-027 §"Auto-CLI conventions" / `--list` enumerates
        // "namespace mains" — namespaces that have a `main` function and
        // are therefore valid `baml run <name>` positional targets.
        let namespace_mains_items: Vec<serde_json::Value> = {
            let mut namespace_mains: Vec<String> = functions
                .iter()
                .filter(|f| f.display_name.ends_with(".main"))
                .filter_map(|f| {
                    f.display_name
                        .strip_suffix(".main")
                        .filter(|ns| namespaces.contains(*ns))
                        .map(String::from)
                })
                .collect();
            namespace_mains.sort();
            namespace_mains.dedup();
            namespace_mains
                .into_iter()
                .map(|ns| serde_json::json!({ "name": ns }))
                .collect()
        };

        serde_json::json!({
            "scripts": script_items,
            "namespace_mains": namespace_mains_items,
            "functions": function_items,
        })
    }

    // ========================================================================
    // Per-target --help
    // ========================================================================

    /// Render auto-derived target help via the shared formatter in
    /// `baml_exec`. The invocation example uses `baml run --function ...
    /// -- ` so the example line reproduces the exact `baml run` shell
    /// syntax (the `--` separator hint is implicit in the example).
    fn print_target_help(function_name: &str, func_info: &UserFunctionInfo) {
        let display = function_name.strip_prefix("user.").unwrap_or(function_name);
        let example_prefix = format!("baml run --function {display} -- ");
        baml_exec::print_target_help(function_name, func_info, &example_prefix);
    }

    fn print_run_help() {
        use clap::CommandFactory;
        let mut cmd = crate::commands::RuntimeCli::command();
        for sub in cmd.get_subcommands_mut() {
            if sub.get_name() == "run" {
                let _ = sub.print_help();
                return;
            }
        }
    }
}

// ============================================================================
// Reserved verbs & namespace helpers
// ============================================================================

const FORMAT_HINT: &str = "[INFO] Your code is unformatted, but will continue to run. You can fix this whenever you'd like by running `baml fmt`.";

fn source_needs_format_hint(source: &str) -> bool {
    let options = baml_fmt::FormatOptions::default();
    match baml_fmt::format(source, &options) {
        Ok(formatted) => formatted != source,
        Err(_) => false,
    }
}

/// BEP-027 Appendix A: names that cannot be used as `[scripts]` keys.
const RESERVED_VERBS: &[&str] = &[
    "run", "pack", "test", "repl", "init", "help", "version", "fmt", "lint", "check", "build",
    "generate", "dev", "start", "serve", "add", "remove", "install", "update", "publish",
    "upgrade", "deps", "clean", "config", "info", "search", "new", "doc", "docs",
];

/// Extract the set of namespace prefixes from the engine's function list.
///
/// A function named `foo.Bar` contributes namespace `foo`; `main` (no dot)
/// contributes nothing.
fn collect_namespaces(engine: &BexEngine) -> HashSet<String> {
    engine
        .user_functions()
        .iter()
        .filter_map(|f| {
            f.display_name
                .rfind('.')
                .map(|i| f.display_name[..i].to_string())
        })
        .collect()
}

// ============================================================================
// Script flag extraction (only path that still lives in run_command after
// the rest of the auto-CLI / JSON coercion logic moved into `baml_exec`).
// ============================================================================

/// Extract flag names (`--key value` or `--key=value`) from a token list,
/// skipping bare (non-flag) tokens. Used by `validate_scripts` to check
/// that script-body flag keys reference real parameter names.
fn extract_flag_keys(tokens: &[String]) -> Vec<String> {
    let mut keys = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];
        if let Some(raw) = token.strip_prefix("--") {
            let key = raw.split('=').next().unwrap_or(raw);
            if !key.is_empty() {
                keys.push(key.to_string());
            }
            if !raw.contains('=') {
                i += 1; // skip the value token
            }
        }
        i += 1;
    }
    keys
}

/// Load expression source from -e argument: inline string, @file, or - for stdin.
fn load_expression_source(source: &str) -> Result<String> {
    if source == "-" {
        std::io::read_to_string(std::io::stdin()).context("Failed to read expression from stdin")
    } else if let Some(path) = source.strip_prefix('@') {
        std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read expression file: {path}"))
    } else {
        Ok(source.to_string())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use bex_engine::Ty;

    use super::*;

    fn s(val: &str) -> String {
        val.to_string()
    }

    fn ty_string() -> Ty {
        Ty::String {
            attr: Default::default(),
        }
    }
    fn ty_int() -> Ty {
        Ty::Int {
            attr: Default::default(),
        }
    }
    fn ty_float() -> Ty {
        Ty::Float {
            attr: Default::default(),
        }
    }
    fn ty_bool() -> Ty {
        Ty::Bool {
            attr: Default::default(),
        }
    }

    #[test]
    fn source_needs_format_hint_returns_false_for_formatted_source() {
        let source = "function main() -> string {\n    \"ok\"\n}\n";
        assert!(!source_needs_format_hint(source));
    }

    #[test]
    fn source_needs_format_hint_returns_true_for_unformatted_source() {
        let source = "function main()->string {\n\"ok\"\n}\n";
        assert!(source_needs_format_hint(source));
    }

    #[test]
    fn source_needs_format_hint_returns_false_on_formatter_error() {
        let source = "function main( -> string {";
        assert!(!source_needs_format_hint(source));
    }

    #[test]
    fn format_hint_text_matches_ticket() {
        assert_eq!(
            FORMAT_HINT,
            "[INFO] Your code is unformatted, but will continue to run. You can fix this whenever you'd like by running `baml fmt`."
        );
    }

    // Tests that touch the filesystem build paths under $TMPDIR. Appending
    // pid + a monotonic counter prevents collisions between parallel `cargo
    // test` runs and stale files left behind by a crashed prior run.
    fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("{prefix}_{}_{n}", std::process::id()))
    }

    // ── parse_cli_value ────────────────────────────────────────────

    // ── parse_auto_cli_args ────────────────────────────────────────

    // ── json_to_external ───────────────────────────────────────────

    // ── parse_script_body ──────────────────────────────────────────

    /// Helper: split a string the same way load_scripts does for string-form entries.
    fn tokens(s: &str) -> Vec<String> {
        s.split_whitespace().map(|t| t.to_string()).collect()
    }

    #[test]
    fn test_parse_script_function_only() {
        let expansion = parse_script_body(&tokens("--function scripts.Backfill")).unwrap();
        assert_eq!(expansion.function.as_deref(), Some("scripts.Backfill"));
        assert!(expansion.extra_args.is_empty());
    }

    #[test]
    fn test_parse_script_args_only() {
        let expansion = parse_script_body(&tokens("-- --verbose=true --model gpt-4o")).unwrap();
        assert!(expansion.function.is_none());
        assert_eq!(
            expansion.extra_args,
            vec!["--verbose=true", "--model", "gpt-4o"]
        );
    }

    #[test]
    fn test_parse_script_function_and_args() {
        let expansion = parse_script_body(&tokens("--function F -- --x 1 --y 2")).unwrap();
        assert_eq!(expansion.function.as_deref(), Some("F"));
        assert_eq!(expansion.extra_args, vec!["--x", "1", "--y", "2"]);
    }

    #[test]
    fn test_parse_script_array_form_with_spaces() {
        // Array form: each element is one token, so values with spaces are preserved.
        let arr = vec![
            "--function".to_string(),
            "scripts.Greet".to_string(),
            "--".to_string(),
            "--name".to_string(),
            "Ada Lovelace".to_string(),
        ];
        let expansion = parse_script_body(&arr).unwrap();
        assert_eq!(expansion.function.as_deref(), Some("scripts.Greet"));
        assert_eq!(expansion.extra_args, vec!["--name", "Ada Lovelace"]);
    }

    /// BEP-027 §"Scripts in `baml.toml`": the toml loader type-checks
    /// script bodies. Unknown run-verb flags before `--` are rejected.
    #[test]
    fn test_parse_script_rejects_unknown_pre_separator_flag() {
        let err = parse_script_body(&tokens("--json-args @x.json")).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("--json-args"), "got: {msg}");
        assert!(msg.contains("Only `--function"), "got: {msg}");
    }

    /// Bare tokens before `--` are also rejected (script body grammar is
    /// `[--function <name>] [-- <target-args>]`).
    #[test]
    fn test_parse_script_rejects_bare_pre_separator_token() {
        let err = parse_script_body(&tokens("random_word")).unwrap_err();
        assert!(format!("{err}").contains("Unexpected token"));
    }

    /// Only the *first* `--` is the separator; subsequent literal `--`
    /// tokens in post-separator content pass through unchanged.
    #[test]
    fn test_parse_script_multiple_dash_dash_first_wins() {
        let expansion = parse_script_body(&tokens("--function F -- --x 1 -- --y 2")).unwrap();
        assert_eq!(expansion.function.as_deref(), Some("F"));
        assert_eq!(expansion.extra_args, vec!["--x", "1", "--", "--y", "2"]);
    }

    // ── load_expression_source ─────────────────────────────────────

    #[test]
    fn test_load_expression_inline() {
        let body = load_expression_source("2 + 2").unwrap();
        assert_eq!(body, "2 + 2");
    }

    #[test]
    fn test_load_expression_file() {
        let dir = unique_temp_dir("baml_test_expr");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("expr.baml");
        std::fs::write(&path, "let x = 42\nx").unwrap();

        let source = format!("@{}", path.display());
        let body = load_expression_source(&source).unwrap();
        assert_eq!(body, "let x = 42\nx");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_load_expression_file_missing_is_error() {
        let result = load_expression_source("@/definitely/does/not/exist/expr.baml");
        assert!(result.is_err(), "missing @file must error");
    }

    // ── Regression tests for recently-fixed BEP-027 behaviors ──────────

    /// BEP-027 Appendix A lists `pack` as a reserved verb. A script
    /// trying to use that name should be rejected at load time.
    #[test]
    fn test_reserved_verbs_includes_pack() {
        assert!(RESERVED_VERBS.contains(&"pack"));
    }

    /// BEP-027 Appendix A: the flag is `--output-format`, not `--output`.
    /// (`baml pack` uses `--output` for output path; `--output-format`
    /// names the serialization format on both verbs.)
    #[test]
    fn test_output_format_flag_name() {
        use clap::{CommandFactory, FromArgMatches, Parser};
        // RunArgs declares its own `--help` field (BEP-027 §"Auto-CLI
        // conventions"), so the test wrapper must disable clap's auto
        // `--help` to mirror the real subcommand registration.
        #[derive(Parser)]
        #[command(disable_help_flag = true)]
        struct Wrapper {
            #[command(flatten)]
            args: RunArgs,
        }
        let cmd = Wrapper::command();
        // Long form parses and binds.
        let matches = cmd
            .clone()
            .try_get_matches_from(["run", "--output-format", "json"])
            .unwrap();
        let parsed = Wrapper::from_arg_matches(&matches).unwrap();
        assert!(matches!(parsed.args.output_format, OutputFormat::Json));
        // Old flag name is rejected.
        assert!(
            cmd.clone()
                .try_get_matches_from(["run", "--output", "json"])
                .is_err()
        );
    }

    /// BEP-027: `--output-format` defaults to `debug` under `baml run`
    /// (human reader). Pack defaults to `json` — tested separately.
    #[test]
    fn test_run_output_format_defaults_to_debug() {
        use clap::{CommandFactory, FromArgMatches, Parser};
        #[derive(Parser)]
        #[command(disable_help_flag = true)]
        struct Wrapper {
            #[command(flatten)]
            args: RunArgs,
        }
        let matches = Wrapper::command().try_get_matches_from(["run"]).unwrap();
        let parsed = Wrapper::from_arg_matches(&matches).unwrap();
        assert!(matches!(parsed.args.output_format, OutputFormat::Debug));
    }

    // ── Default-valued `RunArgs` fixture for behavior tests ───────────

    /// Build a `RunArgs` with default everything; tests flip individual
    /// fields. Keeps test bodies focused on the field under test.
    fn run_args() -> RunArgs {
        RunArgs {
            target: None,
            function: None,
            expression: None,
            json_args: None,
            list: false,
            output_format: OutputFormat::Debug,
            log_file: None,
            verbose: false,
            help: false,
            from: PathBuf::from("."),
            target_args: Vec::new(),
        }
    }

    // ── Dispatch-mode mutex (BEP-027 §"Target resolution") ────────────

    /// `--function` and a positional target are mutually exclusive
    /// dispatch modes.
    #[test]
    fn test_run_rejects_target_plus_function() {
        let mut args = run_args();
        args.target = Some("eval".into());
        args.function = Some("X".into());
        let err = args.run().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("mutually exclusive"), "got: {msg}");
        assert!(
            msg.contains("`<target>`") && msg.contains("`--function`"),
            "got: {msg}"
        );
    }

    /// `-e` and a positional target are mutually exclusive.
    #[test]
    fn test_run_rejects_target_plus_expression() {
        let mut args = run_args();
        args.target = Some("eval".into());
        args.expression = Some("2 + 2".into());
        let err = args.run().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("mutually exclusive"), "got: {msg}");
        assert!(
            msg.contains("`<target>`") && msg.contains("`-e`"),
            "got: {msg}"
        );
    }

    /// `-e` and `--function` are mutually exclusive.
    #[test]
    fn test_run_rejects_function_plus_expression() {
        let mut args = run_args();
        args.function = Some("X".into());
        args.expression = Some("2 + 2".into());
        let err = args.run().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("mutually exclusive"), "got: {msg}");
        assert!(
            msg.contains("`--function`") && msg.contains("`-e`"),
            "got: {msg}"
        );
    }

    /// All three dispatch modes together → mutex error names all three.
    #[test]
    fn test_run_rejects_three_dispatch_modes() {
        let mut args = run_args();
        args.target = Some("t".into());
        args.function = Some("F".into());
        args.expression = Some("e".into());
        let err = args.run().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("`<target>`"));
        assert!(msg.contains("`--function`"));
        assert!(msg.contains("`-e`"));
    }

    // ── `-e` + `--json-args` rejection ─────────────────────────────────

    /// Expression mode has no function signature to bind JSON keys to;
    /// silently dropping the JSON would be a footgun in CI pipelines.
    #[test]
    fn test_run_rejects_expression_plus_json_args() {
        let mut args = run_args();
        args.expression = Some("2 + 2".into());
        args.json_args = Some("{}".into());
        let err = args.run().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("`-e`"), "got: {msg}");
        assert!(msg.contains("--json-args"), "got: {msg}");
    }

    // ── `--list` empty-targets JSON shape (BEP-027 §`--list`) ──────────

    /// Scripted consumers expect `--output-format json --list` to always
    /// emit a parseable document. The pre-fix code short-circuited to
    /// `println!("No runnable targets found.")` in the empty case
    /// regardless of `--output-format`, breaking JSON readers. The
    /// empty-targets path now still builds a JSON value with empty
    /// arrays for `scripts`, `namespace_mains`, and `functions`.
    #[test]
    fn list_empty_json_shape_has_all_keys_with_empty_arrays() {
        let scripts: HashMap<String, Vec<String>> = HashMap::new();
        let namespaces: HashSet<String> = HashSet::new();
        let functions: Vec<UserFunctionInfo> = vec![];

        let value = RunArgs::build_list_json_value(&functions, &scripts, &namespaces);

        let obj = value.as_object().expect("top-level must be an object");
        assert!(
            obj.contains_key("scripts")
                && obj.contains_key("namespace_mains")
                && obj.contains_key("functions"),
            "missing keys; got: {value}"
        );
        assert!(obj["scripts"].as_array().unwrap().is_empty());
        assert!(obj["namespace_mains"].as_array().unwrap().is_empty());
        assert!(obj["functions"].as_array().unwrap().is_empty());

        // Round-trips through serde_json — the same property scripted
        // consumers rely on.
        let s = serde_json::to_string(&value).unwrap();
        let _: serde_json::Value = serde_json::from_str(&s).expect("must parse");
    }

    // ── `parse_scripts` warning behavior ───────────────────────────────

    /// A malformed `baml.toml` should leave the scripts map empty (so
    /// `baml run` can still execute a target) but warn so the user knows
    /// their `[scripts]` table isn't being read.
    #[test]
    fn test_parse_scripts_malformed_toml_returns_empty() {
        let result = RunArgs::parse_scripts("[scripts\nbroken");
        assert!(
            result.is_empty(),
            "malformed toml must not silently produce scripts: got {result:?}"
        );
    }

    /// An empty `baml.toml` (no file) is the normal case — no warning,
    /// empty scripts.
    #[test]
    fn test_parse_scripts_empty_input_no_warn_empty_result() {
        assert!(RunArgs::parse_scripts("").is_empty());
    }
}
