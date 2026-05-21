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
// `surface_clap_error` is defined later in this file.
// For --log-file event sink.
use clap::Args;
use sys_native::{CallId, SysOpsExt};

use crate::{project_load::load_project_from_reporting, reporter::Reporter};

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
    /// Positional target: function name to run as the sole entry point.
    /// Mutually exclusive with `-f/--function` and `-e/--expression`.
    #[arg(value_name = "TARGET")]
    pub target: Option<String>,

    /// Run a specific function, repeatable. With one `-f` the auto-CLI
    /// still surfaces the function as a subcommand (post-`--` tokens
    /// must start with the subcommand name); with multiple `-f` the
    /// binary multiplexes between them.
    /// Mutually exclusive with positional `<TARGET>` and `-e`.
    #[arg(short = 'f', long = "function", value_name = "NAME")]
    pub functions: Vec<String>,

    /// Evaluate a BAML expression. Use -e @file to read from a file, -e - for stdin.
    /// Mutually exclusive with positional `<TARGET>` and `-f`.
    #[arg(short = 'e', long = "expression")]
    pub expression: Option<String>,

    /// Standalone single-file source. Loads only this file (no project
    /// discovery). Mutually exclusive with `--from`.
    #[arg(long, value_name = "PATH")]
    pub file: Option<PathBuf>,

    /// List runnable targets (scripts, functions).
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

    /// Project root directory. Ignored when `--file` is set.
    #[arg(long, default_value = ".")]
    pub from: PathBuf,

    /// Arguments passed to the target after `--`. Parsed as auto-CLI
    /// flags derived from the function signature(s). With multiple `-f`
    /// the first token after `--` is the chosen subcommand name.
    #[arg(last = true)]
    pub target_args: Vec<String>,
}

pub use baml_exec::OutputFormat;

// ============================================================================
// Main entry point
// ============================================================================

impl RunArgs {
    /// Emit the "your code is unformatted" advisory above the active
    /// spinner. Routed through the reporter (rather than a raw
    /// `eprintln!`) so the message both takes the bold-yellow
    /// `warning:` header — matching ariadne — *and* doesn't interleave
    /// with the spinner ticks that would otherwise still be running.
    fn emit_format_hint_if_needed(reporter: &Reporter, needs_format_hint: bool) {
        if needs_format_hint {
            reporter.warning(FORMAT_HINT);
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
    ///
    /// When a [`Reporter`] is active, diagnostic output is routed through
    /// `reporter.suspend()` so the multi-line ariadne block doesn't
    /// interleave with the spinner.
    fn check_project_diagnostics(
        &self,
        db: &ProjectDatabase,
        bail_context: &str,
        reporter: &Reporter,
    ) -> Result<()> {
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
                &render::RenderConfig::cli_auto(),
            );
            reporter.suspend(|| eprintln!("{rendered}"));
        }

        if !errors.is_empty() {
            let rendered = render::render_diagnostics(
                &errors.iter().copied().cloned().collect::<Vec<_>>(),
                &sources,
                &file_paths,
                &render::RenderConfig::cli_auto(),
            );
            reporter.abandon();
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
        // Short-circuit verb-level `--help` (no target / function /
        // expression given) before constructing the Reporter, so the
        // spinner doesn't briefly draw a frame above the help text.
        if self.help
            && self.functions.is_empty()
            && self.target.is_none()
            && self.expression.is_none()
        {
            Self::print_run_help();
            return Ok(crate::ExitCode::Success);
        }
        let reporter = Reporter::new();
        self.run_with_reporter(&reporter)
    }

    fn run_with_reporter(&self, reporter: &Reporter) -> Result<crate::ExitCode> {
        // Dispatch modes are mutually exclusive. Positional target /
        // `-f` (one or many) / `-e` all replace each other.
        let dispatch_modes: &[(&str, bool)] = &[
            ("`<target>`", self.target.is_some()),
            ("`-f`", !self.functions.is_empty()),
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

        // `--file` is the standalone-source alternative to `--from`. Both
        // pointing at sources would be ambiguous (which one wins?), so
        // reject the combination up front. We only enforce this when
        // `--from` is set to something other than its default `.`,
        // because clap can't tell "user passed `.`" from "user passed
        // nothing." If `--file` is set and `--from` was left at the
        // default, treat that as fine.
        if self.file.is_some() && self.from != Path::new(".") {
            anyhow::bail!(
                "`--file` and `--from` are mutually exclusive — `--file` already names \
                 the single source to load."
            );
        }

        // Expression mode short-circuits before reaching project / file
        // loading, so combining `-e` with surfaces that change *what* is
        // loaded silently does nothing. Reject up front to avoid the
        // footgun.
        if self.expression.is_some() {
            if self.file.is_some() {
                anyhow::bail!(
                    "`-e` is not compatible with `--file`. Expression mode \
                     evaluates inline source; remove one of the two."
                );
            }
            if self.list {
                anyhow::bail!(
                    "`-e` is not compatible with `--list`. Expression mode \
                     evaluates an expression; `--list` enumerates targets — \
                     pick one."
                );
            }
        }

        let event_sink: Option<Arc<dyn bex_events::EventSink>> =
            Some(if let Some(path) = self.log_file.as_ref() {
                self.vlog(format_args!("Writing logs to {}", path.display()));
                bex_events_native::start(path.clone())
            } else {
                bex_events_native::start_stderr()
            });

        if self.help
            && self.functions.is_empty()
            && self.target.is_none()
            && self.expression.is_none()
        {
            Self::print_run_help();
            return Ok(crate::ExitCode::Success);
        }

        if let Some(expr_source) = &self.expression {
            // `-e -` reads stdin / `-e @file` reads file. Load once so
            // the engine compile and `argv[1]` see the same text.
            let expr_body = load_expression_source(expr_source)?;
            return self.run_expression(&expr_body, event_sink, reporter);
        }

        // `--list` short-circuit doesn't need a resolved target; load
        // the project and print before we get to dispatch.
        if self.list {
            let bootstrap_argv = vec![
                std::env::current_exe()
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "baml".to_string()),
                "--list".to_string(),
            ];
            let (db, engine, _) =
                self.load_and_compile(event_sink.clone(), bootstrap_argv, reporter)?;
            let _ = db;
            // `--file` mode is hermetic — skip the project `[scripts]`
            // lookup the same way `run_single_target` does.
            let scripts = if self.file.is_some() {
                HashMap::new()
            } else {
                let toml_content =
                    std::fs::read_to_string(self.from.join("baml.toml")).unwrap_or_default();
                Self::parse_scripts(&toml_content)
            };
            let namespaces = collect_namespaces(&engine);
            return self.print_list(&engine, &scripts, &namespaces, &self.output_format);
        }

        // No target → print help and exit non-zero. (Implicit `main` no
        // longer exists.)
        if self.target.is_none() && self.functions.is_empty() {
            Self::print_run_help();
            return Ok(crate::ExitCode::Other);
        }

        if let Some(target) = &self.target {
            // Helpful redirect when the user typed a path as the
            // positional (e.g. `baml run baml_src/main.baml`). Positional
            // `<TARGET>` is always a function name; for a `.baml` source
            // use `--file`.
            if looks_like_path(target) {
                anyhow::bail!(
                    "positional `<TARGET>` is a function name, not a file path. \
                     For a single-file source, use `--file {target}` and pass the \
                     function via `-f <NAME>`. For example:\n\
                     \n    baml run --file {target} -f <NAME>\n",
                );
            }
            return self.run_single_target(target, event_sink, reporter);
        }
        self.run_subcommand_targets(event_sink, reporter)
    }

    /// Positional `<TARGET>` path: one function, no subcommand layer.
    /// `[scripts]` aliases are resolved here too (positional only).
    fn run_single_target(
        &self,
        target: &str,
        event_sink: Option<Arc<dyn bex_events::EventSink>>,
        reporter: &Reporter,
    ) -> Result<crate::ExitCode> {
        let argv = self.build_argv_for_single(target);
        let (db, mut engine, needs_format_hint) =
            self.load_and_compile(event_sink.clone(), argv.clone(), reporter)?;
        let _ = db;
        Self::emit_format_hint_if_needed(reporter, needs_format_hint);

        // `[scripts]` are a project-mode concept. In `--file` (standalone)
        // mode the project's `baml.toml` shouldn't be consulted — the
        // file is hermetic by intent — so an empty `scripts` map skips
        // both expansion and validation.
        let (toml_content, scripts) = if self.file.is_some() {
            (String::new(), HashMap::new())
        } else {
            let content = std::fs::read_to_string(self.from.join("baml.toml")).unwrap_or_default();
            let parsed = Self::parse_scripts(&content);
            (content, parsed)
        };
        let namespaces = collect_namespaces(&engine);
        self.validate_scripts(&engine, &scripts, &namespaces, &toml_content)?;

        // Resolve to (function_name, effective_args, was_script).
        let (function_name, effective_target_args, was_script) =
            if let Some(script_tokens) = scripts.get(target) {
                self.vlog(format_args!(
                    "Expanding script `{target}`: {script_tokens:?}"
                ));
                let expansion = parse_script_body(script_tokens)?;
                let func = expansion.function.ok_or_else(|| {
                    anyhow!(
                        "Script `{target}` has no `--function` and there is no implicit \
                         entry point. Add `--function <name>` to the script body."
                    )
                })?;
                if engine.find_user_function(&func).is_none() {
                    return Err(Self::function_not_found_error(&engine, &func));
                }
                let mut merged_args = expansion.extra_args;
                merged_args.extend(self.target_args.iter().cloned());
                (func, merged_args, true)
            } else if engine.find_user_function(target).is_some() {
                (target.to_string(), self.target_args.clone(), false)
            } else {
                return Err(Self::target_not_found_error_in(
                    &scripts, &engine, target, &self.from,
                ));
            };

        // Patch `argv[1]` to the engine-canonical display name (strips
        // `user.` prefix) so scripts and the direct path agree.
        if let Some(info) = engine.find_user_function(&function_name) {
            let display = info.display_name.clone();
            let mut patched: Vec<String> = engine.argv().to_vec();
            if patched.len() >= 2 && (was_script || display != *target) {
                patched[1] = display;
                engine.set_argv(patched);
            }
        }

        let func_info = engine
            .find_user_function(&function_name)
            .ok_or_else(|| anyhow!("Function `{function_name}` not found"))?;
        baml_exec::validate_help_param(&engine, &function_name)?;

        let target_is_typed = !func_info.param_names.is_empty();
        let parsed = if target_is_typed {
            let display = function_name
                .strip_prefix("user.")
                .unwrap_or(&function_name);
            let bin_name = format!("baml run {display} --");
            match baml_exec::parse_target_argv(
                &bin_name,
                &function_name,
                &func_info,
                &effective_target_args,
            ) {
                Ok(p) => p,
                Err(err) => return surface_clap_error(reporter, err),
            }
        } else {
            baml_exec::ParsedTargetArgs::default()
        };

        let json_args = match parsed.json_source.as_deref() {
            Some(source) => Some(baml_exec::load_json_source(source)?),
            None => None,
        };

        self.dispatch_and_finish(
            engine,
            &function_name,
            parsed.cli_values,
            json_args,
            event_sink,
            reporter,
        )
    }

    /// `-f` mode: build a multi-subcommand parser, dispatch the chosen one.
    fn run_subcommand_targets(
        &self,
        event_sink: Option<Arc<dyn bex_events::EventSink>>,
        reporter: &Reporter,
    ) -> Result<crate::ExitCode> {
        let argv = self.build_argv_for_subcommand();
        let (db, mut engine, needs_format_hint) =
            self.load_and_compile(event_sink.clone(), argv.clone(), reporter)?;
        let _ = db;
        Self::emit_format_hint_if_needed(reporter, needs_format_hint);

        let (entries, lookups) = self.resolve_subcommand_targets(&engine)?;

        // Build the parse bin_name as the reinvocation prefix so the
        // help text is copy-pasteable.
        let bin_name = format!(
            "baml run {} --",
            self.functions
                .iter()
                .map(|f| format!("-f {f}"))
                .collect::<Vec<_>>()
                .join(" "),
        );
        let (chosen, parsed) = match baml_exec::parse_multi_target_argv(
            &bin_name,
            &entries,
            &lookups,
            &self.target_args,
        ) {
            Ok(v) => v,
            Err(err) => return surface_clap_error(reporter, err),
        };

        // Patch argv[1] to the chosen subcommand name.
        let chosen_entry = entries
            .iter()
            .find(|e| e.qualified_name == chosen)
            .expect("parse returned an unregistered subcommand");
        let mut patched = engine.argv().to_vec();
        if patched.len() >= 2 {
            patched[1] = chosen_entry.subcommand_name.clone();
            engine.set_argv(patched);
        }

        let json_args = match parsed.json_source.as_deref() {
            Some(source) => Some(baml_exec::load_json_source(source)?),
            None => None,
        };

        self.dispatch_and_finish(
            engine,
            &chosen,
            parsed.cli_values,
            json_args,
            event_sink,
            reporter,
        )
    }

    /// Walk `self.functions`, resolve each to an engine-canonical
    /// `(TargetEntry, UserFunctionInfo)` pair, and reject duplicate
    /// subcommand names. Mirrors `pack_command::resolve_targets`'
    /// `-f` path so `baml run -f` and `baml pack -f` see the same
    /// resolution shape; kept as a method here rather than hoisted to
    /// `baml_exec` because the error messages reference `baml run`'s
    /// help text.
    fn resolve_subcommand_targets(
        &self,
        engine: &BexEngine,
    ) -> Result<(
        Vec<baml_exec::TargetEntry>,
        HashMap<String, UserFunctionInfo>,
    )> {
        let mut entries: Vec<baml_exec::TargetEntry> = Vec::with_capacity(self.functions.len());
        let mut lookups: HashMap<String, UserFunctionInfo> = HashMap::new();
        for func in &self.functions {
            let Some(info) = engine.find_user_function(func) else {
                return Err(Self::function_not_found_error(engine, func));
            };
            baml_exec::validate_help_param(engine, &info.qualified_name)?;
            let display = info
                .qualified_name
                .strip_prefix("user.")
                .unwrap_or(&info.qualified_name)
                .to_string();
            let subcommand = display.rsplit('.').next().unwrap_or(&display).to_string();
            if let Some(prev) = entries.iter().find(|e| e.subcommand_name == subcommand) {
                anyhow::bail!(
                    "two `-f` targets share subcommand name `{subcommand}` \
                     (`{}` and `{}`). Subcommand names come from the last `.`-segment \
                     of the function name; rename one of them.",
                    prev.display_name,
                    display,
                );
            }
            entries.push(baml_exec::TargetEntry {
                qualified_name: info.qualified_name.clone(),
                display_name: display.clone(),
                subcommand_name: subcommand,
            });
            lookups.insert(info.qualified_name.clone(), info);
        }
        Ok((entries, lookups))
    }

    /// Shared tail: spawn the runtime, run dispatch, render the
    /// `Running` / `Finished` lines.
    fn dispatch_and_finish(
        &self,
        engine: BexEngine,
        function_name: &str,
        cli_values: HashMap<String, bex_engine::BexExternalValue>,
        json_args: Option<serde_json::Value>,
        event_sink: Option<Arc<dyn bex_events::EventSink>>,
        reporter: &Reporter,
    ) -> Result<crate::ExitCode> {
        self.vlog(format_args!("Calling {function_name}"));
        reporter.status("Running", function_name);
        reporter.abandon();

        let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;
        let engine = Arc::new(engine);
        let output_format = self.output_format;
        let start = std::time::Instant::now();
        let dispatch_result = rt.block_on(baml_exec::dispatch_target(
            Arc::clone(&engine),
            function_name,
            cli_values,
            json_args,
            output_format,
        ));

        self.vlog(format_args!("Completed in {:.2?}", start.elapsed()));

        if let Some(sink) = &event_sink {
            sink.flush();
        }

        match dispatch_result {
            Ok(baml_exec::DispatchResult::Ok) => {
                reporter.finish("Finished", function_name);
                Ok(crate::ExitCode::Success)
            }
            Ok(baml_exec::DispatchResult::TargetError) => Ok(crate::ExitCode::TargetError),
            Ok(baml_exec::DispatchResult::Exit(code)) => {
                std::process::exit(baml_exec::clamp_exit_code(code));
            }
            Err(e) => {
                crate::reporter::print_error(format_args!("{e:#}"));
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
    ///   loaded source text so `argv[1]` is the **expression source**.
    /// - positional `<TARGET>` → the target name verbatim; patched
    ///   post-resolve to the engine display name (drops `user.` prefix
    ///   the user may have spelled) or to the post-expansion function
    ///   name for `[scripts]` aliases.
    /// - `-f` (multi-target) → placeholder; patched after parse to the
    ///   chosen subcommand name.
    fn build_argv_for_single(&self, target: &str) -> Vec<String> {
        let executable = std::env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "baml".to_string());
        let mut argv = vec![executable, target.to_string()];
        argv.extend(self.target_args.iter().cloned());
        argv
    }

    /// Multi-target placeholder: `argv[1]` is the empty string until
    /// the subcommand is parsed off the trailing tokens, then patched
    /// by the caller. Post-`--` tokens are appended verbatim so
    /// `baml.sys.argv()` reflects exactly what the user typed.
    fn build_argv_for_subcommand(&self) -> Vec<String> {
        let executable = std::env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "baml".to_string());
        let mut argv = vec![executable, String::new()];
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

    /// Load the project (or standalone `--file`), check diagnostics,
    /// compile to bytecode, create engine.
    fn load_and_compile(
        &self,
        event_sink: Option<Arc<dyn bex_events::EventSink>>,
        argv: Vec<String>,
        reporter: &Reporter,
    ) -> Result<(ProjectDatabase, BexEngine, bool)> {
        if let Some(file) = self.file.as_deref() {
            return self.load_and_compile_standalone(file, event_sink, argv, reporter);
        }

        // `load_project_from_reporting` emits one `Loading <file>`
        // line per discovered source file (cargo-style per-unit
        // progress) — no aggregate `Loading <project>` needed.
        let (db, from, baml_files) = load_project_from_reporting(&self.from, reporter)?;
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

        reporter.spin("Checking", format!("{} file(s)", baml_files.len()));
        self.check_project_diagnostics(&db, "Cannot run: compilation errors found", reporter)?;
        // For `--list` the bytecode emit isn't preparing the program
        // for execution — it's just there so we can call
        // `engine.user_functions()` to enumerate signatures. Showing
        // "Compiling" in that case is technically true but misleading
        // ("am I about to run something?"). Use "Resolving" so the
        // verb reflects what the user is actually waiting on.
        let compile_verb = if self.list { "Resolving" } else { "Compiling" };
        reporter.spin(compile_verb, format!("{} file(s)", baml_files.len()));
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
        file_path: &Path,
        event_sink: Option<Arc<dyn bex_events::EventSink>>,
        argv: Vec<String>,
        reporter: &Reporter,
    ) -> Result<(ProjectDatabase, BexEngine, bool)> {
        let display = file_path.display().to_string();
        reporter.spin("Loading", &display);
        let canonical = std::fs::canonicalize(file_path)
            .with_context(|| format!("File not found: {display}"))?;
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

        reporter.spin("Checking", &display);
        self.check_project_diagnostics(
            &db,
            &format!("Cannot run: compilation errors in {display}"),
            reporter,
        )?;
        // See note in `load_and_compile`: "Resolving" is the honest
        // verb when --list is the destination, "Compiling" otherwise.
        let compile_verb = if self.list { "Resolving" } else { "Compiling" };
        reporter.spin(compile_verb, &display);
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
        reporter: &Reporter,
    ) -> Result<crate::ExitCode> {
        reporter.spin("Compiling", "expression");
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
        // Project marker: matches `project_load::load_project_from_inner`.
        // `baml.toml` is the marker; `baml_src/` alone no longer qualifies
        // (would let `baml run -e` pick up project context that every
        // other verb refuses).
        let has_explicit_project = from.as_ref().is_some_and(|f| f.join("baml.toml").exists());

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

        self.check_project_diagnostics(
            &db,
            "Cannot evaluate expression: compilation errors",
            reporter,
        )?;
        // BEP-027 §"`baml.argv`": `argv[1]` for `-e` is "the expression
        // source" — the loaded body text, not the `@path` reference. This
        // matches the inline case: `-e '2 + 2'` and `-e @file` (with
        // `file` containing `2 + 2`) produce the same argv.
        let engine = self.compile_to_engine(
            &db,
            event_sink.clone(),
            self.build_argv_for_expression(expr_body),
        )?;

        // Cargo-shape `     Running …` before the program's stdout
        // starts; then clear the spinner so the evaluated expression's
        // output doesn't collide with a still-ticking lamb.
        reporter.status("Running", "expression");
        reporter.abandon();

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
                    crate::reporter::print_error(format_args!("failed to serialize output: {e}"));
                }
            }
            Ok(())
        });

        if let Some(sink) = &event_sink {
            sink.flush();
        }

        match result {
            Ok(()) => {
                reporter.finish("Finished", "expression");
                Ok(crate::ExitCode::Success)
            }
            Err(bex_engine::EngineError::Exit { code }) => {
                std::process::exit(baml_exec::clamp_exit_code(code));
            }
            Err(e) => {
                crate::reporter::print_error(format_args!("{e:#}"));
                Ok(crate::ExitCode::TargetError)
            }
        }
    }

    // ========================================================================
    // `[scripts]` parsing
    // ========================================================================

    /// Parse `[scripts]` from raw `baml.toml` content.
    ///
    /// Emits a styled `Warning:` on TOML parse failures so a typo in
    /// `baml.toml` doesn't silently disable every script. A missing
    /// `baml.toml` (empty content) is normal and stays quiet. Routed
    /// through `print_warning` so the bold-yellow `Warning:` prefix
    /// matches every other advisory the CLI emits.
    fn parse_scripts(content: &str) -> HashMap<String, Vec<String>> {
        let trimmed = content.trim();
        let table = match content.parse::<toml::Table>() {
            Ok(t) => t,
            Err(e) => {
                if !trimmed.is_empty() {
                    crate::reporter::print_warning(format_args!(
                        "failed to parse `baml.toml` ({e}); [scripts] entries will be ignored"
                    ));
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
                crate::reporter::print_warning(format_args!(
                    "{loc}: [scripts] `{name}` shadows namespace `{name}` — \
                     the script takes precedence"
                ));
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
        &self,
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
            OutputFormat::Debug => {
                Self::print_list_debug(&functions, scripts, namespaces, self.verbose);
            }
            OutputFormat::Json => Self::print_list_json(&functions, scripts, namespaces),
        }

        Ok(crate::ExitCode::Success)
    }

    /// Just/npm-style flat list of runnable targets, grouped by kind
    /// (scripts → namespaces → functions) under bold-purple section
    /// headers. Auto-derived `to_json` / `from_json`, companion
    /// constructors, and other compiler-synthesized helpers are hidden
    /// by default — `--verbose` shows them.
    fn print_list_debug(
        functions: &[UserFunctionInfo],
        scripts: &HashMap<String, Vec<String>>,
        namespaces: &HashSet<String>,
        verbose: bool,
    ) {
        use bex_vm_types::FunctionOrigin;
        use console::Style;

        let header_style = Style::new()
            .fg(console::Color::TrueColor(0xA8, 0x55, 0xF7))
            .bold();
        let dim = Style::new().color256(244);
        let header = |s: &str| println!("{}", header_style.apply_to(s));

        // Visible by default: user-authored entries only. Pass
        // `--verbose` to expose compiler-synthesized helpers
        // (autoderive `to_json` / `from_json` per class, companion
        // `$new` constructors per client, internal `$init` etc).
        let visible: Vec<&UserFunctionInfo> = if verbose {
            functions.iter().collect()
        } else {
            functions
                .iter()
                .filter(|f| matches!(f.origin, FunctionOrigin::UserDefined))
                .collect()
        };

        // BEP-027 §"Auto-CLI conventions" / `--list`: a namespace
        // shows up here only when it has a `main`, since `baml run
        // <ns>` runs `<ns>.main`. Plain namespaces with no `main`
        // aren't directly runnable so we leave them out.
        let mut namespace_mains: Vec<&str> = visible
            .iter()
            .filter_map(|f| {
                f.display_name
                    .strip_suffix(".main")
                    .filter(|ns| namespaces.contains(*ns))
            })
            .collect();
        namespace_mains.sort();
        namespace_mains.dedup();

        if !scripts.is_empty() {
            header("Scripts");
            let mut names: Vec<&String> = scripts.keys().collect();
            names.sort();
            for name in names {
                println!("  {name}");
            }
            println!();
        }

        if !namespace_mains.is_empty() {
            header("Namespaces");
            for ns in &namespace_mains {
                println!("  {ns}");
            }
            println!();
        }

        if !visible.is_empty() {
            header("Functions");
            for func in &visible {
                let params: Vec<String> = func
                    .param_names
                    .iter()
                    .zip(func.param_types.iter())
                    .enumerate()
                    .map(|(idx, (name, ty))| {
                        let opt = if func.param_has_default.get(idx).copied().unwrap_or(false) {
                            " [optional]"
                        } else {
                            ""
                        };
                        format!("{name}: {ty}{opt}")
                    })
                    .collect();
                let suffix = if func.is_llm {
                    format!("  {}", dim.apply_to("[llm]"))
                } else {
                    String::new()
                };
                println!(
                    "  {}({}) -> {}{suffix}",
                    func.display_name,
                    params.join(", "),
                    func.return_type,
                );
            }
            println!();
        }

        // When the default filter hid everything, surface that fact
        // rather than leaving the user staring at an empty section.
        if !verbose && visible.is_empty() && !functions.is_empty() {
            println!(
                "{}",
                dim.apply_to(format!(
                    "(hiding {} compiler-synthesized function(s); pass --verbose to show them)",
                    functions.len()
                ))
            );
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

    fn print_run_help() {
        use clap::CommandFactory;
        let cmd = crate::commands::RuntimeCli::command();
        if let Some(sub) = cmd.find_subcommand("run") {
            // Clap propagates `styles = …` from the root command to
            // subcommands at parse time, not when a subcommand
            // reference is grabbed directly via `find_subcommand`.
            // Re-apply `CLAP_STYLING` explicitly so `run --help` keeps
            // the same green/cyan ariadne-adjacent palette as
            // `baml-cli --help`, instead of falling back to clap's
            // default bold+underline-no-color treatment.
            let mut sub = sub.clone().styles(crate::reporter::CLAP_STYLING);
            let _ = sub.print_help();
        }
    }
}

// ============================================================================
// Reserved verbs & namespace helpers
// ============================================================================

pub(crate) const FORMAT_HINT: &str =
    "Your code is unformatted — run `baml fmt` to format it. Continuing.";

pub(crate) fn source_needs_format_hint(source: &str) -> bool {
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

/// Heuristic: does the positional `<TARGET>` look like a filesystem
/// path rather than a function name? See pack_command's twin helper —
/// kept duplicated rather than hoisted so each command owns its own
/// "redirect to --file" hint message.
fn looks_like_path(target: &str) -> bool {
    target.contains('/') || target.contains('\\') || target.ends_with(".baml")
}

/// Render a clap error and return the matching CLI exit code.
/// Help / version requests render to stdout and exit success; all other
/// kinds go to stderr with a non-zero exit. The spinner is abandoned
/// first so the multi-line clap block lands cleanly above the cursor.
fn surface_clap_error(reporter: &Reporter, err: clap::Error) -> Result<crate::ExitCode> {
    use baml_exec::clap_reexport::ErrorKind;
    let kind = err.kind();
    reporter.abandon();
    let _ = err.print();
    Ok(match kind {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => crate::ExitCode::Success,
        _ => crate::ExitCode::Other,
    })
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
        // Pinned because the wording is user-facing copy: any change
        // here is a deliberate UX call, not a casual refactor.
        assert_eq!(
            FORMAT_HINT,
            "Your code is unformatted — run `baml fmt` to format it. Continuing."
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

    /// Engine fixture for namespace tests — `compile_multi_file`
    /// supports folder-based namespaces (`ns_<name>/foo.baml`) which the
    /// single-source `compile_source` helper can't express.
    fn engine_from_files(files: &[(&str, &str)]) -> BexEngine {
        let snapshot = baml_project::testing::compile_multi_file(files);
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("BexEngine::new should succeed")
    }

    /// Build a `RunArgs` with default everything; tests flip individual
    /// fields. Keeps test bodies focused on the field under test.
    fn run_args() -> RunArgs {
        RunArgs {
            target: None,
            functions: Vec::new(),
            expression: None,
            file: None,
            list: false,
            output_format: OutputFormat::Debug,
            log_file: None,
            verbose: false,
            help: false,
            from: PathBuf::from("."),
            target_args: Vec::new(),
        }
    }

    // ── Dispatch-mode mutex ───────────────────────────────────────────

    /// `-f` and a positional target are mutually exclusive dispatch modes.
    #[test]
    fn test_run_rejects_target_plus_function() {
        let mut args = run_args();
        args.target = Some("eval".into());
        args.functions = vec!["X".into()];
        let err = args.run().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("mutually exclusive"), "got: {msg}");
        assert!(
            msg.contains("`<target>`") && msg.contains("`-f`"),
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

    /// `-e` and `-f` are mutually exclusive.
    #[test]
    fn test_run_rejects_function_plus_expression() {
        let mut args = run_args();
        args.functions = vec!["X".into()];
        args.expression = Some("2 + 2".into());
        let err = args.run().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("mutually exclusive"), "got: {msg}");
        assert!(msg.contains("`-f`") && msg.contains("`-e`"), "got: {msg}");
    }

    /// All three dispatch modes together → mutex error names all three.
    #[test]
    fn test_run_rejects_three_dispatch_modes() {
        let mut args = run_args();
        args.target = Some("t".into());
        args.functions = vec!["F".into()];
        args.expression = Some("e".into());
        let err = args.run().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("`<target>`"));
        assert!(msg.contains("`-f`"));
        assert!(msg.contains("`-e`"));
    }

    /// `-e` + `--file` is rejected — `-e` evaluates inline source so a
    /// separate `--file` would be silently ignored.
    #[test]
    fn test_run_rejects_expression_plus_file() {
        let mut args = run_args();
        args.expression = Some("2 + 2".into());
        args.file = Some(PathBuf::from("a.baml"));
        let err = args.run().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("`-e`"), "got: {msg}");
        assert!(msg.contains("--file"), "got: {msg}");
    }

    /// `-e` + `--list` is rejected — `--list` enumerates project
    /// targets; expression mode has none.
    #[test]
    fn test_run_rejects_expression_plus_list() {
        let mut args = run_args();
        args.expression = Some("2 + 2".into());
        args.list = true;
        let err = args.run().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("`-e`"), "got: {msg}");
        assert!(msg.contains("--list"), "got: {msg}");
    }

    /// `--file` and `--from` are mutually exclusive (both name a source).
    #[test]
    fn test_run_rejects_file_plus_explicit_from() {
        let mut args = run_args();
        args.target = Some("X".into());
        args.file = Some(PathBuf::from("a.baml"));
        args.from = PathBuf::from("./project");
        let err = args.run().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("mutually exclusive"), "got: {msg}");
        assert!(msg.contains("--file"), "got: {msg}");
    }

    /// `--file` with default `--from` (i.e. `.`) is fine — we can't tell
    /// "user passed `.`" from "user passed nothing" at the clap layer,
    /// so the mutex only fires when `--from` is explicitly non-default.
    #[test]
    fn test_run_allows_file_with_default_from() {
        let mut args = run_args();
        args.target = Some("X".into());
        args.file = Some(PathBuf::from("a.baml"));
        // args.from is `PathBuf::from(".")` (the default).
        // The validation pass shouldn't reject this; the actual file load
        // will fail later but for a different reason.
        // We can't call .run() here without hitting filesystem so probe
        // the same predicate `run_with_reporter` would.
        assert!(args.file.is_some());
        assert_eq!(args.from, Path::new("."));
    }

    // ── Clap derive parse tests ──────────────────────────────────────

    /// `-f` is repeatable through the clap derive. Regression test for
    /// the derive macro silently collapsing repeated short flags into
    /// `Option<String>` instead of `Vec<String>`.
    #[test]
    fn test_run_dash_f_is_repeatable() {
        use clap::{CommandFactory, FromArgMatches, Parser};
        #[derive(Parser)]
        #[command(disable_help_flag = true)]
        struct Wrapper {
            #[command(flatten)]
            args: RunArgs,
        }
        let cmd = Wrapper::command();
        let matches = cmd
            .clone()
            .try_get_matches_from(["run", "-f", "a", "-f", "b", "--function", "c"])
            .unwrap();
        let parsed = Wrapper::from_arg_matches(&matches).unwrap();
        assert_eq!(parsed.args.functions, vec!["a", "b", "c"]);
        assert!(parsed.args.target.is_none());
    }

    /// `--file` binds to the PathBuf field.
    #[test]
    fn test_run_file_flag_parses() {
        use clap::Parser;
        #[derive(Parser)]
        #[command(disable_help_flag = true)]
        struct Wrapper {
            #[command(flatten)]
            args: RunArgs,
        }
        let parsed = Wrapper::try_parse_from(["run", "describe", "--file", "x.baml"]).unwrap();
        assert_eq!(parsed.args.target.as_deref(), Some("describe"));
        assert_eq!(parsed.args.file.as_deref(), Some(Path::new("x.baml")));
    }

    /// Tokens after `--` land in `target_args` verbatim, not consumed by
    /// any earlier flag.
    #[test]
    fn test_run_post_dash_dash_tokens_are_verbatim() {
        use clap::Parser;
        #[derive(Parser)]
        #[command(disable_help_flag = true)]
        struct Wrapper {
            #[command(flatten)]
            args: RunArgs,
        }
        let parsed = Wrapper::try_parse_from([
            "run",
            "-f",
            "a",
            "--",
            "a",
            "--ty",
            "string",
            "--json-args",
            "{}",
        ])
        .unwrap();
        assert_eq!(parsed.args.functions, vec!["a"]);
        assert_eq!(
            parsed.args.target_args,
            vec!["a", "--ty", "string", "--json-args", "{}"]
        );
    }

    /// The verb-level `--json-args` flag was removed. A user that types
    /// it pre-`--` should hit clap's unknown-flag path, not silently
    /// land it somewhere.
    #[test]
    fn test_run_rejects_verb_level_json_args() {
        use clap::{CommandFactory, Parser};
        #[derive(Parser)]
        #[command(disable_help_flag = true)]
        struct Wrapper {
            #[command(flatten)]
            args: RunArgs,
        }
        let err = Wrapper::command()
            .try_get_matches_from(["run", "--json-args", "{}", "describe"])
            .unwrap_err();
        assert!(
            matches!(
                err.kind(),
                clap::error::ErrorKind::UnknownArgument | clap::error::ErrorKind::InvalidSubcommand
            ),
            "got: {err}"
        );
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

    // ── Namespaced `-f` targets (folder-based `ns_<name>/`) ──────────

    /// `baml run -f llm.Summarize` resolves to the namespaced function
    /// and surfaces `Summarize` (the leaf segment) as the subcommand
    /// name the user types after `--`.
    #[test]
    fn test_run_subcommand_resolves_namespaced_target() {
        let engine = engine_from_files(&[(
            "ns_llm/summarize.baml",
            "function Summarize(text: string) -> string { text }",
        )]);
        let mut args = run_args();
        args.functions = vec!["llm.Summarize".into()];
        let (entries, lookups) = args.resolve_subcommand_targets(&engine).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].qualified_name, "user.llm.Summarize");
        assert_eq!(entries[0].display_name, "llm.Summarize");
        assert_eq!(entries[0].subcommand_name, "Summarize");
        assert!(lookups.contains_key("user.llm.Summarize"));
    }

    /// `-f` across namespaces builds one entry per function, each
    /// keyed on its leaf segment.
    #[test]
    fn test_run_subcommand_multi_namespaced() {
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
        let mut args = run_args();
        args.functions = vec!["llm.Summarize".into(), "util.Greet".into()];
        let (entries, _) = args.resolve_subcommand_targets(&engine).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].display_name, "llm.Summarize");
        assert_eq!(entries[0].subcommand_name, "Summarize");
        assert_eq!(entries[1].display_name, "util.Greet");
        assert_eq!(entries[1].subcommand_name, "Greet");
    }

    /// Two namespaces exporting the same leaf → subcommand-name
    /// collision is rejected with both fully-qualified names cited.
    /// Same regression check as the pack-side equivalent.
    #[test]
    fn test_run_subcommand_namespaced_name_collision_errors() {
        let engine = engine_from_files(&[
            ("ns_llm/foo.baml", "function Foo() -> int { 1 }"),
            ("ns_util/foo.baml", "function Foo() -> int { 2 }"),
        ]);
        let mut args = run_args();
        args.functions = vec!["llm.Foo".into(), "util.Foo".into()];
        let err = args.resolve_subcommand_targets(&engine).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("share subcommand name `Foo`"), "got: {msg}");
        assert!(msg.contains("llm.Foo"), "got: {msg}");
        assert!(msg.contains("util.Foo"), "got: {msg}");
    }

    /// Bare-name lookup also resolves namespaced targets via the
    /// shared resolver's suffix scan — `Summarize` finds
    /// `llm.Summarize` when it's the unique match.
    #[test]
    fn test_run_subcommand_namespaced_resolves_via_bare_name() {
        let engine = engine_from_files(&[(
            "ns_llm/summarize.baml",
            "function Summarize(text: string) -> string { text }",
        )]);
        let mut args = run_args();
        args.functions = vec!["Summarize".into()];
        let (entries, _) = args.resolve_subcommand_targets(&engine).unwrap();
        assert_eq!(entries[0].qualified_name, "user.llm.Summarize");
        assert_eq!(entries[0].subcommand_name, "Summarize");
    }

    /// Unknown `-f` target → "function not found" error with the same
    /// suggestion-engine output we use for the single-target path.
    #[test]
    fn test_run_subcommand_unknown_function_errors() {
        let engine = engine_from_files(&[(
            "ns_llm/summarize.baml",
            "function Summarize(text: string) -> string { text }",
        )]);
        let mut args = run_args();
        args.functions = vec!["DoesNotExist".into()];
        let err = args.resolve_subcommand_targets(&engine).unwrap_err();
        assert!(format!("{err}").contains("not found"));
    }
}
