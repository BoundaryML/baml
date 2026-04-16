#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow};
use baml_db::{baml_compiler_diagnostics::Severity, baml_compiler2_emit};
use baml_project::ProjectDatabase;
use baml_workspace::discover_baml_files;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, Ty, UserFunctionInfo};
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
        if token == "--" {
            after_separator = true;
            i += 1;
            continue;
        }
        if after_separator {
            extra_args.push(token.to_string());
        } else if token == "--function" {
            i += 1;
            if i < tokens.len() {
                function = Some(tokens[i].clone());
            } else {
                anyhow::bail!("Script body has --function without a value");
            }
        }
        // Ignore other run-verb flags in the script body for now.
        i += 1;
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

    /// List runnable targets (functions, namespace mains).
    #[arg(long)]
    pub list: bool,

    /// Output format: debug (default) or json.
    #[arg(long, default_value = "debug")]
    pub output: OutputFormat,

    /// Write run logs to a file.
    #[arg(long)]
    pub log_file: Option<PathBuf>,

    /// Verbose output.
    #[arg(long)]
    pub verbose: bool,

    /// Project root directory.
    #[arg(long, default_value = ".")]
    pub from: PathBuf,

    /// Arguments passed to the target function (after `--` separator).
    /// These are parsed as auto-CLI flags derived from the function signature.
    #[arg(last = true)]
    pub target_args: Vec<String>,
}

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum OutputFormat {
    Debug,
    Json,
}

// ============================================================================
// Main entry point
// ============================================================================

impl RunArgs {
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
        let mut error_count = 0;
        for diag in &diagnostics {
            match diag.severity {
                Severity::Warning => self.vlog(format_args!("warning: {}", diag.message)),
                Severity::Error => {
                    if error_count == 0 {
                        eprintln!("Compilation errors:");
                    }
                    error_count += 1;
                    eprintln!("  error: {}", diag.message);
                }
                _ => {}
            }
        }
        if error_count > 0 {
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
        let event_sink: Option<Arc<dyn bex_events::EventSink>> =
            self.log_file.as_ref().map(|path| {
                self.vlog(format_args!("Writing logs to {}", path.display()));
                bex_events_native::start(path.clone())
            });

        if let Some(expr_source) = &self.expression {
            return self.run_expression(expr_source, event_sink);
        }

        let argv = self.build_argv();

        let (db, engine) = if self.target.as_ref().is_some_and(|t| t.ends_with(".baml")) {
            self.load_and_compile_standalone(
                self.target.as_ref().unwrap(),
                event_sink.clone(),
                argv,
            )?
        } else {
            self.load_and_compile(event_sink.clone(), argv)?
        };
        let _ = db; // keep db alive for engine lifetime

        if self.list {
            return self.run_list(&engine);
        }

        let resolved = self.resolve_target(&engine)?;

        let (function_name, effective_target_args) = match resolved {
            ResolvedTarget::Function(name) => (name, self.target_args.clone()),
            ResolvedTarget::Script(expansion) => {
                let func = expansion.function.unwrap_or_else(|| "main".to_string());
                if !engine.function_exists(&func) {
                    return Err(self.function_not_found_error(&engine, &func));
                }
                // Script args come first so CLI args (after `--`) can override them.
                let mut merged_args = expansion.extra_args;
                merged_args.extend(self.target_args.iter().cloned());
                (func, merged_args)
            }
        };

        let params = engine
            .function_params(&function_name)
            .map_err(|e| anyhow!("{e}"))?;
        let param_names: Vec<String> = params.iter().map(|(n, _)| (*n).to_string()).collect();
        let param_types: Vec<Ty> = params.iter().map(|(_, t)| (*t).clone()).collect();

        // Per BEP-027, tokens after `--` are forwarded to the function verbatim,
        // so `--help` alongside other args is a value, not a help request. Only
        // treat it as help when it's the sole forwarded token.
        if effective_target_args.len() == 1 && effective_target_args[0] == "--help" {
            self.print_target_help(&function_name, &param_names, &param_types, &engine);
            return Ok(crate::ExitCode::Success);
        }

        let args = self.build_args_from(&effective_target_args, &param_names, &param_types)?;

        self.vlog(format_args!(
            "Calling {function_name} with {} arg(s)",
            args.len()
        ));

        let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;
        let engine = Arc::new(engine);
        let start = std::time::Instant::now();
        let result = rt.block_on(engine.call_function(
            &function_name,
            args,
            FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        ));

        self.vlog(format_args!("Completed in {:.2?}", start.elapsed()));

        if let Some(sink) = &event_sink {
            sink.flush();
        }

        match result {
            Ok(value) => {
                self.format_output(&value);
                Ok(crate::ExitCode::Success)
            }
            Err(e) => {
                eprintln!("Error: {e}");
                Ok(crate::ExitCode::Other)
            }
        }
    }

    /// Build the argv vector exposed to BAML via `baml.sys.argv()`.
    ///
    /// Per BEP-027:
    ///   [0] = path to the `baml` executable
    ///   [1] = entry path (absolute path for file targets, otherwise the
    ///         function/target/script name)
    ///   [2+] = user tokens after `--`, verbatim
    fn build_argv(&self) -> Vec<String> {
        let executable = std::env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "baml".to_string());

        let entry = if let Some(expr) = &self.expression {
            // Expression mode: argv[1] is the expression target.
            //   `-e '<inline>'` → the inline expression text
            //   `-e @path`      → canonicalized absolute file path
            //   `-e -`          → "-" (stdin marker)
            if expr == "-" {
                "-".to_string()
            } else if let Some(path) = expr.strip_prefix('@') {
                std::fs::canonicalize(path)
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| expr.clone())
            } else {
                expr.clone()
            }
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

    /// Load the project, check diagnostics, compile to bytecode, create engine.
    fn load_and_compile(
        &self,
        event_sink: Option<Arc<dyn bex_events::EventSink>>,
        argv: Vec<String>,
    ) -> Result<(ProjectDatabase, BexEngine)> {
        let (db, from, baml_files) = load_project_from(&self.from)?;
        self.vlog(format_args!("Loading project from {}", from.display()));
        if baml_files.is_empty() {
            anyhow::bail!("No .baml files found in {}", from.display());
        }
        self.vlog(format_args!("Found {} .baml file(s)", baml_files.len()));

        self.check_project_diagnostics(&db, "Cannot run: compilation errors found")?;
        self.vlog(format_args!("Compiling..."));
        let engine = self.compile_to_engine(&db, event_sink, argv)?;
        self.vlog(format_args!(
            "Compiled {} user function(s)",
            engine.user_functions().len()
        ));
        Ok((db, engine))
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
    ) -> Result<(ProjectDatabase, BexEngine)> {
        let canonical = std::fs::canonicalize(Path::new(file_path))
            .with_context(|| format!("File not found: {file_path}"))?;
        self.vlog(format_args!(
            "Standalone mode: loading {}",
            canonical.display()
        ));

        let content = std::fs::read_to_string(&canonical)
            .with_context(|| format!("Failed to read {}", canonical.display()))?;

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
        Ok((db, engine))
    }

    // ========================================================================
    // Expression mode (-e)
    // ========================================================================

    /// Evaluate a BAML expression.
    ///
    /// Wraps the expression in a synthetic `function $expr_main() { <body> }`
    /// and compiles/runs it. If inside a project, project context is available.
    fn run_expression(
        &self,
        source: &str,
        event_sink: Option<Arc<dyn bex_events::EventSink>>,
    ) -> Result<crate::ExitCode> {
        let expr_body = load_expression_source(source)?;
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
        let engine = self.compile_to_engine(&db, event_sink.clone(), self.build_argv())?;

        let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;
        let engine = Arc::new(engine);
        let result = rt.block_on(engine.call_function(
            "baml_run_expr_main__",
            vec![],
            FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        ));

        if let Some(sink) = &event_sink {
            sink.flush();
        }

        match result {
            Ok(value) => {
                self.format_output(&value);
                Ok(crate::ExitCode::Success)
            }
            Err(e) => {
                eprintln!("Error: {e}");
                Ok(crate::ExitCode::Other)
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
    fn resolve_target(&self, engine: &BexEngine) -> Result<ResolvedTarget> {
        // --function takes priority over positional target.
        if let Some(func) = &self.function {
            if engine.function_exists(func) {
                return Ok(ResolvedTarget::function(func.clone()));
            }
            return Err(self.function_not_found_error(engine, func));
        }

        match &self.target {
            None => {
                // No target → root namespace's `main`.
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
                // Ends in .baml → hermetic standalone. The engine was already
                // loaded in standalone mode; just look for `main`.
                if target.ends_with(".baml") {
                    if engine.function_exists("main") {
                        return Ok(ResolvedTarget::function("main".to_string()));
                    }
                    anyhow::bail!(
                        "Standalone file `{target}` has no `main` function.\n\
                         Use `baml run --function <name>` to call a specific function."
                    );
                }

                // Check [scripts] in baml.toml.
                let scripts = self.load_scripts();
                if let Some(script_tokens) = scripts.get(target.as_str()) {
                    self.vlog(format_args!(
                        "Expanding script `{target}`: {script_tokens:?}"
                    ));
                    return Ok(ResolvedTarget::Script(parse_script_body(script_tokens)?));
                }

                // Try namespace main: target.main
                let ns_main = format!("{target}.main");
                if engine.function_exists(&ns_main) {
                    return Ok(ResolvedTarget::function(ns_main));
                }

                // Try as a direct function name.
                if engine.function_exists(target) {
                    return Ok(ResolvedTarget::function(target.clone()));
                }

                Err(self.function_not_found_error(engine, target))
            }
        }
    }

    /// Load `[scripts]` from `baml.toml` in the project directory.
    fn load_scripts(&self) -> HashMap<String, Vec<String>> {
        let toml_path = self.from.join("baml.toml");
        let Ok(content) = std::fs::read_to_string(&toml_path) else {
            return HashMap::new();
        };
        let Ok(table) = content.parse::<toml::Table>() else {
            return HashMap::new();
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

    /// Build a helpful error message when a function/target isn't found.
    fn function_not_found_error(&self, engine: &BexEngine, name: &str) -> anyhow::Error {
        let mut functions = engine.user_functions();
        functions.sort_by(|a, b| a.display_name.cmp(&b.display_name));

        let suggestions: Vec<&str> = functions
            .iter()
            .filter(|f| {
                f.display_name.contains(name)
                    || name.contains(&f.display_name)
                    || strsim::jaro_winkler(&f.display_name, name) > 0.7
            })
            .take(5)
            .map(|f| f.display_name.as_str())
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
    // Argument parsing
    // ========================================================================

    /// Build the ordered argument vector by merging JSON args and auto-CLI flags.
    fn build_args_from(
        &self,
        target_args: &[String],
        param_names: &[String],
        param_types: &[Ty],
    ) -> Result<Vec<BexExternalValue>> {
        let json_map = match &self.json_args {
            Some(source) => {
                let json = load_json_source(source)?;
                let obj = json
                    .as_object()
                    .ok_or_else(|| anyhow!("--json-args must be a JSON object, got: {json}"))?;
                let mut map = HashMap::new();
                for (key, value) in obj {
                    // Known keys coerce with their declared type so enums/classes/
                    // lists/maps marshal correctly. Unknown keys fall through to
                    // untyped conversion and are reported as "unknown argument".
                    let converted = match param_names.iter().position(|n| n == key) {
                        Some(idx) => json_to_external_with_ty(value, &param_types[idx])
                            .with_context(|| format!("--json-args: parameter `{key}`"))?,
                        None => json_to_external(value),
                    };
                    map.insert(key.clone(), converted);
                }
                map
            }
            None => HashMap::new(),
        };

        let cli_map = parse_auto_cli_args(target_args, param_names, param_types)?;

        // CLI args override --json-args values.
        let mut merged = json_map;
        for (key, value) in cli_map {
            merged.insert(key, value);
        }

        let mut ordered = Vec::with_capacity(param_names.len());
        for (i, name) in param_names.iter().enumerate() {
            match merged.remove(name.as_str()) {
                Some(value) => ordered.push(value),
                None => {
                    let ty = &param_types[i];
                    anyhow::bail!(
                        "Missing required argument `--{name}` (type: {ty}).\n\
                         Pass it after `--`: baml run ... -- --{name} <value>"
                    );
                }
            }
        }

        if !merged.is_empty() {
            let unknown: Vec<&str> = merged.keys().map(String::as_str).collect();
            eprintln!(
                "Warning: unknown argument(s) ignored: {}",
                unknown.join(", ")
            );
        }

        Ok(ordered)
    }

    // ========================================================================
    // --list
    // ========================================================================

    fn run_list(&self, engine: &BexEngine) -> Result<crate::ExitCode> {
        let mut functions = engine.user_functions();
        functions.sort_by(|a, b| a.display_name.cmp(&b.display_name));

        if functions.is_empty() {
            println!("No runnable targets found.");
            return Ok(crate::ExitCode::Success);
        }

        match self.output {
            OutputFormat::Debug => self.print_list_debug(&functions),
            OutputFormat::Json => self.print_list_json(&functions),
        }

        Ok(crate::ExitCode::Success)
    }

    fn print_list_debug(&self, functions: &[UserFunctionInfo]) {
        println!("Available targets:\n");

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

        for (ns, funcs) in &grouped {
            let label = if ns.is_empty() { "(root)" } else { ns.as_str() };
            println!("  {label}:");
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
                    "    {short_name}({}) -> {}",
                    params.join(", "),
                    func.return_type
                );
            }
            println!();
        }

        println!("Run with: baml run --function <name> -- --arg1 value1");
    }

    fn print_list_json(&self, functions: &[UserFunctionInfo]) {
        let items: Vec<serde_json::Value> = functions
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

        println!(
            "{}",
            serde_json::to_string_pretty(&items).unwrap_or_else(|_| "[]".to_string())
        );
    }

    // ========================================================================
    // Per-target --help
    // ========================================================================

    fn print_target_help(
        &self,
        function_name: &str,
        param_names: &[String],
        param_types: &[Ty],
        engine: &BexEngine,
    ) {
        let display = function_name.strip_prefix("user.").unwrap_or(function_name);

        let ret_type = engine.function_params(function_name).ok().and_then(|_| {
            engine
                .user_functions()
                .into_iter()
                .find(|f| f.qualified_name == function_name || f.display_name == display)
                .map(|f| f.return_type)
        });

        let ret_str = ret_type
            .as_ref()
            .map_or("?".to_string(), std::string::ToString::to_string);

        let params_str: Vec<String> = param_names
            .iter()
            .zip(param_types.iter())
            .map(|(n, t)| format!("{n}: {t}"))
            .collect();

        println!("function {display}({}) -> {ret_str}", params_str.join(", "));
        println!();

        if param_names.is_empty() {
            println!("  This function takes no arguments.");
        } else {
            println!("  Arguments (pass after `--`):\n");
            for (name, ty) in param_names.iter().zip(param_types.iter()) {
                let type_hint = match ty {
                    Ty::Bool { .. } => " (use --name=true or --name=false)".to_string(),
                    Ty::Enum(tn, _) => format!(" (enum {tn})"),
                    Ty::Class(..) | Ty::Map { .. } | Ty::List(..) => {
                        " (use --json-args for complex types)".to_string()
                    }
                    _ => String::new(),
                };
                println!("    --{name} <{ty}>{type_hint}");
            }
        }

        println!();
        println!(
            "  Example: baml run --function {display} -- {}",
            param_names
                .iter()
                .zip(param_types.iter())
                .map(|(n, t)| format!("--{n} {}", example_value(t)))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }

    // ========================================================================
    // Output formatting
    // ========================================================================

    fn format_output(&self, value: &BexExternalValue) {
        match self.output {
            OutputFormat::Debug => println!("{}", format_value(value)),
            OutputFormat::Json => {
                let json = external_to_json(value);
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json).unwrap_or_else(|_| "null".to_string())
                );
            }
        }
    }
}

// ============================================================================
// Auto-CLI parser
// ============================================================================

/// Parse tokens after `--` into a map of parameter name → value.
///
/// Supports:
/// - `--name value` (two tokens)
/// - `--name=value` (single token with `=`)
/// - Positional sugar: single bare token when function has exactly one parameter
fn parse_auto_cli_args(
    tokens: &[String],
    param_names: &[String],
    param_types: &[Ty],
) -> Result<HashMap<String, BexExternalValue>> {
    if tokens.is_empty() || param_names.is_empty() {
        return Ok(HashMap::new());
    }

    // Positional sugar: single non-flag token + exactly one param.
    if tokens.len() == 1 && !tokens[0].starts_with("--") && param_names.len() == 1 {
        let value = parse_cli_value(&tokens[0], &param_types[0])
            .with_context(|| format!("Invalid value for `{}`: {}", param_names[0], tokens[0]))?;
        let mut map = HashMap::new();
        map.insert(param_names[0].clone(), value);
        return Ok(map);
    }

    let mut args = HashMap::new();
    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];
        if !token.starts_with("--") {
            anyhow::bail!(
                "Unexpected positional argument: `{token}`.\n\
                 Use `--param_name value` syntax for named arguments."
            );
        }
        let raw = &token[2..];

        let (key, val_str) = if let Some(eq_pos) = raw.find('=') {
            (&raw[..eq_pos], &raw[eq_pos + 1..])
        } else {
            i += 1;
            if i >= tokens.len() {
                anyhow::bail!("Missing value for `--{raw}`");
            }
            (raw, tokens[i].as_str())
        };

        let param_idx = find_param_index(key, param_names)?;
        let value = parse_cli_value(val_str, &param_types[param_idx])
            .with_context(|| format!("Invalid value for `--{key}`: {val_str}"))?;
        args.insert(key.to_string(), value);
        i += 1;
    }

    Ok(args)
}

/// Find parameter index by name, returning a helpful error if not found.
fn find_param_index(key: &str, param_names: &[String]) -> Result<usize> {
    param_names.iter().position(|n| n == key).ok_or_else(|| {
        let available: Vec<&str> = param_names.iter().map(String::as_str).collect();
        anyhow!(
            "Unknown parameter `--{key}`.\nAvailable parameters: {}",
            available.join(", ")
        )
    })
}

/// Convert a CLI string value to a `BexExternalValue` based on the target type.
fn parse_cli_value(raw: &str, ty: &Ty) -> Result<BexExternalValue> {
    match ty {
        Ty::String { .. } => Ok(BexExternalValue::String(raw.to_string())),

        Ty::Int { .. } => {
            let v: i64 = raw
                .parse()
                .with_context(|| format!("Expected integer, got `{raw}`"))?;
            Ok(BexExternalValue::Int(v))
        }

        Ty::Float { .. } => {
            let v: f64 = raw
                .parse()
                .with_context(|| format!("Expected float, got `{raw}`"))?;
            Ok(BexExternalValue::Float(v))
        }

        Ty::Bool { .. } => match raw {
            "true" => Ok(BexExternalValue::Bool(true)),
            "false" => Ok(BexExternalValue::Bool(false)),
            _ => anyhow::bail!("Expected `true` or `false`, got `{raw}`"),
        },

        Ty::Null { .. } => {
            if raw == "null" {
                Ok(BexExternalValue::Null)
            } else {
                anyhow::bail!("Expected `null`, got `{raw}`")
            }
        }

        Ty::Optional(inner, _) => {
            if raw == "null" {
                Ok(BexExternalValue::Null)
            } else {
                parse_cli_value(raw, inner)
            }
        }

        Ty::Enum(type_name, _) => Ok(BexExternalValue::Variant {
            enum_name: type_name.display_name.to_string(),
            variant_name: raw.to_string(),
        }),

        // Complex types accept inline JSON as a convenience; anything else must
        // go through `--json-args`.
        Ty::Class(..) | Ty::Map { .. } | Ty::List(..) | Ty::Union(..) => {
            match serde_json::from_str::<serde_json::Value>(raw) {
                Ok(json) => json_to_external_with_ty(&json, ty),
                Err(_) => anyhow::bail!(
                    "Parameter type `{ty}` requires JSON.\n\
                     Use `--json-args '{{...}}'` or pass a JSON string for this parameter."
                ),
            }
        }

        _ => Ok(BexExternalValue::String(raw.to_string())),
    }
}

// ============================================================================
// JSON argument loading
// ============================================================================

/// Load JSON from the `--json-args` source: inline string, @file, or - for stdin.
fn load_json_source(source: &str) -> Result<serde_json::Value> {
    if source == "-" {
        let input =
            std::io::read_to_string(std::io::stdin()).context("Failed to read JSON from stdin")?;
        serde_json::from_str(&input).context("Invalid JSON from stdin")
    } else if let Some(path) = source.strip_prefix('@') {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read file: {path}"))?;
        serde_json::from_str(&content).with_context(|| format!("Invalid JSON in file: {path}"))
    } else {
        serde_json::from_str(source).context("Invalid inline JSON for --json-args")
    }
}

/// Recursively convert a `serde_json::Value` to `BexExternalValue` with no
/// type information. Used as a fallback when the target type is unknown
/// (e.g., unknown `--json-args` keys, or nested class fields whose schema
/// we don't have at this layer). Prefer [`json_to_external_with_ty`] whenever
/// the target `Ty` is available.
fn json_to_external(value: &serde_json::Value) -> BexExternalValue {
    match value {
        serde_json::Value::Null => BexExternalValue::Null,
        serde_json::Value::Bool(b) => BexExternalValue::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                BexExternalValue::Int(i)
            } else {
                BexExternalValue::Float(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => BexExternalValue::String(s.clone()),
        serde_json::Value::Array(items) => BexExternalValue::Array {
            element_type: Ty::String {
                attr: Default::default(),
            },
            items: items.iter().map(json_to_external).collect(),
        },
        serde_json::Value::Object(map) => BexExternalValue::Instance {
            class_name: String::new(),
            fields: map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_external(v)))
                .collect(),
        },
    }
}

/// Convert a `serde_json::Value` to a `BexExternalValue` using the target
/// `Ty` to drive coercion. This is what makes enum JSON become `Variant`,
/// object JSON become `Instance { class_name }` with the correct name, and
/// lists/maps carry the declared element/value types.
///
/// Class field types are not resolved here (we don't have the class schema
/// at this layer), so nested class fields fall back to [`json_to_external`].
fn json_to_external_with_ty(value: &serde_json::Value, ty: &Ty) -> Result<BexExternalValue> {
    use serde_json::Value as J;
    match ty {
        Ty::Optional(inner, _) => {
            if matches!(value, J::Null) {
                Ok(BexExternalValue::Null)
            } else {
                json_to_external_with_ty(value, inner)
            }
        }

        Ty::Null { .. } => match value {
            J::Null => Ok(BexExternalValue::Null),
            _ => anyhow::bail!("Expected null, got `{value}`"),
        },

        Ty::Bool { .. } => match value {
            J::Bool(b) => Ok(BexExternalValue::Bool(*b)),
            _ => anyhow::bail!("Expected bool, got `{value}`"),
        },

        Ty::Int { .. } => match value {
            J::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(BexExternalValue::Int(i))
                } else if let Some(u) = n.as_u64() {
                    i64::try_from(u)
                        .map(BexExternalValue::Int)
                        .map_err(|_| anyhow!("Integer out of range for int: {u}"))
                } else {
                    anyhow::bail!("Expected integer, got `{value}`")
                }
            }
            _ => anyhow::bail!("Expected integer, got `{value}`"),
        },

        Ty::Float { .. } => match value {
            J::Number(n) => n
                .as_f64()
                .map(BexExternalValue::Float)
                .ok_or_else(|| anyhow!("Expected float, got `{value}`")),
            _ => anyhow::bail!("Expected float, got `{value}`"),
        },

        Ty::String { .. } => match value {
            J::String(s) => Ok(BexExternalValue::String(s.clone())),
            _ => anyhow::bail!("Expected string, got `{value}`"),
        },

        Ty::Enum(type_name, _) => match value {
            J::String(s) => Ok(BexExternalValue::Variant {
                enum_name: type_name.display_name.to_string(),
                variant_name: s.clone(),
            }),
            _ => anyhow::bail!(
                "Expected enum variant name (string) for `{}`, got `{value}`",
                type_name.display_name
            ),
        },

        Ty::Class(type_name, _) => match value {
            J::Object(map) => Ok(BexExternalValue::Instance {
                class_name: type_name.display_name.to_string(),
                fields: map
                    .iter()
                    .map(|(k, v)| (k.clone(), json_to_external(v)))
                    .collect(),
            }),
            _ => anyhow::bail!(
                "Expected object for class `{}`, got `{value}`",
                type_name.display_name
            ),
        },

        Ty::List(inner, _) => match value {
            J::Array(items) => {
                let mut converted = Vec::with_capacity(items.len());
                for item in items {
                    converted.push(json_to_external_with_ty(item, inner)?);
                }
                Ok(BexExternalValue::Array {
                    element_type: (**inner).clone(),
                    items: converted,
                })
            }
            _ => anyhow::bail!("Expected array for `{ty}`, got `{value}`"),
        },

        Ty::Map {
            key,
            value: value_ty,
            ..
        } => match value {
            J::Object(map) => {
                let mut pairs = Vec::with_capacity(map.len());
                for (k, v) in map {
                    pairs.push((k.clone(), json_to_external_with_ty(v, value_ty)?));
                }
                Ok(BexExternalValue::Map {
                    key_type: (**key).clone(),
                    value_type: (**value_ty).clone(),
                    entries: pairs.into_iter().collect(),
                })
            }
            _ => anyhow::bail!("Expected object for map `{ty}`, got `{value}`"),
        },

        Ty::Union(variants, _) => coerce_json_union(value, variants),

        // Types we don't specifically coerce: fall back to untyped conversion.
        _ => Ok(json_to_external(value)),
    }
}

/// Best-effort coercion into a union: try each variant and return the first
/// that succeeds. On failure, surface the last variant's error.
fn coerce_json_union(value: &serde_json::Value, variants: &[Ty]) -> Result<BexExternalValue> {
    let mut last_err: Option<anyhow::Error> = None;
    for variant in variants {
        match json_to_external_with_ty(value, variant) {
            Ok(v) => return Ok(v),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("No union variant matched value `{value}`")))
}

// ============================================================================
// Output conversion
// ============================================================================

/// Convert a `BexExternalValue` to a `serde_json::Value` for JSON output.
fn external_to_json(value: &BexExternalValue) -> serde_json::Value {
    match value {
        BexExternalValue::Null => serde_json::Value::Null,
        BexExternalValue::Int(i) => serde_json::json!(i),
        BexExternalValue::Float(f) => serde_json::json!(f),
        BexExternalValue::Bool(b) => serde_json::json!(b),
        BexExternalValue::String(s) => serde_json::json!(s),
        BexExternalValue::Array { items, .. } => {
            serde_json::Value::Array(items.iter().map(external_to_json).collect())
        }
        BexExternalValue::Map { entries, .. } => serde_json::Value::Object(
            entries
                .iter()
                .map(|(k, v)| (k.clone(), external_to_json(v)))
                .collect(),
        ),
        BexExternalValue::Instance {
            class_name, fields, ..
        } => {
            let mut map: serde_json::Map<String, serde_json::Value> = fields
                .iter()
                .map(|(k, v)| (k.clone(), external_to_json(v)))
                .collect();
            if !class_name.is_empty() {
                map.insert("__type".to_string(), serde_json::json!(class_name));
            }
            serde_json::Value::Object(map)
        }
        BexExternalValue::Variant {
            enum_name,
            variant_name,
        } => serde_json::json!({ "__type": enum_name, "value": variant_name }),
        BexExternalValue::Union { value, .. } => external_to_json(value),
        BexExternalValue::Uint8Array(bytes) => {
            serde_json::json!(format!("<bytes:{}>", bytes.len()))
        }
        _ => serde_json::json!(format!("{value:?}")),
    }
}

/// Human-readable formatting for `BexExternalValue`.
fn format_value(value: &BexExternalValue) -> String {
    match value {
        BexExternalValue::Null => "null".to_string(),
        BexExternalValue::Int(i) => i.to_string(),
        BexExternalValue::Float(f) => {
            let s = f.to_string();
            if s.contains('.') || !f.is_finite() {
                s
            } else {
                format!("{s}.0")
            }
        }
        BexExternalValue::Bool(b) => b.to_string(),
        BexExternalValue::String(s) => format!("{s:?}"),
        BexExternalValue::Array { items, .. } => {
            let inner: Vec<String> = items.iter().map(format_value).collect();
            format!("[{}]", inner.join(", "))
        }
        BexExternalValue::Map { entries, .. } => {
            let inner: Vec<String> = entries
                .iter()
                .map(|(k, v)| format!("{k:?}: {}", format_value(v)))
                .collect();
            format!("{{{}}}", inner.join(", "))
        }
        BexExternalValue::Instance { class_name, fields } => {
            let inner: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{k}: {}", format_value(v)))
                .collect();
            if class_name.is_empty() {
                format!("{{{}}}", inner.join(", "))
            } else {
                format!("{class_name} {{{}}}", inner.join(", "))
            }
        }
        BexExternalValue::Variant { variant_name, .. } => variant_name.clone(),
        BexExternalValue::Union { value, .. } => format_value(value),
        BexExternalValue::Uint8Array(bytes) => format!("<bytes:{}>", bytes.len()),
        _ => format!("{value:?}"),
    }
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

/// Generate a placeholder example value for a type (used in --help output).
fn example_value(ty: &Ty) -> &'static str {
    match ty {
        Ty::String { .. } => "\"value\"",
        Ty::Int { .. } => "42",
        Ty::Float { .. } => "3.14",
        Ty::Bool { .. } => "true",
        Ty::Null { .. } => "null",
        Ty::Enum(..) => "VariantName",
        _ => "...",
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use bex_engine::{Ty, TypeName};

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

    #[test]
    fn test_parse_cli_value_string() {
        let val = parse_cli_value("hello", &ty_string()).unwrap();
        assert!(matches!(val, BexExternalValue::String(s) if s == "hello"));
    }

    #[test]
    fn test_parse_cli_value_int() {
        let val = parse_cli_value("42", &ty_int()).unwrap();
        assert!(matches!(val, BexExternalValue::Int(42)));
    }

    #[test]
    fn test_parse_cli_value_int_negative() {
        let val = parse_cli_value("-7", &ty_int()).unwrap();
        assert!(matches!(val, BexExternalValue::Int(-7)));
    }

    #[test]
    fn test_parse_cli_value_int_invalid() {
        assert!(parse_cli_value("abc", &ty_int()).is_err());
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn test_parse_cli_value_float() {
        let val = parse_cli_value("3.14", &ty_float()).unwrap();
        assert!(matches!(val, BexExternalValue::Float(f) if (f - 3.14).abs() < 0.001));
    }

    #[test]
    fn test_parse_cli_value_bool_true() {
        let val = parse_cli_value("true", &ty_bool()).unwrap();
        assert!(matches!(val, BexExternalValue::Bool(true)));
    }

    #[test]
    fn test_parse_cli_value_bool_false() {
        let val = parse_cli_value("false", &ty_bool()).unwrap();
        assert!(matches!(val, BexExternalValue::Bool(false)));
    }

    #[test]
    fn test_parse_cli_value_bool_invalid() {
        assert!(parse_cli_value("yes", &ty_bool()).is_err());
    }

    #[test]
    fn test_parse_cli_value_optional_null() {
        let ty = Ty::Optional(Box::new(ty_int()), Default::default());
        let val = parse_cli_value("null", &ty).unwrap();
        assert!(matches!(val, BexExternalValue::Null));
    }

    #[test]
    fn test_parse_cli_value_optional_value() {
        let ty = Ty::Optional(Box::new(ty_int()), Default::default());
        let val = parse_cli_value("42", &ty).unwrap();
        assert!(matches!(val, BexExternalValue::Int(42)));
    }

    #[test]
    fn test_parse_cli_value_enum() {
        let tn = TypeName {
            name: "Color".into(),
            module_path: vec![],
            display_name: "Color".into(),
        };
        let ty = Ty::Enum(tn, Default::default());
        let val = parse_cli_value("Red", &ty).unwrap();
        match val {
            BexExternalValue::Variant {
                enum_name,
                variant_name,
            } => {
                assert_eq!(enum_name, "Color");
                assert_eq!(variant_name, "Red");
            }
            _ => panic!("Expected Variant"),
        }
    }

    // ── parse_auto_cli_args ────────────────────────────────────────

    #[test]
    fn test_auto_cli_empty() {
        let result = parse_auto_cli_args(&[], &[s("x")], &[ty_int()]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_auto_cli_named_args() {
        let tokens = vec![s("--a"), s("10"), s("--b"), s("20")];
        let names = vec![s("a"), s("b")];
        let types = vec![ty_int(), ty_int()];
        let result = parse_auto_cli_args(&tokens, &names, &types).unwrap();
        assert!(matches!(result.get("a"), Some(BexExternalValue::Int(10))));
        assert!(matches!(result.get("b"), Some(BexExternalValue::Int(20))));
    }

    #[test]
    fn test_auto_cli_equals_syntax() {
        let tokens = vec![s("--flag=true")];
        let names = vec![s("flag")];
        let types = vec![ty_bool()];
        let result = parse_auto_cli_args(&tokens, &names, &types).unwrap();
        assert!(matches!(
            result.get("flag"),
            Some(BexExternalValue::Bool(true))
        ));
    }

    #[test]
    fn test_auto_cli_positional_sugar() {
        let tokens = vec![s("hello")];
        let names = vec![s("name")];
        let types = vec![ty_string()];
        let result = parse_auto_cli_args(&tokens, &names, &types).unwrap();
        assert!(matches!(result.get("name"), Some(BexExternalValue::String(s)) if s == "hello"));
    }

    #[test]
    fn test_auto_cli_positional_sugar_requires_single_param() {
        // Two params — positional sugar should not apply.
        let tokens = vec![s("hello")];
        let names = vec![s("a"), s("b")];
        let types = vec![ty_string(), ty_string()];
        let result = parse_auto_cli_args(&tokens, &names, &types);
        assert!(result.is_err());
    }

    #[test]
    fn test_auto_cli_unknown_param() {
        let tokens = vec![s("--unknown"), s("val")];
        let names = vec![s("a")];
        let types = vec![ty_int()];
        let result = parse_auto_cli_args(&tokens, &names, &types);
        assert!(result.is_err());
    }

    #[test]
    fn test_auto_cli_missing_value() {
        let tokens = vec![s("--a")];
        let names = vec![s("a")];
        let types = vec![ty_int()];
        let result = parse_auto_cli_args(&tokens, &names, &types);
        assert!(result.is_err());
    }

    // ── json_to_external ───────────────────────────────────────────

    #[test]
    fn test_json_to_external_null() {
        let val = json_to_external(&serde_json::json!(null));
        assert!(matches!(val, BexExternalValue::Null));
    }

    #[test]
    fn test_json_to_external_int() {
        let val = json_to_external(&serde_json::json!(42));
        assert!(matches!(val, BexExternalValue::Int(42)));
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn test_json_to_external_float() {
        let val = json_to_external(&serde_json::json!(3.14));
        assert!(matches!(val, BexExternalValue::Float(f) if (f - 3.14).abs() < 0.001));
    }

    #[test]
    fn test_json_to_external_string() {
        let val = json_to_external(&serde_json::json!("hello"));
        assert!(matches!(val, BexExternalValue::String(s) if s == "hello"));
    }

    #[test]
    fn test_json_to_external_bool() {
        let val = json_to_external(&serde_json::json!(true));
        assert!(matches!(val, BexExternalValue::Bool(true)));
    }

    #[test]
    fn test_json_to_external_array() {
        let val = json_to_external(&serde_json::json!([1, 2, 3]));
        match val {
            BexExternalValue::Array { items, .. } => assert_eq!(items.len(), 3),
            _ => panic!("Expected Array"),
        }
    }

    #[test]
    fn test_json_to_external_object() {
        let val = json_to_external(&serde_json::json!({"a": 1, "b": "two"}));
        match val {
            BexExternalValue::Instance { fields, .. } => {
                assert_eq!(fields.len(), 2);
                assert!(matches!(fields.get("a"), Some(BexExternalValue::Int(1))));
            }
            _ => panic!("Expected Instance"),
        }
    }

    // ── json_to_external_with_ty ───────────────────────────────────

    fn tn(name: &str) -> TypeName {
        TypeName {
            name: name.into(),
            module_path: vec![],
            display_name: name.into(),
        }
    }

    #[test]
    fn test_typed_enum_becomes_variant() {
        let ty = Ty::Enum(tn("Color"), Default::default());
        let val = json_to_external_with_ty(&serde_json::json!("Red"), &ty).unwrap();
        match val {
            BexExternalValue::Variant {
                enum_name,
                variant_name,
            } => {
                assert_eq!(enum_name, "Color");
                assert_eq!(variant_name, "Red");
            }
            other => panic!("Expected Variant, got {other:?}"),
        }
    }

    #[test]
    fn test_typed_class_instance_gets_name() {
        let ty = Ty::Class(tn("User"), Default::default());
        let val =
            json_to_external_with_ty(&serde_json::json!({"id": 1, "name": "alice"}), &ty).unwrap();
        match val {
            BexExternalValue::Instance { class_name, fields } => {
                assert_eq!(class_name, "User");
                assert_eq!(fields.len(), 2);
                assert!(matches!(fields.get("id"), Some(BexExternalValue::Int(1))));
            }
            other => panic!("Expected Instance, got {other:?}"),
        }
    }

    #[test]
    fn test_typed_list_carries_element_type() {
        let ty = Ty::List(Box::new(ty_int()), Default::default());
        let val = json_to_external_with_ty(&serde_json::json!([1, 2, 3]), &ty).unwrap();
        match val {
            BexExternalValue::Array {
                element_type,
                items,
            } => {
                assert!(matches!(element_type, Ty::Int { .. }));
                assert_eq!(items.len(), 3);
                assert!(matches!(items[0], BexExternalValue::Int(1)));
            }
            other => panic!("Expected Array, got {other:?}"),
        }
    }

    #[test]
    fn test_typed_list_of_enum() {
        let ty = Ty::List(
            Box::new(Ty::Enum(tn("Color"), Default::default())),
            Default::default(),
        );
        let val = json_to_external_with_ty(&serde_json::json!(["Red", "Blue"]), &ty).unwrap();
        match val {
            BexExternalValue::Array { items, .. } => {
                assert_eq!(items.len(), 2);
                match &items[0] {
                    BexExternalValue::Variant {
                        enum_name,
                        variant_name,
                    } => {
                        assert_eq!(enum_name, "Color");
                        assert_eq!(variant_name, "Red");
                    }
                    other => panic!("Expected Variant, got {other:?}"),
                }
            }
            other => panic!("Expected Array, got {other:?}"),
        }
    }

    #[test]
    fn test_typed_map_with_int_values() {
        let ty = Ty::Map {
            key: Box::new(ty_string()),
            value: Box::new(ty_int()),
            attr: Default::default(),
        };
        let val = json_to_external_with_ty(&serde_json::json!({"a": 1, "b": 2}), &ty).unwrap();
        match val {
            BexExternalValue::Map {
                value_type,
                entries,
                ..
            } => {
                assert!(matches!(value_type, Ty::Int { .. }));
                assert_eq!(entries.len(), 2);
                assert!(matches!(entries.get("a"), Some(BexExternalValue::Int(1))));
            }
            other => panic!("Expected Map, got {other:?}"),
        }
    }

    #[test]
    fn test_typed_optional_null() {
        let ty = Ty::Optional(Box::new(ty_int()), Default::default());
        let val = json_to_external_with_ty(&serde_json::json!(null), &ty).unwrap();
        assert!(matches!(val, BexExternalValue::Null));
    }

    #[test]
    fn test_typed_optional_present() {
        let ty = Ty::Optional(Box::new(ty_int()), Default::default());
        let val = json_to_external_with_ty(&serde_json::json!(5), &ty).unwrap();
        assert!(matches!(val, BexExternalValue::Int(5)));
    }

    #[test]
    fn test_typed_type_mismatch_is_error() {
        let ty = ty_int();
        assert!(json_to_external_with_ty(&serde_json::json!("not-an-int"), &ty).is_err());
    }

    // ── external_to_json ───────────────────────────────────────────

    #[test]
    fn test_external_to_json_roundtrip() {
        let val = BexExternalValue::Int(42);
        assert_eq!(external_to_json(&val), serde_json::json!(42));

        let val = BexExternalValue::String("hello".into());
        assert_eq!(external_to_json(&val), serde_json::json!("hello"));

        let val = BexExternalValue::Bool(true);
        assert_eq!(external_to_json(&val), serde_json::json!(true));

        let val = BexExternalValue::Null;
        assert_eq!(external_to_json(&val), serde_json::json!(null));
    }

    // ── load_json_source ───────────────────────────────────────────

    #[test]
    fn test_load_json_source_inline() {
        let val = load_json_source(r#"{"a": 1}"#).unwrap();
        assert_eq!(val, serde_json::json!({"a": 1}));
    }

    #[test]
    fn test_load_json_source_invalid() {
        assert!(load_json_source("not json").is_err());
    }

    #[test]
    fn test_load_json_source_file() {
        let dir = unique_temp_dir("baml_test_json");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.json");
        std::fs::write(&path, r#"{"x": 42}"#).unwrap();

        let source = format!("@{}", path.display());
        let val = load_json_source(&source).unwrap();
        assert_eq!(val, serde_json::json!({"x": 42}));

        let _ = std::fs::remove_dir_all(dir);
    }

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

    #[test]
    fn test_load_json_source_file_missing_is_error() {
        let result = load_json_source("@/definitely/does/not/exist/args.json");
        assert!(result.is_err(), "missing @file must error");
    }

    #[test]
    fn test_load_json_source_file_with_invalid_json() {
        let dir = unique_temp_dir("baml_test_invalid_json");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.json");
        std::fs::write(&path, "{ not valid json").unwrap();

        let source = format!("@{}", path.display());
        let result = load_json_source(&source);
        assert!(result.is_err(), "invalid JSON from @file must error");

        let _ = std::fs::remove_dir_all(dir);
    }

    // ========================================================================
    // BEP-027 conformance tests
    //
    // Each test is tagged with the BEP-027 section/clause it pins down so a
    // future reader can trace the behavior back to the spec.
    // ========================================================================

    /// Helper: construct a default `RunArgs` for tests. Fields are mutated in
    /// each test to target the specific BEP clause under test.
    fn run_args() -> RunArgs {
        RunArgs {
            target: None,
            function: None,
            expression: None,
            json_args: None,
            list: false,
            output: OutputFormat::Debug,
            log_file: None,
            verbose: false,
            from: PathBuf::from("."),
            target_args: vec![],
        }
    }

    // ── BEP-027 §"baml.argv" — argv layout ─────────────────────────

    /// "If there is no target, `baml run` runs the root namespace's `main`."
    /// + argv[1] is the entry path. With no target the entry is `main`.
    #[test]
    fn test_bep_argv_no_target_entry_is_main() {
        let args = run_args();
        let argv = args.build_argv();
        assert!(!argv[0].is_empty(), "argv[0] must be the executable path");
        assert_eq!(argv[1], "main", "no-target run uses `main` as argv[1]");
        assert_eq!(argv.len(), 2, "no user tokens → argv has length 2");
    }

    /// BEP-027: "`argv[2+]` = Every user token after `--`, in order."
    #[test]
    fn test_bep_argv_passes_user_tokens_verbatim() {
        let mut args = run_args();
        args.target_args = vec![s("hello"), s("world"), s("--flag=1")];
        let argv = args.build_argv();
        assert_eq!(&argv[2..], &["hello", "world", "--flag=1"]);
    }

    /// BEP-027 §"Calling a function: `--function`": argv[1] is the function name.
    #[test]
    fn test_bep_argv_function_flag_sets_entry() {
        let mut args = run_args();
        args.function = Some(s("llm.Summarize"));
        let argv = args.build_argv();
        assert_eq!(argv[1], "llm.Summarize");
    }

    /// BEP-027 §"Hermetic single-file mode": `<file.baml>` is an absolute,
    /// stable path so argv[1] is identical across machines.
    #[test]
    fn test_bep_argv_baml_file_target_is_canonicalized() {
        let dir = unique_temp_dir("baml_test_argv_file");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("hello.baml");
        std::fs::write(&file, "function main() { print(\"hi\") }").unwrap();

        let mut args = run_args();
        args.target = Some(file.to_string_lossy().into_owned());
        let argv = args.build_argv();

        let canonical = std::fs::canonicalize(&file).unwrap();
        assert_eq!(argv[1], canonical.to_string_lossy());

        let _ = std::fs::remove_dir_all(dir);
    }

    /// Non-.baml targets (scripts and namespace names) pass through verbatim.
    #[test]
    fn test_bep_argv_script_or_namespace_target_verbatim() {
        let mut args = run_args();
        args.target = Some(s("backfill"));
        let argv = args.build_argv();
        assert_eq!(argv[1], "backfill");
    }

    /// BEP-027 §"Expression mode": `-e '<inline>'` → argv[1] is the expression
    /// text. Agreed with user: "it should put the expression there".
    #[test]
    fn test_bep_argv_expression_inline() {
        let mut args = run_args();
        args.expression = Some(s("2 + 2"));
        let argv = args.build_argv();
        assert_eq!(argv[1], "2 + 2");
    }

    /// BEP-027 §"Expression mode": `-e -` reads from stdin; argv[1] is the
    /// literal `-` marker so BAML code can distinguish stdin mode.
    #[test]
    fn test_bep_argv_expression_stdin_marker() {
        let mut args = run_args();
        args.expression = Some(s("-"));
        let argv = args.build_argv();
        assert_eq!(argv[1], "-");
    }

    /// BEP-027 §"Expression mode": `-e @file` resolves to the canonical path.
    #[test]
    fn test_bep_argv_expression_file_canonicalized() {
        let dir = unique_temp_dir("baml_test_argv_expr");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("expr.baml");
        std::fs::write(&file, "1 + 1").unwrap();

        let mut args = run_args();
        args.expression = Some(format!("@{}", file.display()));
        let argv = args.build_argv();

        let canonical = std::fs::canonicalize(&file).unwrap();
        assert_eq!(argv[1], canonical.to_string_lossy());

        let _ = std::fs::remove_dir_all(dir);
    }

    /// If the `.baml` file can't be canonicalized (doesn't exist yet), argv[1]
    /// falls back to the original string rather than panicking.
    #[test]
    fn test_bep_argv_baml_file_missing_falls_back_to_raw() {
        let mut args = run_args();
        args.target = Some(s("/definitely/missing/path.baml"));
        let argv = args.build_argv();
        assert_eq!(argv[1], "/definitely/missing/path.baml");
    }

    // ── BEP-027 §"JSON argument form" — build_args_from ────────────

    /// BEP-027: "pass arguments as JSON via `--json-args`".
    #[test]
    fn test_bep_build_args_json_only() {
        let mut args = run_args();
        args.json_args = Some(s(r#"{"text": "hello", "max_words": 30}"#));
        let ordered = args
            .build_args_from(&[], &[s("text"), s("max_words")], &[ty_string(), ty_int()])
            .unwrap();
        assert!(matches!(&ordered[0], BexExternalValue::String(s) if s == "hello"));
        assert!(matches!(&ordered[1], BexExternalValue::Int(30)));
    }

    /// BEP-027 §"Auto-CLI conventions": parameters appear after `--`. No JSON.
    #[test]
    fn test_bep_build_args_cli_only() {
        let args = run_args();
        let ordered = args
            .build_args_from(
                &[s("--text"), s("hi"), s("--max_words"), s("5")],
                &[s("text"), s("max_words")],
                &[ty_string(), ty_int()],
            )
            .unwrap();
        assert!(matches!(&ordered[0], BexExternalValue::String(s) if s == "hi"));
        assert!(matches!(&ordered[1], BexExternalValue::Int(5)));
    }

    /// BEP-027: "auto-CLI flags (after `--`) override JSON keys".
    #[test]
    fn test_bep_build_args_cli_overrides_json() {
        let mut args = run_args();
        args.json_args = Some(s(r#"{"text": "from-json", "max_words": 10}"#));
        let ordered = args
            .build_args_from(
                &[s("--max_words"), s("99")],
                &[s("text"), s("max_words")],
                &[ty_string(), ty_int()],
            )
            .unwrap();
        // text from JSON; max_words overridden by CLI.
        assert!(matches!(&ordered[0], BexExternalValue::String(s) if s == "from-json"));
        assert!(matches!(&ordered[1], BexExternalValue::Int(99)));
    }

    /// BEP-027 §"Auto-CLI conventions": a required parameter with no value
    /// surfaces a clear error at load time.
    #[test]
    fn test_bep_build_args_missing_required_is_error() {
        let args = run_args();
        let err = args
            .build_args_from(&[], &[s("text")], &[ty_string()])
            .unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("--text"),
            "error should name the missing param: {msg}"
        );
    }

    /// Unknown arg keys are *not* a hard error — BEP permits pass-through
    /// extras (they appear in `baml.argv`). The CLI emits a stderr warning.
    #[test]
    fn test_bep_build_args_unknown_is_non_fatal() {
        let mut args = run_args();
        args.json_args = Some(s(r#"{"text": "hi", "unknown_key": 1}"#));
        let result = args.build_args_from(&[], &[s("text")], &[ty_string()]);
        assert!(
            result.is_ok(),
            "unknown JSON keys should warn, not fail: {result:?}"
        );
    }

    /// BEP-027 type coercion: an enum parameter from JSON becomes `Variant`,
    /// not `String`.
    #[test]
    fn test_bep_build_args_enum_via_json() {
        let mut args = run_args();
        args.json_args = Some(s(r#"{"style": "detailed"}"#));
        let ordered = args
            .build_args_from(
                &[],
                &[s("style")],
                &[Ty::Enum(tn("SummaryStyle"), Default::default())],
            )
            .unwrap();
        match &ordered[0] {
            BexExternalValue::Variant {
                enum_name,
                variant_name,
            } => {
                assert_eq!(enum_name, "SummaryStyle");
                assert_eq!(variant_name, "detailed");
            }
            other => panic!("Expected Variant, got {other:?}"),
        }
    }

    /// Same coercion path via auto-CLI (not JSON).
    #[test]
    fn test_bep_build_args_enum_via_cli() {
        let args = run_args();
        let ordered = args
            .build_args_from(
                &[s("--style"), s("Concise")],
                &[s("style")],
                &[Ty::Enum(tn("SummaryStyle"), Default::default())],
            )
            .unwrap();
        match &ordered[0] {
            BexExternalValue::Variant { variant_name, .. } => {
                assert_eq!(variant_name, "Concise");
            }
            other => panic!("Expected Variant, got {other:?}"),
        }
    }

    /// BEP-027 §"JSON argument form": `@file` source for `--json-args`.
    #[test]
    fn test_bep_build_args_json_from_file() {
        let dir = unique_temp_dir("baml_test_json_args_file");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("args.json");
        std::fs::write(&path, r#"{"text": "filed", "max_words": 7}"#).unwrap();

        let mut args = run_args();
        args.json_args = Some(format!("@{}", path.display()));
        let ordered = args
            .build_args_from(&[], &[s("text"), s("max_words")], &[ty_string(), ty_int()])
            .unwrap();
        assert!(matches!(&ordered[0], BexExternalValue::String(s) if s == "filed"));
        assert!(matches!(&ordered[1], BexExternalValue::Int(7)));

        let _ = std::fs::remove_dir_all(dir);
    }

    /// BEP-027: "--json-args must be a JSON object" — top-level arrays/scalars
    /// are rejected with a clear error.
    #[test]
    fn test_bep_build_args_json_must_be_object() {
        let mut args = run_args();
        args.json_args = Some(s(r#"[1, 2, 3]"#));
        let err = args
            .build_args_from(&[], &[s("text")], &[ty_string()])
            .unwrap_err();
        assert!(format!("{err}").contains("JSON object"));
    }

    // ── BEP-027 §"Auto-CLI conventions" — parse_auto_cli_args ──────

    /// BEP-027: "Flag names mirror parameter names verbatim (start_date →
    /// --start_date). No kebab translation."
    #[test]
    fn test_bep_auto_cli_verbatim_names_no_kebab() {
        let tokens = vec![s("--start_date"), s("2024-01-01")];
        let names = vec![s("start_date")];
        let result = parse_auto_cli_args(&tokens, &names, &[ty_string()]).unwrap();
        assert!(result.contains_key("start_date"));
        // The kebab-form must NOT be accepted.
        let tokens_kebab = vec![s("--start-date"), s("2024-01-01")];
        assert!(parse_auto_cli_args(&tokens_kebab, &names, &[ty_string()]).is_err());
    }

    /// BEP-027: "Booleans use --flag=true / --flag=false, not --flag /
    /// --no-flag."
    #[test]
    fn test_bep_auto_cli_bool_equals_true_false() {
        let names = vec![s("verbose")];
        let types = vec![ty_bool()];

        let tokens_true = vec![s("--verbose=true")];
        let r = parse_auto_cli_args(&tokens_true, &names, &types).unwrap();
        assert!(matches!(
            r.get("verbose"),
            Some(BexExternalValue::Bool(true))
        ));

        let tokens_false = vec![s("--verbose=false")];
        let r = parse_auto_cli_args(&tokens_false, &names, &types).unwrap();
        assert!(matches!(
            r.get("verbose"),
            Some(BexExternalValue::Bool(false))
        ));
    }

    /// BEP-027: multiple named args in one invocation (from the Backfill
    /// example: `-- --start_date X --end_date Y --dry_run=true`).
    #[test]
    fn test_bep_auto_cli_multiple_named_mixed_forms() {
        let tokens = vec![
            s("--start_date"),
            s("2024-01-01"),
            s("--end_date"),
            s("2024-12-31"),
            s("--dry_run=true"),
        ];
        let names = vec![s("start_date"), s("end_date"), s("dry_run")];
        let types = vec![ty_string(), ty_string(), ty_bool()];
        let r = parse_auto_cli_args(&tokens, &names, &types).unwrap();
        assert!(
            matches!(r.get("start_date"), Some(BexExternalValue::String(s)) if s == "2024-01-01")
        );
        assert!(
            matches!(r.get("end_date"), Some(BexExternalValue::String(s)) if s == "2024-12-31")
        );
        assert!(matches!(
            r.get("dry_run"),
            Some(BexExternalValue::Bool(true))
        ));
    }

    /// BEP-027 §"Calling a function": "Single-required-param positional
    /// sugar" — `baml run --function llm.Summarize -- "the text"`.
    #[test]
    fn test_bep_auto_cli_positional_sugar_single_required() {
        let tokens = vec![s("the text to summarize")];
        let names = vec![s("text")];
        let types = vec![ty_string()];
        let r = parse_auto_cli_args(&tokens, &names, &types).unwrap();
        assert!(
            matches!(r.get("text"), Some(BexExternalValue::String(s)) if s == "the text to summarize")
        );
    }

    // ── BEP-027 §"Scripts in baml.toml" — parse_script_body ────────

    /// An empty script body resolves to a zero-arg expansion (runs the
    /// script's default main).
    #[test]
    fn test_bep_parse_script_body_empty() {
        let expansion = parse_script_body(&[]).unwrap();
        assert!(expansion.function.is_none());
        assert!(expansion.extra_args.is_empty());
    }

    /// Unknown run-verb flags in a script body are silently ignored today
    /// (documented). Locking in the behavior so an intentional change will
    /// break this test rather than users.
    #[test]
    fn test_bep_parse_script_body_ignores_other_flags() {
        let body = tokens("--verbose --output json -- --text hi");
        let expansion = parse_script_body(&body).unwrap();
        assert!(expansion.function.is_none());
        // Only tokens after `--` survive.
        assert_eq!(expansion.extra_args, vec!["--text", "hi"]);
    }

    /// `--function` without a trailing value is a load-time error (BEP-027:
    /// "Fail at load time, not at runtime").
    #[test]
    fn test_bep_parse_script_body_function_without_value_errors() {
        let body = vec![s("--function")];
        let err = parse_script_body(&body).unwrap_err();
        assert!(format!("{err}").contains("--function"));
    }

    /// Current behavior: every `--` token is consumed as a separator, not
    /// re-emitted into `extra_args`. The BEP does not specify what happens
    /// with repeated `--` tokens inside a script body. Locking in the
    /// current behavior so a future change is a deliberate spec decision.
    #[test]
    fn test_parse_script_body_collapses_repeated_separators() {
        let body = tokens("-- --one -- --two");
        let expansion = parse_script_body(&body).unwrap();
        assert_eq!(expansion.extra_args, vec!["--one", "--two"]);
    }

    // ── parse_cli_value — additional type paths ────────────────────

    #[test]
    fn test_parse_cli_value_null() {
        let ty = Ty::Null {
            attr: Default::default(),
        };
        assert!(matches!(
            parse_cli_value("null", &ty).unwrap(),
            BexExternalValue::Null
        ));
        assert!(parse_cli_value("nope", &ty).is_err());
    }

    #[test]
    fn test_parse_cli_value_list_via_json() {
        let ty = Ty::List(Box::new(ty_int()), Default::default());
        let val = parse_cli_value("[1,2,3]", &ty).unwrap();
        match val {
            BexExternalValue::Array {
                element_type,
                items,
            } => {
                assert!(matches!(element_type, Ty::Int { .. }));
                assert_eq!(items.len(), 3);
            }
            other => panic!("Expected Array, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_cli_value_map_via_json() {
        let ty = Ty::Map {
            key: Box::new(ty_string()),
            value: Box::new(ty_int()),
            attr: Default::default(),
        };
        let val = parse_cli_value(r#"{"a":1,"b":2}"#, &ty).unwrap();
        match val {
            BexExternalValue::Map {
                value_type,
                entries,
                ..
            } => {
                assert!(matches!(value_type, Ty::Int { .. }));
                assert_eq!(entries.len(), 2);
            }
            other => panic!("Expected Map, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_cli_value_class_via_json_has_class_name() {
        let ty = Ty::Class(tn("User"), Default::default());
        let val = parse_cli_value(r#"{"id":1}"#, &ty).unwrap();
        match val {
            BexExternalValue::Instance { class_name, .. } => {
                assert_eq!(class_name, "User");
            }
            other => panic!("Expected Instance, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_cli_value_list_invalid_json_errors() {
        let ty = Ty::List(Box::new(ty_int()), Default::default());
        assert!(parse_cli_value("not-json", &ty).is_err());
    }

    // ── format_value — user-facing debug rendering ─────────────────

    #[test]
    fn test_format_value_primitives() {
        assert_eq!(format_value(&BexExternalValue::Null), "null");
        assert_eq!(format_value(&BexExternalValue::Int(42)), "42");
        assert_eq!(format_value(&BexExternalValue::Bool(true)), "true");
        // Floats always render with a decimal point (or as `inf`/`nan`).
        assert_eq!(format_value(&BexExternalValue::Float(3.0)), "3.0");
        assert_eq!(format_value(&BexExternalValue::Float(1.5)), "1.5");
        // Strings are debug-quoted so they're unambiguous.
        assert_eq!(
            format_value(&BexExternalValue::String("hi".into())),
            "\"hi\""
        );
    }

    #[test]
    fn test_format_value_array_of_ints() {
        let v = BexExternalValue::Array {
            element_type: ty_int(),
            items: vec![
                BexExternalValue::Int(1),
                BexExternalValue::Int(2),
                BexExternalValue::Int(3),
            ],
        };
        assert_eq!(format_value(&v), "[1, 2, 3]");
    }

    #[test]
    fn test_format_value_instance_named() {
        let v = BexExternalValue::Instance {
            class_name: "User".into(),
            fields: [
                ("id".to_string(), BexExternalValue::Int(1)),
                ("name".to_string(), BexExternalValue::String("a".into())),
            ]
            .into_iter()
            .collect(),
        };
        let rendered = format_value(&v);
        assert!(rendered.starts_with("User {"), "got: {rendered}");
        assert!(rendered.contains("id: 1"));
        assert!(rendered.contains("name: \"a\""));
    }

    #[test]
    fn test_format_value_instance_anonymous_omits_class_name() {
        let v = BexExternalValue::Instance {
            class_name: String::new(),
            fields: [("x".to_string(), BexExternalValue::Int(1))]
                .into_iter()
                .collect(),
        };
        // No leading "<name> " when class_name is empty.
        assert!(!format_value(&v).starts_with(' '));
        assert_eq!(format_value(&v), "{x: 1}");
    }

    #[test]
    fn test_format_value_variant_renders_variant_name() {
        let v = BexExternalValue::Variant {
            enum_name: "Color".into(),
            variant_name: "Red".into(),
        };
        assert_eq!(format_value(&v), "Red");
    }

    // ── external_to_json — extra shapes ────────────────────────────

    #[test]
    fn test_external_to_json_instance_adds_type_tag() {
        let v = BexExternalValue::Instance {
            class_name: "User".into(),
            fields: [("id".to_string(), BexExternalValue::Int(1))]
                .into_iter()
                .collect(),
        };
        let j = external_to_json(&v);
        assert_eq!(j["__type"], serde_json::json!("User"));
        assert_eq!(j["id"], serde_json::json!(1));
    }

    #[test]
    fn test_external_to_json_instance_anonymous_no_type_tag() {
        let v = BexExternalValue::Instance {
            class_name: String::new(),
            fields: [("id".to_string(), BexExternalValue::Int(1))]
                .into_iter()
                .collect(),
        };
        let j = external_to_json(&v);
        assert!(j.get("__type").is_none());
        assert_eq!(j["id"], serde_json::json!(1));
    }

    #[test]
    fn test_external_to_json_variant_shape() {
        let v = BexExternalValue::Variant {
            enum_name: "Color".into(),
            variant_name: "Red".into(),
        };
        let j = external_to_json(&v);
        assert_eq!(j, serde_json::json!({"__type": "Color", "value": "Red"}));
    }

    // ── example_value — used by --help output ──────────────────────

    #[test]
    fn test_example_value_covers_all_common_types() {
        assert_eq!(example_value(&ty_string()), "\"value\"");
        assert_eq!(example_value(&ty_int()), "42");
        assert_eq!(example_value(&ty_float()), "3.14");
        assert_eq!(example_value(&ty_bool()), "true");
        assert_eq!(
            example_value(&Ty::Null {
                attr: Default::default(),
            }),
            "null"
        );
        assert_eq!(
            example_value(&Ty::Enum(tn("E"), Default::default())),
            "VariantName"
        );
        // Complex/unknown types fall back to `...`.
        assert_eq!(
            example_value(&Ty::List(Box::new(ty_int()), Default::default())),
            "..."
        );
    }

    // ========================================================================
    // BEP-027 §"Target resolution" — resolve_target (requires a real engine)
    //
    // These tests compile a small BAML program, construct a `BexEngine`, and
    // drive `RunArgs::resolve_target` to verify each resolution branch the
    // BEP specifies:
    //
    //   1. Ends in `.baml` → hermetic standalone file → run `main`.
    //   2. Matches a `[scripts]` key → expand alias.
    //   3. Matches a top-level namespace with `main` → run `<ns>.main`.
    //   4. Otherwise → error with "did you mean…".
    //
    // Plus the separate "--function priority" and "no-target" rules.
    // ========================================================================

    /// Build a `BexEngine` from inline BAML source for resolve_target tests.
    fn engine_from_source(source: &str) -> BexEngine {
        let snapshot = baml_tests::engine::compile_source(source);
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("BexEngine::new should succeed")
    }

    /// RunArgs pointing at an empty tempdir so `load_scripts()` sees no
    /// `baml.toml` and doesn't pick up the host project's config.
    fn run_args_with_clean_from(dir: &std::path::Path) -> RunArgs {
        let mut args = run_args();
        args.from = dir.to_path_buf();
        args
    }

    /// BEP-027: "With no target, `baml run` runs the root namespace's `main`."
    #[test]
    fn test_bep_resolve_no_target_runs_root_main() {
        let engine = engine_from_source("function main() -> int { 42 }");
        let tmp = tempfile::tempdir().unwrap();
        let args = run_args_with_clean_from(tmp.path());

        match args.resolve_target(&engine).unwrap() {
            ResolvedTarget::Function(name) => assert_eq!(name, "main"),
            other => panic!("expected Function(main), got {other:?}"),
        }
    }

    /// BEP-027: "Error if there is none" (no target, no root main).
    #[test]
    fn test_bep_resolve_no_target_no_main_is_error() {
        let engine = engine_from_source("function other() -> int { 1 }");
        let tmp = tempfile::tempdir().unwrap();
        let args = run_args_with_clean_from(tmp.path());

        let err = args.resolve_target(&engine).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("No `main` function"), "got: {msg}");
        assert!(msg.contains("--function"), "should suggest --function");
        assert!(msg.contains("--list"), "should suggest --list");
    }

    /// BEP-027: "--function takes priority over positional target".
    #[test]
    fn test_bep_resolve_function_flag_overrides_target() {
        let engine = engine_from_source(
            r#"
                function main() -> int { 1 }
                function Summarize(text: string) -> string { text }
            "#,
        );
        let tmp = tempfile::tempdir().unwrap();
        let mut args = run_args_with_clean_from(tmp.path());
        args.target = Some(s("backfill"));
        args.function = Some(s("Summarize"));

        match args.resolve_target(&engine).unwrap() {
            ResolvedTarget::Function(name) => assert_eq!(name, "Summarize"),
            other => panic!("expected Function(Summarize), got {other:?}"),
        }
    }

    /// --function with an unknown name surfaces a helpful error.
    #[test]
    fn test_bep_resolve_function_flag_unknown_errors() {
        let engine = engine_from_source(
            r#"
                function Summarize(text: string) -> string { text }
            "#,
        );
        let tmp = tempfile::tempdir().unwrap();
        let mut args = run_args_with_clean_from(tmp.path());
        args.function = Some(s("DoesNotExist"));

        let err = args.resolve_target(&engine).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("No runnable target"),
            "error should mention unknown target: {msg}"
        );
    }

    /// BEP-027 §"Hermetic single-file mode": a `.baml` target runs `main`
    /// from the loaded file (the caller is responsible for having loaded
    /// the engine in standalone mode).
    #[test]
    fn test_bep_resolve_baml_file_runs_main() {
        let engine = engine_from_source("function main() -> int { 7 }");
        let tmp = tempfile::tempdir().unwrap();
        let mut args = run_args_with_clean_from(tmp.path());
        args.target = Some(s("hello.baml"));

        match args.resolve_target(&engine).unwrap() {
            ResolvedTarget::Function(name) => assert_eq!(name, "main"),
            other => panic!("expected Function(main), got {other:?}"),
        }
    }

    /// BEP-027: `.baml` target without a `main` is a clear error telling
    /// the user about `--function`.
    #[test]
    fn test_bep_resolve_baml_file_without_main_errors() {
        let engine = engine_from_source("function helper() -> int { 1 }");
        let tmp = tempfile::tempdir().unwrap();
        let mut args = run_args_with_clean_from(tmp.path());
        args.target = Some(s("scratch.baml"));

        let err = args.resolve_target(&engine).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("no `main`"), "got: {msg}");
        assert!(msg.contains("--function"));
    }

    /// BEP-027 §"Target resolution" step 4: unknown positional target errors
    /// with a helpful message.
    #[test]
    fn test_bep_resolve_unknown_target_errors() {
        let engine = engine_from_source(
            r#"
                function Summarize(text: string) -> string { text }
            "#,
        );
        let tmp = tempfile::tempdir().unwrap();
        let mut args = run_args_with_clean_from(tmp.path());
        args.target = Some(s("not_a_thing"));

        let err = args.resolve_target(&engine).unwrap_err();
        assert!(format!("{err}").contains("No runnable target"));
    }

    /// BEP-027: "Matches a top-level namespace with a `main` → run that
    /// namespace's `main`." Namespaces are derived from `ns_*` folder
    /// segments (see `baml_compiler2_hir::file_package`).
    #[test]
    fn test_bep_resolve_namespace_main_via_load_and_compile() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        std::fs::write(project.join("baml.toml"), "[project]\nname = \"test\"\n").unwrap();
        let baml_src = project.join("baml_src");
        let ns_eval = baml_src.join("ns_eval");
        std::fs::create_dir_all(&ns_eval).unwrap();
        // Root namespace: no main.
        std::fs::write(
            baml_src.join("root.baml"),
            "function other() -> int { 1 }\n",
        )
        .unwrap();
        // `eval` namespace: ns_eval/main.baml.
        std::fs::write(ns_eval.join("main.baml"), "function main() -> int { 99 }\n").unwrap();

        let mut args = run_args();
        args.from = project.to_path_buf();
        args.target = Some(s("eval"));

        let (_db, engine) = args
            .load_and_compile(None, Vec::new())
            .expect("compile should succeed");
        match args.resolve_target(&engine).unwrap() {
            ResolvedTarget::Function(name) => assert_eq!(name, "eval.main"),
            other => panic!("expected Function(eval.main), got {other:?}"),
        }
    }

    /// BEP-027 §"Scripts in baml.toml": a positional target matching a
    /// `[scripts]` key expands the alias.
    #[test]
    fn test_bep_resolve_script_alias_expands() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        std::fs::write(
            project.join("baml.toml"),
            r#"
[scripts]
dev = "-- --verbose=true"
backfill = "--function scripts.Backfill"
"#,
        )
        .unwrap();
        let baml_src = project.join("baml_src");
        let ns_scripts = baml_src.join("ns_scripts");
        std::fs::create_dir_all(&ns_scripts).unwrap();
        std::fs::write(
            ns_scripts.join("routines.baml"),
            "function Backfill(start_date: string) -> int { 0 }\n",
        )
        .unwrap();
        std::fs::write(baml_src.join("root.baml"), "function main() -> int { 0 }\n").unwrap();

        // `baml run dev` → script expansion with trailing extra_args.
        let mut args_dev = run_args();
        args_dev.from = project.to_path_buf();
        args_dev.target = Some(s("dev"));
        let (_db, engine) = args_dev.load_and_compile(None, Vec::new()).unwrap();
        match args_dev.resolve_target(&engine).unwrap() {
            ResolvedTarget::Script(expansion) => {
                assert!(expansion.function.is_none());
                assert_eq!(expansion.extra_args, vec!["--verbose=true"]);
            }
            other => panic!("expected Script, got {other:?}"),
        }

        // `baml run backfill` → expands to `--function scripts.Backfill`.
        let mut args_bf = run_args();
        args_bf.from = project.to_path_buf();
        args_bf.target = Some(s("backfill"));
        let (_db, engine) = args_bf.load_and_compile(None, Vec::new()).unwrap();
        match args_bf.resolve_target(&engine).unwrap() {
            ResolvedTarget::Script(expansion) => {
                assert_eq!(expansion.function.as_deref(), Some("scripts.Backfill"));
            }
            other => panic!("expected Script, got {other:?}"),
        }
    }

    /// BEP-027: "The toml loader type-checks each script body at load time."
    /// Tests `load_scripts` directly — both cargo-style forms:
    ///   - string form (split on whitespace, no shell quoting)
    ///   - array form (each element verbatim, preserves values with spaces)
    #[test]
    fn test_load_scripts_string_and_array_forms() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        std::fs::write(
            project.join("baml.toml"),
            r#"
[scripts]
# Cargo-style string form: split on whitespace.
dev = "-- --verbose=true --model gpt-4o-mini"
# Array form: each element verbatim — preserves values containing spaces.
greet = ["--function", "g.Hello", "--", "--name", "Ada Lovelace"]
"#,
        )
        .unwrap();

        let args = run_args_with_clean_from(project);
        let scripts = args.load_scripts();

        let dev = scripts.get("dev").expect("dev alias should load");
        assert_eq!(
            dev,
            &vec![
                "--".to_string(),
                "--verbose=true".to_string(),
                "--model".to_string(),
                "gpt-4o-mini".to_string()
            ]
        );

        let greet = scripts.get("greet").expect("greet alias should load");
        // Array form preserves "Ada Lovelace" as a single token.
        assert_eq!(
            greet,
            &vec![
                "--function".to_string(),
                "g.Hello".to_string(),
                "--".to_string(),
                "--name".to_string(),
                "Ada Lovelace".to_string(),
            ]
        );
    }

    /// Missing `baml.toml` is not an error — `load_scripts` returns an empty
    /// map so projects without scripts still work.
    #[test]
    fn test_load_scripts_missing_toml_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let args = run_args_with_clean_from(tmp.path());
        assert!(args.load_scripts().is_empty());
    }

    /// A `baml.toml` with no `[scripts]` table is also non-fatal.
    #[test]
    fn test_load_scripts_no_scripts_table_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("baml.toml"), "[project]\nname = \"x\"\n").unwrap();
        let args = run_args_with_clean_from(tmp.path());
        assert!(args.load_scripts().is_empty());
    }

    /// BEP-027: a positional target matching a direct function name (rare —
    /// the preferred form is `--function`) still resolves.
    #[test]
    fn test_resolve_positional_direct_function_name() {
        let engine = engine_from_source(
            r#"
                function Summarize(text: string) -> string { text }
            "#,
        );
        let tmp = tempfile::tempdir().unwrap();
        let mut args = run_args_with_clean_from(tmp.path());
        args.target = Some(s("Summarize"));

        match args.resolve_target(&engine).unwrap() {
            ResolvedTarget::Function(name) => {
                // May be qualified internally; CLI pass-through keeps the
                // user-typed form.
                assert!(
                    name == "Summarize" || name.ends_with(".Summarize"),
                    "got: {name}"
                );
            }
            other => panic!("expected Function, got {other:?}"),
        }
    }
}
