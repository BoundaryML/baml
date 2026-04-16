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
    pub fn run(&self) -> Result<crate::ExitCode> {
        // Set up event sink for --log-file.
        let event_sink: Option<Arc<dyn bex_events::EventSink>> =
            self.log_file.as_ref().map(|path| {
                if self.verbose {
                    eprintln!("[verbose] Writing logs to {}", path.display());
                }
                bex_events_native::start(path.clone())
            });

        // Expression mode: -e '<expr>'
        if let Some(expr_source) = &self.expression {
            return self.run_expression(expr_source, event_sink);
        }

        // Detect hermetic standalone file mode: target ends in .baml.
        let (db, engine) = if self.target.as_ref().is_some_and(|t| t.ends_with(".baml")) {
            self.load_and_compile_standalone(self.target.as_ref().unwrap(), event_sink.clone())?
        } else {
            self.load_and_compile(event_sink.clone())?
        };
        let _ = db; // keep db alive for engine lifetime

        if self.list {
            return self.run_list(&engine);
        }

        // Resolve which function to call.
        let resolved = self.resolve_target(&engine)?;

        // For scripts, merge script args with CLI args.
        let (function_name, effective_target_args) = match resolved {
            ResolvedTarget::Function(name) => (name, self.target_args.clone()),
            ResolvedTarget::Script(expansion) => {
                let func = expansion.function.unwrap_or_else(|| "main".to_string());
                if !engine.function_exists(&func) {
                    return Err(self.function_not_found_error(&engine, &func));
                }
                // Script args come first, CLI args (after --) override/append.
                let mut merged_args = expansion.extra_args;
                merged_args.extend(self.target_args.iter().cloned());
                (func, merged_args)
            }
        };

        // Get function params for auto-CLI and arg building.
        let params = engine
            .function_params(&function_name)
            .map_err(|e| anyhow!("{e}"))?;
        let param_names: Vec<String> = params.iter().map(|(n, _)| (*n).to_string()).collect();
        let param_types: Vec<Ty> = params.iter().map(|(_, t)| (*t).clone()).collect();

        // Per-target --help: only when --help is the sole arg and came from
        // the user (not from forwarded extras after `--`). When --help appears
        // alongside other target args it's a forwarded value, not a help request.
        // Per BEP-027: "everything after `--` is forwarded to the function
        // without being parsed against the signature."
        if effective_target_args.len() == 1 && effective_target_args[0] == "--help" {
            self.print_target_help(&function_name, &param_names, &param_types, &engine);
            return Ok(crate::ExitCode::Success);
        }

        // Parse arguments (JSON + auto-CLI merge).
        let args = self.build_args_from(&effective_target_args, &param_names, &param_types)?;

        if self.verbose {
            eprintln!(
                "[verbose] Calling {function_name} with {} arg(s)",
                args.len()
            );
        }

        // Execute the function.
        let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;
        let engine = Arc::new(engine);
        let start = std::time::Instant::now();
        let result = rt.block_on(engine.call_function(
            &function_name,
            args,
            FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        ));

        if self.verbose {
            eprintln!("[verbose] Completed in {:.2?}", start.elapsed());
        }

        // Flush event sink before printing result.
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

    /// Load the project, check diagnostics, compile to bytecode, create engine.
    fn load_and_compile(
        &self,
        event_sink: Option<Arc<dyn bex_events::EventSink>>,
    ) -> Result<(ProjectDatabase, BexEngine)> {
        let from = std::fs::canonicalize(&self.from)
            .with_context(|| format!("Could not resolve project path: {}", self.from.display()))?;

        if self.verbose {
            eprintln!("[verbose] Loading project from {}", from.display());
        }

        // Set up the compiler database and load all .baml files.
        let mut db = ProjectDatabase::new();
        let project = db.set_project_root(&from);
        let baml_files = discover_baml_files(&from);
        if baml_files.is_empty() {
            anyhow::bail!("No .baml files found in {}", from.display());
        }

        if self.verbose {
            eprintln!("[verbose] Found {} .baml file(s)", baml_files.len());
        }

        for file_path in &baml_files {
            let content = std::fs::read_to_string(file_path)
                .with_context(|| format!("Failed to read {}", file_path.display()))?;
            db.add_or_update_file(file_path, &content);
        }

        // Check for diagnostic errors.
        let source_files = db.get_source_files();
        let diagnostics = baml_project::collect_diagnostics(&db, project, &source_files);
        let warnings: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == baml_db::baml_compiler_diagnostics::Severity::Warning)
            .collect();
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();

        if self.verbose && !warnings.is_empty() {
            for diag in &warnings {
                eprintln!("[verbose] warning: {}", diag.message);
            }
        }

        if !errors.is_empty() {
            eprintln!("Compilation errors ({}):", errors.len());
            for diag in &errors {
                eprintln!("  error: {}", diag.message);
            }
            anyhow::bail!("Cannot run: compilation errors found");
        }

        // Compile to bytecode.
        if self.verbose {
            eprintln!("[verbose] Compiling...");
        }
        let compile_options = baml_compiler2_emit::CompileOptions {
            emit_test_cases: false,
        };
        let bytecode = baml_compiler2_emit::generate_project_bytecode(&db, &compile_options)
            .map_err(|e| anyhow!("Compilation failed: {e:?}"))?;

        // Create the engine.
        let engine = BexEngine::new(bytecode, Arc::new(sys_native::SysOps::native()), event_sink)
            .map_err(|e| anyhow!("Failed to create engine: {e:?}"))?;

        if self.verbose {
            let funcs = engine.user_functions();
            eprintln!("[verbose] Compiled {} user function(s)", funcs.len());
        }

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
    ) -> Result<(ProjectDatabase, BexEngine)> {
        let path = std::path::Path::new(file_path);
        let canonical =
            std::fs::canonicalize(path).with_context(|| format!("File not found: {file_path}"))?;

        if self.verbose {
            eprintln!("[verbose] Standalone mode: loading {}", canonical.display());
        }

        let content = std::fs::read_to_string(&canonical)
            .with_context(|| format!("Failed to read {}", canonical.display()))?;

        // Use the file's parent directory as the project root (for relative path resolution).
        let parent = canonical
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));

        let mut db = ProjectDatabase::new();
        let project = db.set_project_root(parent);
        db.add_or_update_file(&canonical, &content);

        // Check for diagnostic errors.
        let source_files = db.get_source_files();
        let diagnostics = baml_project::collect_diagnostics(&db, project, &source_files);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        if !errors.is_empty() {
            eprintln!("Compilation errors ({}):", errors.len());
            for diag in &errors {
                eprintln!("  error: {}", diag.message);
            }
            anyhow::bail!("Cannot run: compilation errors in {file_path}");
        }

        // Compile to bytecode.
        let compile_options = baml_compiler2_emit::CompileOptions {
            emit_test_cases: false,
        };
        let bytecode = baml_compiler2_emit::generate_project_bytecode(&db, &compile_options)
            .map_err(|e| anyhow!("Compilation failed: {e:?}"))?;

        let engine = BexEngine::new(bytecode, Arc::new(sys_native::SysOps::native()), event_sink)
            .map_err(|e| anyhow!("Failed to create engine: {e:?}"))?;

        if self.verbose {
            let funcs = engine.user_functions();
            eprintln!(
                "[verbose] Compiled {} function(s) from standalone file",
                funcs.len()
            );
        }

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
        // Load the expression source.
        let expr_body = load_expression_source(source)?;

        if self.verbose {
            eprintln!(
                "[verbose] Expression mode: evaluating {} byte(s)",
                expr_body.len()
            );
        }

        // Wrap in a synthetic function. Use `-> unknown` so any return type is accepted.
        let synthetic = format!("function baml_run_expr_main__() -> unknown {{\n{expr_body}\n}}");

        // Expression mode uses a minimal project context.
        // If --from points to a directory with a baml.toml or baml_src, load it.
        // Otherwise, use a temp directory with just the synthetic file.
        // The default --from is "." (cwd), which always exists. If the user
        // explicitly passed a different path and it doesn't exist, that's an error.
        let from = match std::fs::canonicalize(&self.from) {
            Ok(path) => Some(path),
            Err(_) if self.from == Path::new(".") => None,
            Err(e) => anyhow::bail!("Cannot resolve --from path `{}`: {e}", self.from.display()),
        };

        let mut db = ProjectDatabase::new();

        // Check if there's an explicit project (baml.toml or baml_src directory).
        let has_explicit_project = from
            .as_ref()
            .is_some_and(|f| f.join("baml.toml").exists() || f.join("baml_src").exists());

        let project_root = if has_explicit_project {
            let root = from.as_ref().unwrap();
            let project = db.set_project_root(root);
            // Load only baml_src if it exists, not the whole directory.
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
            if self.verbose {
                eprintln!(
                    "[verbose] Project context: loaded {} file(s)",
                    baml_files.len()
                );
            }
            let _ = project;
            root.clone()
        } else {
            // No project — use a temp directory.
            let tmp = std::env::temp_dir().join("baml_expr");
            std::fs::create_dir_all(&tmp).ok();
            db.set_project_root(&tmp);
            if self.verbose {
                eprintln!("[verbose] Project context: none (standalone expression)");
            }
            tmp
        };

        // Inject the synthetic expression file.
        let synthetic_path = project_root.join("__expr__.baml");
        db.add_or_update_file(&synthetic_path, &synthetic);

        // Check for diagnostic errors.
        let source_files = db.get_source_files();
        let project = db
            .get_project()
            .ok_or_else(|| anyhow!("No project context"))?;
        let diagnostics = baml_project::collect_diagnostics(&db, project, &source_files);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        if !errors.is_empty() {
            eprintln!("Expression errors ({}):", errors.len());
            for diag in &errors {
                eprintln!("  error: {}", diag.message);
            }
            anyhow::bail!("Cannot evaluate expression: compilation errors");
        }

        // Compile and run.
        let compile_options = baml_compiler2_emit::CompileOptions {
            emit_test_cases: false,
        };
        let bytecode = baml_compiler2_emit::generate_project_bytecode(&db, &compile_options)
            .map_err(|e| anyhow!("Compilation failed: {e:?}"))?;

        let engine = BexEngine::new(
            bytecode,
            Arc::new(sys_native::SysOps::native()),
            event_sink.clone(),
        )
        .map_err(|e| anyhow!("Failed to create engine: {e:?}"))?;

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
                    if self.verbose {
                        eprintln!("[verbose] Expanding script `{target}`: {script_tokens:?}");
                    }
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
        // Parse --json-args if present.
        let json_map = match &self.json_args {
            Some(source) => {
                let json = load_json_source(source)?;
                let obj = json
                    .as_object()
                    .ok_or_else(|| anyhow!("--json-args must be a JSON object, got: {json}"))?;
                let mut map = HashMap::new();
                for (key, value) in obj {
                    map.insert(key.clone(), json_to_external(value));
                }
                map
            }
            None => HashMap::new(),
        };

        // Parse auto-CLI flags from target_args (tokens after --).
        let cli_map = parse_auto_cli_args(target_args, param_names, param_types)?;

        // Merge: CLI overrides JSON.
        let mut merged = json_map;
        for (key, value) in cli_map {
            merged.insert(key, value);
        }

        // Build ordered args matching function parameter order.
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

        // Warn about unknown args.
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

        // Group by namespace prefix, sorting by (namespace, short_name).
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
        // Display name: strip "user." prefix.
        let display = function_name.strip_prefix("user.").unwrap_or(function_name);

        let ret_type = engine.function_params(function_name).ok().and_then(|_| {
            // Get return type from user_functions.
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
        // No tokens to parse, or no params to bind.
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

        let raw = &token[2..]; // strip leading "--"

        // Handle --name=value syntax.
        if let Some(eq_pos) = raw.find('=') {
            let key = &raw[..eq_pos];
            let val_str = &raw[eq_pos + 1..];

            let param_idx = find_param_index(key, param_names)?;
            let value = parse_cli_value(val_str, &param_types[param_idx])
                .with_context(|| format!("Invalid value for `--{key}`: {val_str}"))?;
            args.insert(key.to_string(), value);
        } else {
            // --name value (two tokens).
            let key = raw;
            let param_idx = find_param_index(key, param_names)?;

            i += 1;
            if i >= tokens.len() {
                anyhow::bail!("Missing value for `--{key}`");
            }
            let val_str = &tokens[i];
            let value = parse_cli_value(val_str, &param_types[param_idx])
                .with_context(|| format!("Invalid value for `--{key}`: {val_str}"))?;
            args.insert(key.to_string(), value);
        }

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

        // Complex types: user should use --json-args.
        Ty::Class(..) | Ty::Map { .. } | Ty::List(..) | Ty::Union(..) => {
            // Try parsing as JSON as a convenience.
            match serde_json::from_str::<serde_json::Value>(raw) {
                Ok(json) => Ok(json_to_external(&json)),
                Err(_) => anyhow::bail!(
                    "Parameter type `{ty}` requires JSON.\n\
                     Use `--json-args '{{...}}'` or pass a JSON string for this parameter."
                ),
            }
        }

        _ => {
            // Fallback: try as string.
            Ok(BexExternalValue::String(raw.to_string()))
        }
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

/// Recursively convert a `serde_json::Value` to `BexExternalValue`.
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
        let dir = std::env::temp_dir().join("baml_test_json");
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
        let dir = std::env::temp_dir().join("baml_test_expr");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("expr.baml");
        std::fs::write(&path, "let x = 42\nx").unwrap();

        let source = format!("@{}", path.display());
        let body = load_expression_source(&source).unwrap();
        assert_eq!(body, "let x = 42\nx");

        let _ = std::fs::remove_dir_all(dir);
    }
}
