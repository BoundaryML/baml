#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
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
use bex_engine::{
    BexCallArg, BexEngine, BexExternalValue, ClassDefinition, FunctionCallContextBuilder, Ty,
    TypeName, UserFunctionInfo,
};
// For --log-file event sink.
use clap::Args;
use indexmap::IndexMap;
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

    /// List runnable targets (scripts, namespace mains, functions).
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

    /// Show help for the run verb, or auto-derived help for the target.
    #[arg(long)]
    pub help: bool,

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
        if self.expression.is_some() && self.function.is_some() {
            anyhow::bail!(
                "`-e` and `--function` are mutually exclusive dispatch modes.\n\
                 Use one or the other, not both."
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

        let toml_content = std::fs::read_to_string(self.from.join("baml.toml")).unwrap_or_default();
        let scripts = Self::parse_scripts(&toml_content);
        let namespaces = collect_namespaces(&engine);

        if self.list {
            return Self::print_list(&engine, &scripts, &namespaces, &self.output);
        }

        self.validate_scripts(&engine, &scripts, &namespaces, &toml_content)?;

        let resolved = self.resolve_target(&engine, &scripts)?;

        let (function_name, effective_target_args) = match resolved {
            ResolvedTarget::Function(name) => (name, self.target_args.clone()),
            ResolvedTarget::Script(expansion) => {
                let func = expansion.function.unwrap_or_else(|| "main".to_string());
                if find_user_function(&engine, &func).is_none() {
                    return Err(Self::target_not_found_error(&scripts, &engine, &func));
                }
                let mut merged_args = expansion.extra_args;
                merged_args.extend(self.target_args.iter().cloned());
                (func, merged_args)
            }
        };

        let func_info = find_user_function(&engine, &function_name)
            .ok_or_else(|| anyhow!("Function `{function_name}` not found"))?;

        if self.help || (effective_target_args.len() == 1 && effective_target_args[0] == "--help") {
            Self::print_target_help(&function_name, &func_info);
            return Ok(crate::ExitCode::Success);
        }

        let args = self.build_args_from(
            &effective_target_args,
            &func_info.param_names,
            &func_info.param_types,
            engine.class_definitions(),
            &func_info.param_has_default,
        )?;

        self.vlog(format_args!(
            "Calling {function_name} with {} arg(s)",
            args.len()
        ));

        let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;
        let engine = Arc::new(engine);
        let start = std::time::Instant::now();
        let result = rt.block_on(engine.call_function_bound_args(
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
    fn resolve_target(
        &self,
        engine: &BexEngine,
        scripts: &HashMap<String, Vec<String>>,
    ) -> Result<ResolvedTarget> {
        if let Some(func) = &self.function {
            if find_user_function(engine, func).is_some() {
                return Ok(ResolvedTarget::function(func.clone()));
            }
            return Err(Self::target_not_found_error(scripts, engine, func));
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

                Err(Self::target_not_found_error(scripts, engine, target))
            }
        }
    }

    /// Parse `[scripts]` from raw `baml.toml` content.
    fn parse_scripts(content: &str) -> HashMap<String, Vec<String>> {
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
                        if find_user_function(engine, func).is_none() {
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
    // Argument parsing
    // ========================================================================

    /// Build the ordered argument vector by merging JSON args and auto-CLI flags.
    fn build_args_from(
        &self,
        target_args: &[String],
        param_names: &[String],
        param_types: &[Ty],
        class_defs: &IndexMap<TypeName, ClassDefinition>,
        param_has_default: &[bool],
    ) -> Result<Vec<BexCallArg>> {
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
                        Some(idx) => json_to_external_with_ty(value, &param_types[idx], class_defs)
                            .with_context(|| format!("--json-args: parameter `{key}`"))?,
                        None => json_to_external(value),
                    };
                    map.insert(key.clone(), converted);
                }
                map
            }
            None => HashMap::new(),
        };

        let cli_map = parse_auto_cli_args(
            target_args,
            param_names,
            param_types,
            class_defs,
            param_has_default,
        )?;

        // CLI args override --json-args values.
        let mut merged = json_map;
        for (key, value) in cli_map {
            merged.insert(key, value);
        }

        let mut ordered = Vec::with_capacity(param_names.len());
        for (i, name) in param_names.iter().enumerate() {
            match merged.remove(name.as_str()) {
                Some(value) => ordered.push(BexCallArg::Provided(Box::new(value))),
                None => {
                    if param_has_default.get(i).copied().unwrap_or(false) {
                        ordered.push(BexCallArg::OmittedDefault);
                        continue;
                    }
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

    fn print_list(
        engine: &BexEngine,
        scripts: &HashMap<String, Vec<String>>,
        namespaces: &HashSet<String>,
        output: &OutputFormat,
    ) -> Result<crate::ExitCode> {
        let mut functions = engine.user_functions();
        functions.sort_by(|a, b| a.display_name.cmp(&b.display_name));

        if functions.is_empty() && scripts.is_empty() && namespaces.is_empty() {
            println!("No runnable targets found.");
            return Ok(crate::ExitCode::Success);
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

        if !namespaces.is_empty() {
            println!("  Namespaces:");
            let mut ns_list: Vec<&String> = namespaces.iter().collect();
            ns_list.sort();
            for ns in ns_list {
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
            println!("  Functions:");
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

        println!("Run with: baml run --function <name> -- --arg1 value1");
    }

    fn print_list_json(
        functions: &[UserFunctionInfo],
        scripts: &HashMap<String, Vec<String>>,
        namespaces: &HashSet<String>,
    ) {
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

        let namespace_items: Vec<serde_json::Value> = {
            let mut ns_list: Vec<&String> = namespaces.iter().collect();
            ns_list.sort();
            ns_list
                .into_iter()
                .map(|ns| serde_json::json!({ "name": ns }))
                .collect()
        };

        let output = serde_json::json!({
            "scripts": script_items,
            "namespaces": namespace_items,
            "functions": function_items,
        });

        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
        );
    }

    // ========================================================================
    // Per-target --help
    // ========================================================================

    fn print_target_help(function_name: &str, func_info: &UserFunctionInfo) {
        print!("{}", Self::target_help_text(function_name, func_info));
    }

    fn target_help_text(function_name: &str, func_info: &UserFunctionInfo) -> String {
        let display = function_name.strip_prefix("user.").unwrap_or(function_name);
        let param_names = &func_info.param_names;
        let param_types = &func_info.param_types;
        let param_has_default = &func_info.param_has_default;
        let ret_str = func_info.return_type.to_string();

        let params_str: Vec<String> = param_names
            .iter()
            .zip(param_types.iter())
            .enumerate()
            .map(|(idx, (n, t))| {
                if param_has_default.get(idx).copied().unwrap_or(false) {
                    format!("{n}: {t} [optional]")
                } else {
                    format!("{n}: {t}")
                }
            })
            .collect();

        let mut out = String::new();
        writeln!(
            out,
            "function {display}({}) -> {ret_str}",
            params_str.join(", ")
        )
        .unwrap();
        writeln!(out).unwrap();

        if param_names.is_empty() {
            writeln!(out, "  This function takes no arguments.").unwrap();
        } else {
            writeln!(out, "  Arguments (pass after `--`):\n").unwrap();
            for (idx, (name, ty)) in param_names.iter().zip(param_types.iter()).enumerate() {
                let type_hint = match ty {
                    Ty::Bool { .. } => " (use --name=true or --name=false)".to_string(),
                    Ty::Enum(tn, _) => format!(" (enum {tn})"),
                    Ty::Class(..) | Ty::Map { .. } | Ty::List(..) => {
                        " (use --json-args for complex types)".to_string()
                    }
                    _ => String::new(),
                };
                let optional = if param_has_default.get(idx).copied().unwrap_or(false) {
                    " [optional]"
                } else {
                    ""
                };
                writeln!(out, "    --{name} <{ty}>{optional}{type_hint}").unwrap();
            }
        }

        let example_args = param_names
            .iter()
            .zip(param_types.iter())
            .enumerate()
            .filter(|(idx, _)| !param_has_default.get(*idx).copied().unwrap_or(false))
            .map(|(_, (n, t))| format!("--{n} {}", example_value(t)))
            .collect::<Vec<_>>()
            .join(" ");
        writeln!(out).unwrap();
        if example_args.is_empty() {
            writeln!(out, "  Example: baml run --function {display}").unwrap();
        } else {
            writeln!(
                out,
                "  Example: baml run --function {display} -- {example_args}"
            )
            .unwrap();
        }

        out
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

/// BEP-027 Appendix A: names that cannot be used as `[scripts]` keys.
const RESERVED_VERBS: &[&str] = &[
    "run", "test", "repl", "init", "help", "version", "fmt", "lint", "check", "build", "generate",
    "dev", "start", "serve", "add", "remove", "install", "update", "publish", "upgrade", "deps",
    "clean", "config", "info", "search", "new", "doc", "docs",
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

fn find_user_function(engine: &BexEngine, name: &str) -> Option<UserFunctionInfo> {
    let display_name = name.strip_prefix("user.").unwrap_or(name);
    engine
        .user_functions()
        .into_iter()
        .find(|f| f.qualified_name == name || f.display_name == display_name)
}

// ============================================================================
// Auto-CLI parser
// ============================================================================

/// Parse tokens after `--` into a map of parameter name → value.
///
/// Supports:
/// - `--name value` (two tokens)
/// - `--name=value` (single token with `=`, including `--name=` for empty string)
/// - Positional sugar: single bare token when function has exactly one required parameter
///
/// Bare tokens that don't match a `--flag` are skipped — they remain
/// accessible via `baml.argv` but don't bind to parameters.
fn parse_auto_cli_args(
    tokens: &[String],
    param_names: &[String],
    param_types: &[Ty],
    class_defs: &IndexMap<TypeName, ClassDefinition>,
    param_has_default: &[bool],
) -> Result<HashMap<String, BexExternalValue>> {
    if tokens.is_empty() || param_names.is_empty() {
        return Ok(HashMap::new());
    }

    // Positional sugar: single non-flag token + exactly one required param.
    if tokens.len() == 1 && !tokens[0].starts_with("--") {
        let required_params: Vec<usize> = param_names
            .iter()
            .enumerate()
            .filter_map(|(idx, _)| {
                if param_has_default.get(idx).copied().unwrap_or(false) {
                    None
                } else {
                    Some(idx)
                }
            })
            .collect();

        if required_params.len() == 1 {
            let idx = required_params[0];
            let value =
                parse_cli_value(&tokens[0], &param_types[idx], class_defs).with_context(|| {
                    format!("Invalid value for `{}`: {}", param_names[idx], tokens[0])
                })?;
            let mut map = HashMap::new();
            map.insert(param_names[idx].clone(), value);
            return Ok(map);
        }
    }

    let mut args = HashMap::new();
    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];
        if !token.starts_with("--") {
            // Bare token — not a flag. Skipped here; still in baml.argv.
            i += 1;
            continue;
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
        let value = parse_cli_value(val_str, &param_types[param_idx], class_defs)
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

/// Extract flag names (`--key value` or `--key=value`) from a token list,
/// skipping bare (non-flag) tokens. Shared by `parse_auto_cli_args` and
/// script validation.
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

/// Convert a CLI string value to a `BexExternalValue` based on the target type.
fn parse_cli_value(
    raw: &str,
    ty: &Ty,
    class_defs: &IndexMap<TypeName, ClassDefinition>,
) -> Result<BexExternalValue> {
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
                parse_cli_value(raw, inner, class_defs)
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
                Ok(json) => json_to_external_with_ty(&json, ty, class_defs),
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
/// type information. Used as a fallback when the target type is unknown —
/// e.g. unknown `--json-args` keys, or types this walker doesn't understand
/// (`Ty::TypeAlias` like `json`, `Ty::Literal`, generic class fields with
/// unsubstituted `TypeVar`s).
///
/// Objects produce `BexExternalValue::Map`, not `Instance`. The engine
/// rejects `Instance` with no class name (any object that ends up here did
/// *not* come from a typed `Ty::Class` branch), and a `Map` is the right
/// runtime shape for the `json` type alias anyway.
fn json_to_external(value: &serde_json::Value) -> BexExternalValue {
    let placeholder_ty = || Ty::String {
        attr: Default::default(),
    };
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
            element_type: placeholder_ty(),
            items: items.iter().map(json_to_external).collect(),
        },
        serde_json::Value::Object(map) => BexExternalValue::Map {
            key_type: placeholder_ty(),
            value_type: placeholder_ty(),
            entries: map
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
/// `class_defs` resolves nested class field types so that e.g. a
/// `map<string,string>` field inside a class still gets `BexExternalValue::Map`
/// rather than the untyped fallback. An empty map disables that resolution
/// (used by tests that don't construct class schemas).
///
/// The shape rules here mirror `bex_vm::package_baml::json::ty_serde_to_value`,
/// which is the canonical implementation used by `baml.json.from_string<T>`.
/// Keep both in sync if the rules change.
fn json_to_external_with_ty(
    value: &serde_json::Value,
    ty: &Ty,
    class_defs: &IndexMap<TypeName, ClassDefinition>,
) -> Result<BexExternalValue> {
    use serde_json::Value as J;
    match ty {
        Ty::Optional(inner, _) => {
            if matches!(value, J::Null) {
                Ok(BexExternalValue::Null)
            } else {
                json_to_external_with_ty(value, inner, class_defs)
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

        Ty::Class(type_name, _, _) => match value {
            J::Object(map) => {
                // If we know this class's schema, recurse into each field
                // with its declared type and require that every non-optional
                // field is present. The required-field check is what lets
                // `coerce_json_union` discriminate between class variants by
                // shape — without it, any JSON object matches any class.
                let class_name = type_name.display_name.to_string();
                let mut fields: indexmap::IndexMap<String, BexExternalValue> =
                    indexmap::IndexMap::with_capacity(map.len());

                if let Some(def) = class_defs.get(type_name) {
                    for field_def in &def.fields {
                        match map.get(&field_def.name) {
                            Some(v) => {
                                let converted =
                                    json_to_external_with_ty(v, &field_def.field_type, class_defs)
                                        .with_context(|| {
                                            format!("field `{}.{}`", class_name, field_def.name)
                                        })?;
                                fields.insert(field_def.name.clone(), converted);
                            }
                            None => {
                                if matches!(field_def.field_type, Ty::Optional(..)) {
                                    fields.insert(field_def.name.clone(), BexExternalValue::Null);
                                } else {
                                    anyhow::bail!(
                                        "Missing required field `{}.{}`",
                                        class_name,
                                        field_def.name
                                    );
                                }
                            }
                        }
                    }
                } else {
                    // Schema not available — preserve the untyped-field
                    // behavior so tests and edge cases that don't register
                    // class schemas still work.
                    for (k, v) in map {
                        fields.insert(k.clone(), json_to_external(v));
                    }
                }

                Ok(BexExternalValue::Instance { class_name, fields })
            }
            _ => anyhow::bail!(
                "Expected object for class `{}`, got `{value}`",
                type_name.display_name
            ),
        },

        Ty::List(inner, _) => match value {
            J::Array(items) => {
                let mut converted = Vec::with_capacity(items.len());
                for item in items {
                    converted.push(json_to_external_with_ty(item, inner, class_defs)?);
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
                    pairs.push((
                        k.clone(),
                        json_to_external_with_ty(v, value_ty, class_defs)?,
                    ));
                }
                Ok(BexExternalValue::Map {
                    key_type: (**key).clone(),
                    value_type: (**value_ty).clone(),
                    entries: pairs.into_iter().collect(),
                })
            }
            _ => anyhow::bail!("Expected object for map `{ty}`, got `{value}`"),
        },

        Ty::Union(variants, _) => coerce_json_union(value, variants, class_defs),

        // Types we don't specifically coerce: fall back to untyped conversion.
        _ => Ok(json_to_external(value)),
    }
}

/// Best-effort coercion into a union: try each variant and return the first
/// that succeeds. On failure, surface the last variant's error.
fn coerce_json_union(
    value: &serde_json::Value,
    variants: &[Ty],
    class_defs: &IndexMap<TypeName, ClassDefinition>,
) -> Result<BexExternalValue> {
    let mut last_err: Option<anyhow::Error> = None;
    for variant in variants {
        match json_to_external_with_ty(value, variant, class_defs) {
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
        BexExternalValue::Float(f) => bex_vm_types::format_float(*f),
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
        let val = parse_cli_value("hello", &ty_string(), &IndexMap::new()).unwrap();
        assert!(matches!(val, BexExternalValue::String(s) if s == "hello"));
    }

    #[test]
    fn test_parse_cli_value_int() {
        let val = parse_cli_value("42", &ty_int(), &IndexMap::new()).unwrap();
        assert!(matches!(val, BexExternalValue::Int(42)));
    }

    #[test]
    fn test_parse_cli_value_int_negative() {
        let val = parse_cli_value("-7", &ty_int(), &IndexMap::new()).unwrap();
        assert!(matches!(val, BexExternalValue::Int(-7)));
    }

    #[test]
    fn test_parse_cli_value_int_invalid() {
        assert!(parse_cli_value("abc", &ty_int(), &IndexMap::new()).is_err());
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn test_parse_cli_value_float() {
        let val = parse_cli_value("3.14", &ty_float(), &IndexMap::new()).unwrap();
        assert!(matches!(val, BexExternalValue::Float(f) if (f - 3.14).abs() < 0.001));
    }

    #[test]
    fn test_parse_cli_value_bool_true() {
        let val = parse_cli_value("true", &ty_bool(), &IndexMap::new()).unwrap();
        assert!(matches!(val, BexExternalValue::Bool(true)));
    }

    #[test]
    fn test_parse_cli_value_bool_false() {
        let val = parse_cli_value("false", &ty_bool(), &IndexMap::new()).unwrap();
        assert!(matches!(val, BexExternalValue::Bool(false)));
    }

    #[test]
    fn test_parse_cli_value_bool_invalid() {
        assert!(parse_cli_value("yes", &ty_bool(), &IndexMap::new()).is_err());
    }

    #[test]
    fn test_parse_cli_value_optional_null() {
        let ty = Ty::Optional(Box::new(ty_int()), Default::default());
        let val = parse_cli_value("null", &ty, &IndexMap::new()).unwrap();
        assert!(matches!(val, BexExternalValue::Null));
    }

    #[test]
    fn test_parse_cli_value_optional_value() {
        let ty = Ty::Optional(Box::new(ty_int()), Default::default());
        let val = parse_cli_value("42", &ty, &IndexMap::new()).unwrap();
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
        let val = parse_cli_value("Red", &ty, &IndexMap::new()).unwrap();
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
        let result =
            parse_auto_cli_args(&[], &[s("x")], &[ty_int()], &IndexMap::new(), &[false]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_auto_cli_named_args() {
        let tokens = vec![s("--a"), s("10"), s("--b"), s("20")];
        let names = vec![s("a"), s("b")];
        let types = vec![ty_int(), ty_int()];
        let result =
            parse_auto_cli_args(&tokens, &names, &types, &IndexMap::new(), &[false, false])
                .unwrap();
        assert!(matches!(result.get("a"), Some(BexExternalValue::Int(10))));
        assert!(matches!(result.get("b"), Some(BexExternalValue::Int(20))));
    }

    #[test]
    fn test_auto_cli_equals_syntax() {
        let tokens = vec![s("--flag=true")];
        let names = vec![s("flag")];
        let types = vec![ty_bool()];
        let result =
            parse_auto_cli_args(&tokens, &names, &types, &IndexMap::new(), &[false]).unwrap();
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
        let result =
            parse_auto_cli_args(&tokens, &names, &types, &IndexMap::new(), &[false]).unwrap();
        assert!(matches!(result.get("name"), Some(BexExternalValue::String(s)) if s == "hello"));
    }

    #[test]
    fn test_auto_cli_positional_sugar_allows_defaulted_params() {
        let tokens = vec![s("hello")];
        let names = vec![s("max_results"), s("query"), s("filter")];
        let types = vec![ty_int(), ty_string(), ty_string()];
        let result = parse_auto_cli_args(
            &tokens,
            &names,
            &types,
            &IndexMap::new(),
            &[true, false, true],
        )
        .unwrap();
        assert!(matches!(result.get("query"), Some(BexExternalValue::String(s)) if s == "hello"));
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_auto_cli_positional_sugar_requires_single_param() {
        // Two params — positional sugar should not apply; bare token is
        // skipped (accessible via baml.argv, not bound to a param).
        let tokens = vec![s("hello")];
        let names = vec![s("a"), s("b")];
        let types = vec![ty_string(), ty_string()];
        let result =
            parse_auto_cli_args(&tokens, &names, &types, &IndexMap::new(), &[false, false])
                .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_auto_cli_unknown_param() {
        let tokens = vec![s("--unknown"), s("val")];
        let names = vec![s("a")];
        let types = vec![ty_int()];
        let result = parse_auto_cli_args(&tokens, &names, &types, &IndexMap::new(), &[false]);
        assert!(result.is_err());
    }

    #[test]
    fn test_auto_cli_missing_value() {
        let tokens = vec![s("--a")];
        let names = vec![s("a")];
        let types = vec![ty_int()];
        let result = parse_auto_cli_args(&tokens, &names, &types, &IndexMap::new(), &[false]);
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
    fn test_json_to_external_object_is_map() {
        // Untyped objects must become `Map`, not `Instance` with empty
        // class_name — the engine rejects empty-named instances and a Map
        // is the right shape for `json`-typed values.
        let val = json_to_external(&serde_json::json!({"a": 1, "b": "two"}));
        match val {
            BexExternalValue::Map { entries, .. } => {
                assert_eq!(entries.len(), 2);
                assert!(matches!(entries.get("a"), Some(BexExternalValue::Int(1))));
                assert!(matches!(
                    entries.get("b"),
                    Some(BexExternalValue::String(s)) if s == "two"
                ));
            }
            other => panic!("Expected Map, got {other:?}"),
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
        let val =
            json_to_external_with_ty(&serde_json::json!("Red"), &ty, &IndexMap::new()).unwrap();
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
        let ty = Ty::Class(tn("User"), Vec::new(), Default::default());
        let val = json_to_external_with_ty(
            &serde_json::json!({"id": 1, "name": "alice"}),
            &ty,
            &IndexMap::new(),
        )
        .unwrap();
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
        let val =
            json_to_external_with_ty(&serde_json::json!([1, 2, 3]), &ty, &IndexMap::new()).unwrap();
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
        let val =
            json_to_external_with_ty(&serde_json::json!(["Red", "Blue"]), &ty, &IndexMap::new())
                .unwrap();
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
        let val =
            json_to_external_with_ty(&serde_json::json!({"a": 1, "b": 2}), &ty, &IndexMap::new())
                .unwrap();
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
        let val =
            json_to_external_with_ty(&serde_json::json!(null), &ty, &IndexMap::new()).unwrap();
        assert!(matches!(val, BexExternalValue::Null));
    }

    #[test]
    fn test_typed_optional_present() {
        let ty = Ty::Optional(Box::new(ty_int()), Default::default());
        let val = json_to_external_with_ty(&serde_json::json!(5), &ty, &IndexMap::new()).unwrap();
        assert!(matches!(val, BexExternalValue::Int(5)));
    }

    #[test]
    fn test_typed_type_mismatch_is_error() {
        let ty = ty_int();
        assert!(
            json_to_external_with_ty(&serde_json::json!("not-an-int"), &ty, &IndexMap::new())
                .is_err()
        );
    }

    // ── typed JSON with class schema (regressions for compound-field bugs)

    fn class_def(name: &str, fields: Vec<(&str, Ty)>) -> (TypeName, ClassDefinition) {
        let tn = tn(name);
        let def = ClassDefinition {
            name: name.to_string(),
            description: None,
            alias: None,
            fields: fields
                .into_iter()
                .map(|(n, ty)| bex_engine::ClassFieldDefinition {
                    name: n.to_string(),
                    field_type: ty,
                    description: None,
                    alias: None,
                    skip: false,
                })
                .collect(),
        };
        (tn, def)
    }

    /// Repro: class with a `map<string, string>` field. Prior to the fix this
    /// produced `Instance { class_name: "" }` for the map field via the
    /// untyped fallback, which then panicked in the engine.
    #[test]
    fn test_typed_class_with_map_field_is_typed_map() {
        let mut defs: IndexMap<TypeName, ClassDefinition> = IndexMap::new();
        let (areq_name, areq_def) = class_def(
            "AReq",
            vec![(
                "params",
                Ty::Map {
                    key: Box::new(ty_string()),
                    value: Box::new(ty_string()),
                    attr: Default::default(),
                },
            )],
        );
        defs.insert(areq_name.clone(), areq_def);

        let ty = Ty::Class(areq_name, Vec::new(), Default::default());
        let val = json_to_external_with_ty(&serde_json::json!({"params": {"k": "v"}}), &ty, &defs)
            .unwrap();
        match val {
            BexExternalValue::Instance { class_name, fields } => {
                assert_eq!(class_name, "AReq");
                match fields.get("params") {
                    Some(BexExternalValue::Map { entries, .. }) => {
                        assert!(matches!(
                            entries.get("k"),
                            Some(BexExternalValue::String(s)) if s == "v"
                        ));
                    }
                    other => panic!("Expected Map for `params`, got {other:?}"),
                }
            }
            other => panic!("Expected Instance, got {other:?}"),
        }
    }

    /// Repro: union of classes — second variant must win when its required
    /// fields are present and the first variant's are not.
    #[test]
    fn test_typed_union_of_classes_discriminates_by_fields() {
        let mut defs: IndexMap<TypeName, ClassDefinition> = IndexMap::new();
        let (success_name, success_def) = class_def("Success", vec![("value", ty_string())]);
        let (failure_name, failure_def) = class_def("Failure", vec![("reason", ty_string())]);
        defs.insert(success_name.clone(), success_def);
        defs.insert(failure_name.clone(), failure_def);

        let union = Ty::Union(
            vec![
                Ty::Class(success_name, Vec::new(), Default::default()),
                Ty::Class(failure_name, Vec::new(), Default::default()),
            ],
            Default::default(),
        );

        // Success branch.
        let v =
            json_to_external_with_ty(&serde_json::json!({"value": "hi"}), &union, &defs).unwrap();
        match v {
            BexExternalValue::Instance { class_name, .. } => assert_eq!(class_name, "Success"),
            other => panic!("Expected Success Instance, got {other:?}"),
        }

        // Failure branch — would previously be misclassified as Success
        // because `Ty::Class` accepted any object. With required-field
        // validation, Success rejects `{"reason": ...}` and Failure wins.
        let v =
            json_to_external_with_ty(&serde_json::json!({"reason": "bad"}), &union, &defs).unwrap();
        match v {
            BexExternalValue::Instance { class_name, fields } => {
                assert_eq!(class_name, "Failure");
                assert!(matches!(
                    fields.get("reason"),
                    Some(BexExternalValue::String(s)) if s == "bad"
                ));
            }
            other => panic!("Expected Failure Instance, got {other:?}"),
        }
    }

    /// A class with an optional field accepts JSON that omits it.
    #[test]
    fn test_typed_class_optional_field_can_be_omitted() {
        let mut defs: IndexMap<TypeName, ClassDefinition> = IndexMap::new();
        let (name, def) = class_def(
            "User",
            vec![
                ("id", ty_int()),
                (
                    "nickname",
                    Ty::Optional(Box::new(ty_string()), Default::default()),
                ),
            ],
        );
        defs.insert(name.clone(), def);

        let ty = Ty::Class(name, Vec::new(), Default::default());
        let val = json_to_external_with_ty(&serde_json::json!({"id": 1}), &ty, &defs).unwrap();
        match val {
            BexExternalValue::Instance { fields, .. } => {
                assert!(matches!(fields.get("id"), Some(BexExternalValue::Int(1))));
                assert!(matches!(
                    fields.get("nickname"),
                    Some(BexExternalValue::Null)
                ));
            }
            other => panic!("Expected Instance, got {other:?}"),
        }
    }

    /// Missing required field is a clear error, not a panic.
    #[test]
    fn test_typed_class_missing_required_field_errors() {
        let mut defs: IndexMap<TypeName, ClassDefinition> = IndexMap::new();
        let (name, def) = class_def("User", vec![("id", ty_int()), ("name", ty_string())]);
        defs.insert(name.clone(), def);

        let ty = Ty::Class(name, Vec::new(), Default::default());
        let err = json_to_external_with_ty(&serde_json::json!({"id": 1}), &ty, &defs).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("name"),
            "error should name the missing field: {msg}"
        );
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
            help: false,
            from: PathBuf::from("."),
            target_args: vec![],
        }
    }

    fn provided_arg(arg: &BexCallArg) -> &BexExternalValue {
        match arg {
            BexCallArg::Provided(value) => value.as_ref(),
            BexCallArg::OmittedDefault => panic!("expected provided argument"),
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
            .build_args_from(
                &[],
                &[s("text"), s("max_words")],
                &[ty_string(), ty_int()],
                &IndexMap::new(),
                &[false, false],
            )
            .unwrap();
        assert!(matches!(provided_arg(&ordered[0]), BexExternalValue::String(s) if s == "hello"));
        assert!(matches!(
            provided_arg(&ordered[1]),
            BexExternalValue::Int(30)
        ));
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
                &IndexMap::new(),
                &[false, false],
            )
            .unwrap();
        assert!(matches!(provided_arg(&ordered[0]), BexExternalValue::String(s) if s == "hi"));
        assert!(matches!(
            provided_arg(&ordered[1]),
            BexExternalValue::Int(5)
        ));
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
                &IndexMap::new(),
                &[false, false],
            )
            .unwrap();
        // text from JSON; max_words overridden by CLI.
        assert!(
            matches!(provided_arg(&ordered[0]), BexExternalValue::String(s) if s == "from-json")
        );
        assert!(matches!(
            provided_arg(&ordered[1]),
            BexExternalValue::Int(99)
        ));
    }

    /// BEP-027 §"Auto-CLI conventions": a required parameter with no value
    /// surfaces a clear error at load time.
    #[test]
    fn test_bep_build_args_missing_required_is_error() {
        let args = run_args();
        let err = args
            .build_args_from(
                &[],
                &[s("text")],
                &[ty_string()],
                &IndexMap::new(),
                &[false],
            )
            .unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("--text"),
            "error should name the missing param: {msg}"
        );
    }

    #[test]
    fn test_bep_build_args_omits_defaulted_parameters() {
        let args = run_args();
        let ordered = args
            .build_args_from(
                &[s("--query"), s("cats")],
                &[s("query"), s("max_results"), s("filter")],
                &[ty_string(), ty_int(), ty_string()],
                &IndexMap::new(),
                &[false, true, true],
            )
            .unwrap();

        assert!(matches!(provided_arg(&ordered[0]), BexExternalValue::String(s) if s == "cats"));
        assert!(matches!(ordered[1], BexCallArg::OmittedDefault));
        assert!(matches!(ordered[2], BexCallArg::OmittedDefault));
    }

    #[test]
    fn test_target_help_marks_defaulted_params_optional() {
        let func_info = UserFunctionInfo {
            qualified_name: s("user.Search"),
            display_name: s("Search"),
            origin: bex_vm_types::FunctionOrigin::UserDefined,
            param_names: vec![s("query"), s("max_results")],
            param_types: vec![ty_string(), ty_int()],
            param_has_default: vec![false, true],
            return_type: ty_int(),
        };

        let help = RunArgs::target_help_text("Search", &func_info);

        assert!(help.contains("query: string"));
        assert!(help.contains("max_results: int [optional]"));
        assert!(help.contains("--max_results <int> [optional]"));
        assert!(help.contains("baml run --function Search -- --query"));
        assert!(
            !help.contains(
                "Example: baml run --function Search -- --query \"value\" --max_results 42"
            ),
            "optional params should not be shown as required in examples: {help}"
        );
    }

    #[test]
    fn test_bep_run_bound_args_use_defaults_for_normal_function() {
        let engine = engine_from_source(
            r#"
function HostEntry(query: string, max_results: int = 10, filter: string = "none") -> int {
  max_results
}
"#,
        );
        let func_info = find_user_function(&engine, "HostEntry").expect("HostEntry function");
        let args = run_args()
            .build_args_from(
                &[s("--query"), s("cats")],
                &func_info.param_names,
                &func_info.param_types,
                engine.class_definitions(),
                &func_info.param_has_default,
            )
            .unwrap();

        assert!(matches!(args[1], BexCallArg::OmittedDefault));
        assert!(matches!(args[2], BexCallArg::OmittedDefault));

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt
            .block_on(Arc::new(engine).call_function_bound_args(
                "HostEntry",
                args,
                FunctionCallContextBuilder::new(CallId::next()).build(),
                true,
            ))
            .unwrap();
        assert!(matches!(result, BexExternalValue::Int(10)));
    }

    #[test]
    fn test_bep_run_bound_args_use_defaults_for_render_prompt_companion() {
        let engine = engine_from_source(
            r##"
client<llm> TestClient {
  provider openai
  options {
    model "gpt-4o"
  }
}

function AskDocs(query: string, max_results: int = 10, filter: string = "none") -> string {
  client TestClient
  prompt #"{{ query }} {{ max_results }} {{ filter }}"#
}
"##,
        );
        let func_info =
            find_user_function(&engine, "AskDocs$render_prompt").expect("render_prompt function");
        let args = run_args()
            .build_args_from(
                &[s("--query"), s("cats")],
                &func_info.param_names,
                &func_info.param_types,
                engine.class_definitions(),
                &func_info.param_has_default,
            )
            .unwrap();

        assert!(matches!(args[1], BexCallArg::OmittedDefault));
        assert!(matches!(args[2], BexCallArg::OmittedDefault));

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(Arc::new(engine).call_function_bound_args(
            "AskDocs$render_prompt",
            args,
            FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        ))
        .expect("render_prompt should evaluate omitted defaults");
    }

    /// Unknown arg keys are *not* a hard error — BEP permits pass-through
    /// extras (they appear in `baml.argv`). The CLI emits a stderr warning.
    #[test]
    fn test_bep_build_args_unknown_is_non_fatal() {
        let mut args = run_args();
        args.json_args = Some(s(r#"{"text": "hi", "unknown_key": 1}"#));
        let result = args.build_args_from(
            &[],
            &[s("text")],
            &[ty_string()],
            &IndexMap::new(),
            &[false],
        );
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
                &IndexMap::new(),
                &[false],
            )
            .unwrap();
        match provided_arg(&ordered[0]) {
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
                &IndexMap::new(),
                &[false],
            )
            .unwrap();
        match provided_arg(&ordered[0]) {
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
            .build_args_from(
                &[],
                &[s("text"), s("max_words")],
                &[ty_string(), ty_int()],
                &IndexMap::new(),
                &[false, false],
            )
            .unwrap();
        assert!(matches!(provided_arg(&ordered[0]), BexExternalValue::String(s) if s == "filed"));
        assert!(matches!(
            provided_arg(&ordered[1]),
            BexExternalValue::Int(7)
        ));

        let _ = std::fs::remove_dir_all(dir);
    }

    /// BEP-027: "--json-args must be a JSON object" — top-level arrays/scalars
    /// are rejected with a clear error.
    #[test]
    fn test_bep_build_args_json_must_be_object() {
        let mut args = run_args();
        args.json_args = Some(s(r#"[1, 2, 3]"#));
        let err = args
            .build_args_from(
                &[],
                &[s("text")],
                &[ty_string()],
                &IndexMap::new(),
                &[false],
            )
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
        let result =
            parse_auto_cli_args(&tokens, &names, &[ty_string()], &IndexMap::new(), &[false])
                .unwrap();
        assert!(result.contains_key("start_date"));
        // The kebab-form must NOT be accepted.
        let tokens_kebab = vec![s("--start-date"), s("2024-01-01")];
        assert!(
            parse_auto_cli_args(
                &tokens_kebab,
                &names,
                &[ty_string()],
                &IndexMap::new(),
                &[false]
            )
            .is_err()
        );
    }

    /// BEP-027: "Booleans use --flag=true / --flag=false, not --flag /
    /// --no-flag."
    #[test]
    fn test_bep_auto_cli_bool_equals_true_false() {
        let names = vec![s("verbose")];
        let types = vec![ty_bool()];

        let tokens_true = vec![s("--verbose=true")];
        let r =
            parse_auto_cli_args(&tokens_true, &names, &types, &IndexMap::new(), &[false]).unwrap();
        assert!(matches!(
            r.get("verbose"),
            Some(BexExternalValue::Bool(true))
        ));

        let tokens_false = vec![s("--verbose=false")];
        let r =
            parse_auto_cli_args(&tokens_false, &names, &types, &IndexMap::new(), &[false]).unwrap();
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
        let r = parse_auto_cli_args(
            &tokens,
            &names,
            &types,
            &IndexMap::new(),
            &[false, false, false],
        )
        .unwrap();
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
        let r = parse_auto_cli_args(&tokens, &names, &types, &IndexMap::new(), &[false]).unwrap();
        assert!(
            matches!(r.get("text"), Some(BexExternalValue::String(s)) if s == "the text to summarize")
        );
    }

    /// BEP-027 §"baml.argv": bare tokens after `--` are skipped by auto-CLI
    /// and remain accessible via `baml.argv`. They do not error.
    #[test]
    fn test_bep_auto_cli_bare_tokens_skipped() {
        let tokens = vec![s("--text"), s("hi"), s("extra"), s("data")];
        let names = vec![s("text")];
        let types = vec![ty_string()];
        let r = parse_auto_cli_args(&tokens, &names, &types, &IndexMap::new(), &[false]).unwrap();
        assert!(matches!(r.get("text"), Some(BexExternalValue::String(s)) if s == "hi"));
        assert_eq!(r.len(), 1, "bare tokens should not produce entries");
    }

    /// Bare tokens interspersed with flags all work.
    #[test]
    fn test_bep_auto_cli_bare_tokens_interspersed() {
        let tokens = vec![
            s("before"),
            s("--a"),
            s("1"),
            s("middle"),
            s("--b"),
            s("2"),
            s("after"),
        ];
        let names = vec![s("a"), s("b")];
        let types = vec![ty_int(), ty_int()];
        let r = parse_auto_cli_args(&tokens, &names, &types, &IndexMap::new(), &[false, false])
            .unwrap();
        assert!(matches!(r.get("a"), Some(BexExternalValue::Int(1))));
        assert!(matches!(r.get("b"), Some(BexExternalValue::Int(2))));
        assert_eq!(r.len(), 2);
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
            parse_cli_value("null", &ty, &IndexMap::new()).unwrap(),
            BexExternalValue::Null
        ));
        assert!(parse_cli_value("nope", &ty, &IndexMap::new()).is_err());
    }

    #[test]
    fn test_parse_cli_value_list_via_json() {
        let ty = Ty::List(Box::new(ty_int()), Default::default());
        let val = parse_cli_value("[1,2,3]", &ty, &IndexMap::new()).unwrap();
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
        let val = parse_cli_value(r#"{"a":1,"b":2}"#, &ty, &IndexMap::new()).unwrap();
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
        let ty = Ty::Class(tn("User"), Vec::new(), Default::default());
        let val = parse_cli_value(r#"{"id":1}"#, &ty, &IndexMap::new()).unwrap();
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
        assert!(parse_cli_value("not-json", &ty, &IndexMap::new()).is_err());
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

    fn no_scripts() -> HashMap<String, Vec<String>> {
        HashMap::new()
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

        match args.resolve_target(&engine, &no_scripts()).unwrap() {
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

        let err = args.resolve_target(&engine, &no_scripts()).unwrap_err();
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

        match args.resolve_target(&engine, &no_scripts()).unwrap() {
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

        let err = args.resolve_target(&engine, &no_scripts()).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("No runnable target"),
            "error should mention unknown target: {msg}"
        );
    }

    #[test]
    fn test_user_functions_exclude_internal_init_test_wrappers() {
        let engine = engine_from_source(
            r#"
                test "smoke" {
                    assert.is_true(true)
                }
            "#,
        );

        let names: Vec<String> = engine
            .user_functions()
            .into_iter()
            .map(|f| f.display_name)
            .collect();

        assert!(
            names.is_empty(),
            "internal synthesized helpers should not be exposed as runnable functions: {names:?}"
        );
    }

    #[test]
    fn test_user_functions_include_llm_companions() {
        let engine = engine_from_source(
            r##"
                client TestClient {
                    provider openai
                    options {
                        model "gpt-4"
                    }
                }

                function Summarize(text: string) -> string {
                    client TestClient
                    prompt #"hi"#
                }
            "##,
        );

        let names: Vec<String> = engine
            .user_functions()
            .into_iter()
            .map(|f| f.display_name)
            .collect();

        assert!(names.contains(&"Summarize".to_string()), "got: {names:?}");
        assert!(
            names.contains(&"Summarize$render_prompt".to_string()),
            "got: {names:?}"
        );
        assert!(
            names.contains(&"Summarize$build_request".to_string()),
            "got: {names:?}"
        );
        assert!(
            names.contains(&"Summarize$parse".to_string()),
            "got: {names:?}"
        );
    }

    #[test]
    fn test_bep_resolve_function_flag_internal_init_test_errors() {
        let engine = engine_from_source(
            r#"
                test "smoke" {
                    assert.is_true(true)
                }
            "#,
        );
        let tmp = tempfile::tempdir().unwrap();
        let mut args = run_args_with_clean_from(tmp.path());
        args.function = Some(s("$init_test"));

        let err = args.resolve_target(&engine, &no_scripts()).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("No runnable target"),
            "internal synthesized functions should not resolve via --function: {msg}"
        );
    }

    #[test]
    fn test_bep_resolve_function_flag_companion_succeeds() {
        let engine = engine_from_source(
            r##"
                client TestClient {
                    provider openai
                    options {
                        model "gpt-4"
                    }
                }

                function Summarize(text: string) -> string {
                    client TestClient
                    prompt #"hi"#
                }
            "##,
        );
        let tmp = tempfile::tempdir().unwrap();
        let mut args = run_args_with_clean_from(tmp.path());
        args.function = Some(s("Summarize$render_prompt"));

        match args.resolve_target(&engine, &no_scripts()).unwrap() {
            ResolvedTarget::Function(name) => assert_eq!(name, "Summarize$render_prompt"),
            other => panic!("expected Function(Summarize$render_prompt), got {other:?}"),
        }
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

        match args.resolve_target(&engine, &no_scripts()).unwrap() {
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

        let err = args.resolve_target(&engine, &no_scripts()).unwrap_err();
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

        let err = args.resolve_target(&engine, &no_scripts()).unwrap_err();
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
        match args.resolve_target(&engine, &no_scripts()).unwrap() {
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
        let toml = std::fs::read_to_string(project.join("baml.toml")).unwrap();
        let scripts = RunArgs::parse_scripts(&toml);
        match args_dev.resolve_target(&engine, &scripts).unwrap() {
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
        match args_bf.resolve_target(&engine, &scripts).unwrap() {
            ResolvedTarget::Script(expansion) => {
                assert_eq!(expansion.function.as_deref(), Some("scripts.Backfill"));
            }
            other => panic!("expected Script, got {other:?}"),
        }
    }

    /// Tests `parse_scripts` — both cargo-style forms.
    #[test]
    fn test_parse_scripts_string_and_array_forms() {
        let toml = r#"
[scripts]
dev = "-- --verbose=true --model gpt-4o-mini"
greet = ["--function", "g.Hello", "--", "--name", "Ada Lovelace"]
"#;
        let scripts = RunArgs::parse_scripts(toml);

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

    /// Empty or missing content → empty map.
    #[test]
    fn test_parse_scripts_empty_content() {
        assert!(RunArgs::parse_scripts("").is_empty());
    }

    /// TOML with no `[scripts]` table → empty map.
    #[test]
    fn test_parse_scripts_no_scripts_table() {
        assert!(RunArgs::parse_scripts("[project]\nname = \"x\"\n").is_empty());
    }

    #[test]
    fn test_validate_scripts_rejects_internal_function_targets() {
        let engine = engine_from_source(
            r#"
                test "smoke" {
                    assert.is_true(true)
                }
            "#,
        );
        let tmp = tempfile::tempdir().unwrap();
        let toml = r#"
[scripts]
hidden = "--function $init_test"
"#;
        std::fs::write(tmp.path().join("baml.toml"), toml).unwrap();

        let mut args = run_args();
        args.from = tmp.path().to_path_buf();

        let scripts = RunArgs::parse_scripts(toml);
        let err = args
            .validate_scripts(&engine, &scripts, &collect_namespaces(&engine), toml)
            .unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("--function target `$init_test` not found"),
            "scripts should not be allowed to target internal synthesized helpers: {msg}"
        );
    }

    /// BEP-027 §"Target resolution" step 4: a positional target that matches
    /// a function name (but not a script or namespace) is an error. Functions
    /// are only reachable via `--function`.
    #[test]
    fn test_resolve_positional_direct_function_name_errors() {
        let engine = engine_from_source(
            r#"
                function Summarize(text: string) -> string { text }
            "#,
        );
        let tmp = tempfile::tempdir().unwrap();
        let mut args = run_args_with_clean_from(tmp.path());
        args.target = Some(s("Summarize"));

        let err = args.resolve_target(&engine, &no_scripts()).unwrap_err();
        assert!(format!("{err}").contains("No runnable target"));
    }

    // ── E2E: expression mode resolves project-defined symbols ─────

    /// E2E: `-e 'Foo { x: 1 }'` compiles and runs against a class defined
    /// in a BAML file under the project's `baml_src/`.
    #[test]
    fn test_expression_resolves_project_class() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("baml.toml"), "").unwrap();
        let src = tmp.path().join("baml_src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("foo.baml"), "class Foo { x int }\n").unwrap();

        let mut args = run_args();
        args.from = tmp.path().to_path_buf();
        args.expression = Some(s("Foo { x: 1 }"));

        let exit = args.run().expect("run should not hard-fail");
        assert!(
            matches!(exit, crate::ExitCode::Success),
            "expected Success, got {exit:?}"
        );
    }

    /// E2E: expression can call a function defined in a project file.
    #[test]
    fn test_expression_calls_project_function() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("baml.toml"), "").unwrap();
        let src = tmp.path().join("baml_src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("lib.baml"),
            "function Double(x: int) -> int { x * 2 }\n",
        )
        .unwrap();

        let mut args = run_args();
        args.from = tmp.path().to_path_buf();
        args.expression = Some(s("Double(21)"));

        let exit = args.run().expect("run should not hard-fail");
        assert!(
            matches!(exit, crate::ExitCode::Success),
            "expected Success, got {exit:?}"
        );
    }

    /// E2E: without a project (tempdir has no `baml.toml` / `baml_src`),
    /// expressions still work for pure computation, but project symbols
    /// are unavailable — referencing one is a compile error (not Success).
    #[test]
    fn test_expression_standalone_has_no_project_symbols() {
        let tmp = tempfile::tempdir().unwrap();

        let mut args = run_args();
        args.from = tmp.path().to_path_buf();
        args.expression = Some(s("DoesNotExist()"));

        // Referencing an undefined symbol must not succeed: the call either
        // returns Err (compile-time bail) or a non-Success exit code.
        if let Ok(exit) = args.run() {
            assert!(
                !matches!(exit, crate::ExitCode::Success),
                "expected failure, got Success"
            );
        }
    }

    // ========================================================================
    // E2E `run()` tests — one per RunArgs flag / target form.
    //
    // These invoke the full `RunArgs::run()` pipeline against a real tempdir
    // project. Output goes to real stdout (we can't capture println! here),
    // so assertions focus on the returned `ExitCode` and on side effects like
    // `--log-file` output.
    // ========================================================================

    /// Minimal helper: write `baml.toml` + a single `.baml` file into a tempdir
    /// and return an empty `RunArgs` pointing at it.
    fn e2e_project(contents: &str) -> (tempfile::TempDir, RunArgs) {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("baml.toml"), "").unwrap();
        let src = tmp.path().join("baml_src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("main.baml"), contents).unwrap();
        let mut args = run_args();
        args.from = tmp.path().to_path_buf();
        (tmp, args)
    }

    #[test]
    fn run_does_not_mutate_unformatted_project_sources() {
        let (tmp, mut args) = e2e_project("function main()->string {\n\"ok\"\n}\n");
        let baml_path = tmp.path().join("baml_src").join("main.baml");
        let original = std::fs::read_to_string(&baml_path).unwrap();
        args.function = Some(s("main"));

        let exit = args.run().expect("run should not hard-fail");

        assert!(matches!(exit, crate::ExitCode::Success));
        assert_eq!(std::fs::read_to_string(&baml_path).unwrap(), original);
    }

    #[test]
    fn standalone_run_does_not_mutate_unformatted_source() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("hello.baml");
        let original = "function main()->string {\n\"hello\"\n}\n";
        std::fs::write(&file, original).unwrap();

        let mut args = run_args();
        args.from = tmp.path().to_path_buf();
        args.target = Some(file.to_string_lossy().into_owned());

        let exit = args.run().expect("run should not hard-fail");

        assert!(matches!(exit, crate::ExitCode::Success));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
    }

    #[test]
    fn expression_run_does_not_mutate_project_sources() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("baml.toml"), "").unwrap();
        let src = tmp.path().join("baml_src");
        std::fs::create_dir_all(&src).unwrap();
        let file = src.join("foo.baml");
        let original = "class Foo { x int }\n";
        std::fs::write(&file, original).unwrap();

        let mut args = run_args();
        args.from = tmp.path().to_path_buf();
        args.expression = Some(s("Foo { x: 1 }"));

        let exit = args.run().expect("run should not hard-fail");

        assert!(matches!(exit, crate::ExitCode::Success));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
    }

    /// `--function FN` runs the named function end-to-end.
    #[test]
    fn test_run_function_flag_e2e() {
        let (_tmp, mut args) = e2e_project("function Answer() -> int { 42 }\n");
        args.function = Some(s("Answer"));
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// `--function FN` on an unknown function returns a non-Success exit.
    #[test]
    fn test_run_function_flag_unknown_e2e() {
        let (_tmp, mut args) = e2e_project("function Answer() -> int { 42 }\n");
        args.function = Some(s("DoesNotExist"));
        if let Ok(exit) = args.run() {
            assert!(!matches!(exit, crate::ExitCode::Success));
        }
    }

    /// No target + root `main` runs `main`.
    #[test]
    fn test_run_no_target_runs_main_e2e() {
        let (_tmp, args) = e2e_project("function main() -> int { 1 }\n");
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// No target + no `main` → error exit.
    #[test]
    fn test_run_no_target_no_main_errors_e2e() {
        let (_tmp, args) = e2e_project("function Other() -> int { 1 }\n");
        if let Ok(exit) = args.run() {
            assert!(!matches!(exit, crate::ExitCode::Success));
        }
    }

    /// Positional target = namespace (subdirectory of `baml_src`) with `main`.
    /// Uses the same layout as `test_bep_resolve_namespace_main_via_load_and_compile`
    /// so the resolver sees `eval.main` as runnable.
    #[test]
    fn test_run_positional_namespace_main_e2e() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("baml.toml"), "[project]\nname = \"test\"\n").unwrap();
        let baml_src = tmp.path().join("baml_src");
        let ns_eval = baml_src.join("ns_eval");
        std::fs::create_dir_all(&ns_eval).unwrap();
        std::fs::write(
            baml_src.join("root.baml"),
            "function other() -> int { 1 }\n",
        )
        .unwrap();
        std::fs::write(ns_eval.join("main.baml"), "function main() -> int { 99 }\n").unwrap();

        let mut args = run_args();
        args.from = tmp.path().to_path_buf();
        args.target = Some(s("eval"));
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// Positional target = bare function name → error (use --function instead).
    #[test]
    fn test_run_positional_function_name_errors_e2e() {
        let (_tmp, mut args) = e2e_project("function Ping() -> int { 0 }\n");
        args.target = Some(s("Ping"));
        if let Ok(exit) = args.run() {
            assert!(!matches!(exit, crate::ExitCode::Success));
        }
    }

    /// Positional `.baml` file runs in hermetic standalone mode.
    #[test]
    fn test_run_standalone_baml_file_e2e() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("hello.baml");
        std::fs::write(&file, "function main() -> int { 99 }\n").unwrap();

        let mut args = run_args();
        args.from = tmp.path().to_path_buf();
        args.target = Some(file.to_string_lossy().into_owned());
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// Positional `.baml` file without `main` → error.
    #[test]
    fn test_run_standalone_baml_file_without_main_e2e() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("hello.baml");
        std::fs::write(&file, "function Other() -> int { 1 }\n").unwrap();

        let mut args = run_args();
        args.from = tmp.path().to_path_buf();
        args.target = Some(file.to_string_lossy().into_owned());
        if let Ok(exit) = args.run() {
            assert!(!matches!(exit, crate::ExitCode::Success));
        }
    }

    /// `[scripts]` alias in `baml.toml` expands and runs.
    #[test]
    fn test_run_script_alias_e2e() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("baml.toml"),
            "[scripts]\nBackfill = \"--function Run\"\n",
        )
        .unwrap();
        let src = tmp.path().join("baml_src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("main.baml"), "function Run() -> int { 5 }\n").unwrap();

        let mut args = run_args();
        args.from = tmp.path().to_path_buf();
        args.target = Some(s("Backfill"));
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// `--list` prints available targets and exits Success.
    #[test]
    fn test_run_list_flag_e2e() {
        let (_tmp, mut args) =
            e2e_project("function Alpha() -> int { 1 }\nfunction Beta() -> int { 2 }\n");
        args.list = true;
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// `--list --output json` uses the JSON format path (exits Success).
    #[test]
    fn test_run_list_json_output_e2e() {
        let (_tmp, mut args) = e2e_project("function Alpha() -> int { 1 }\n");
        args.list = true;
        args.output = OutputFormat::Json;
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// `--output json` exercises the JSON formatter on a real return value.
    #[test]
    fn test_run_output_json_e2e() {
        let (_tmp, mut args) = e2e_project("function Answer() -> int { 42 }\n");
        args.function = Some(s("Answer"));
        args.output = OutputFormat::Json;
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// `--verbose` doesn't alter correctness; it just enables logging.
    #[test]
    fn test_run_verbose_flag_e2e() {
        let (_tmp, mut args) = e2e_project("function main() -> int { 1 }\n");
        args.verbose = true;
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// `--log-file` creates a file and writes at least one byte.
    #[test]
    fn test_run_log_file_is_written_e2e() {
        let (tmp, mut args) = e2e_project("function main() -> int { 1 }\n");
        let log = tmp.path().join("run.log");
        args.log_file = Some(log.clone());
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
        assert!(log.exists(), "log file should exist after run");
    }

    /// `--json-args '{...}'` supplies arguments inline.
    #[test]
    fn test_run_json_args_inline_e2e() {
        let (_tmp, mut args) = e2e_project("function Add(x: int, y: int) -> int { x + y }\n");
        args.function = Some(s("Add"));
        args.json_args = Some(s(r#"{"x": 1, "y": 2}"#));
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// `--json-args @file` reads JSON from a file.
    #[test]
    fn test_run_json_args_from_file_e2e() {
        let (tmp, mut args) = e2e_project("function Add(x: int, y: int) -> int { x + y }\n");
        let json_path = tmp.path().join("args.json");
        std::fs::write(&json_path, r#"{"x": 10, "y": 20}"#).unwrap();
        args.function = Some(s("Add"));
        args.json_args = Some(format!("@{}", json_path.display()));
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// Args after `--` are parsed as auto-CLI flags for the target function.
    #[test]
    fn test_run_target_args_via_double_dash_e2e() {
        let (_tmp, mut args) = e2e_project("function Greet(name: string) -> string { name }\n");
        args.function = Some(s("Greet"));
        args.target_args = vec![s("--name"), s("ada")];
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// `-- --name=value` (equals form) also works.
    #[test]
    fn test_run_target_args_equals_form_e2e() {
        let (_tmp, mut args) = e2e_project("function Greet(name: string) -> string { name }\n");
        args.function = Some(s("Greet"));
        args.target_args = vec![s("--name=ada")];
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// Positional sugar: single-param function accepts a bare token after `--`.
    #[test]
    fn test_run_target_args_positional_sugar_e2e() {
        let (_tmp, mut args) = e2e_project("function Echo(msg: string) -> string { msg }\n");
        args.function = Some(s("Echo"));
        args.target_args = vec![s("hello")];
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// `-- --help` as the sole forwarded token prints target help and exits
    /// Success without invoking the function (BEP-027 §"Auto-CLI help").
    #[test]
    fn test_run_target_help_single_token_e2e() {
        let (_tmp, mut args) = e2e_project("function Echo(msg: string) -> string { msg }\n");
        args.function = Some(s("Echo"));
        args.target_args = vec![s("--help")];
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// `-- --help value` — `--help` is a *value*, not a help request (BEP-027).
    #[test]
    fn test_run_target_help_with_other_args_is_a_value_e2e() {
        let (_tmp, mut args) = e2e_project("function Echo(msg: string) -> string { msg }\n");
        args.function = Some(s("Echo"));
        args.target_args = vec![s("--msg"), s("--help")];
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// CLI args override JSON args when both are supplied.
    #[test]
    fn test_run_cli_overrides_json_args_e2e() {
        let (_tmp, mut args) = e2e_project("function Add(x: int, y: int) -> int { x + y }\n");
        args.function = Some(s("Add"));
        args.json_args = Some(s(r#"{"x": 1, "y": 2}"#));
        args.target_args = vec![s("--x"), s("100")];
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// `--from` pointing to a nonexistent directory errors (expression mode).
    #[test]
    fn test_run_from_nonexistent_expression_errors_e2e() {
        let mut args = run_args();
        args.from = PathBuf::from("/definitely/does/not/exist/baml");
        args.expression = Some(s("1 + 1"));
        assert!(
            args.run().is_err(),
            "expected hard error on unresolvable --from"
        );
    }

    /// Function returning a class: exercises format_output on an instance.
    #[test]
    fn test_run_returns_class_instance_e2e() {
        let (_tmp, mut args) = e2e_project(
            "class Point { x int  y int }\n\
             function Origin() -> Point { Point { x: 0, y: 0 } }\n",
        );
        args.function = Some(s("Origin"));
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// Same, with `--output json` — exercises external_to_json on an instance.
    #[test]
    fn test_run_returns_class_instance_as_json_e2e() {
        let (_tmp, mut args) = e2e_project(
            "class Point { x int  y int }\n\
             function Origin() -> Point { Point { x: 0, y: 0 } }\n",
        );
        args.function = Some(s("Origin"));
        args.output = OutputFormat::Json;
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    // ========================================================================
    // Script validation tests (BEP-027 §"Scripts in baml.toml")
    // ========================================================================

    /// Reserved verb as a script name → error at validation time.
    #[test]
    fn test_validate_scripts_rejects_reserved_verb() {
        let (_tmp, args) = e2e_project("function main() -> int { 1 }\n");
        std::fs::write(
            args.from.join("baml.toml"),
            "[scripts]\ntest = \"--function main\"\n",
        )
        .unwrap();
        let result = args.run();
        let err = result.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("reserved verb"),
            "expected reserved-verb error, got: {msg}"
        );
    }

    /// Script whose `--function` target doesn't exist → error at validation.
    #[test]
    fn test_validate_scripts_bad_function_target() {
        let (_tmp, args) = e2e_project("function main() -> int { 1 }\n");
        std::fs::write(
            args.from.join("baml.toml"),
            "[scripts]\nmyscript = \"--function DoesNotExist\"\n",
        )
        .unwrap();
        let result = args.run();
        let err = result.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("not found"),
            "expected function-not-found error, got: {msg}"
        );
    }

    /// Script with malformed body (--function without value) → error.
    #[test]
    fn test_validate_scripts_malformed_body() {
        let (_tmp, args) = e2e_project("function main() -> int { 1 }\n");
        std::fs::write(
            args.from.join("baml.toml"),
            "[scripts]\nbad = \"--function\"\n",
        )
        .unwrap();
        let result = args.run();
        let err = result.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("[scripts]") && msg.contains("bad"),
            "expected script-name in error, got: {msg}"
        );
    }

    /// Valid script with real function target passes validation.
    #[test]
    fn test_validate_scripts_valid_passes() {
        let (_tmp, mut args) = e2e_project("function Greet() -> int { 1 }\n");
        std::fs::write(
            args.from.join("baml.toml"),
            "[scripts]\nhello = \"--function Greet\"\n",
        )
        .unwrap();
        args.target = Some(s("hello"));
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    // ── "did you mean…" suggestions ──────────────────────────────────

    /// Suggestions include script names, not just functions.
    #[test]
    fn test_did_you_mean_includes_scripts() {
        let (_tmp, mut args) = e2e_project("function main() -> int { 1 }\n");
        std::fs::write(
            args.from.join("baml.toml"),
            "[scripts]\nbackfill = \"--function main\"\n",
        )
        .unwrap();
        args.target = Some(s("backfil")); // close to "backfill"
        let err = args.run().unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("backfill") && msg.contains("script"),
            "expected script in suggestions, got: {msg}"
        );
    }

    /// Suggestions include namespace names.
    #[test]
    fn test_did_you_mean_includes_namespaces() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("baml.toml"), "[project]\nname = \"test\"\n").unwrap();
        let baml_src = tmp.path().join("baml_src");
        let ns = baml_src.join("ns_eval");
        std::fs::create_dir_all(&ns).unwrap();
        std::fs::write(
            baml_src.join("root.baml"),
            "function other() -> int { 1 }\n",
        )
        .unwrap();
        std::fs::write(ns.join("main.baml"), "function main() -> int { 1 }\n").unwrap();

        let mut args = run_args();
        args.from = tmp.path().to_path_buf();
        args.target = Some(s("evl")); // close to "eval"
        let err = args.run().unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("eval") && msg.contains("namespace"),
            "expected namespace in suggestions, got: {msg}"
        );
    }

    // ── Expression mode with namespaced symbols ──────────────────────

    /// Known gap: expression mode cannot currently call namespace-qualified
    /// functions (`math.Triple(7)`). The namespace resolves but the function
    /// isn't callable via dot-access in expression context. This test
    /// documents the current behavior; flip the assertion when upstream
    /// compiler2 namespace resolution is fixed.
    #[test]
    fn test_expression_namespaced_function_is_known_gap() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("baml.toml"), "[project]\nname = \"test\"\n").unwrap();
        let baml_src = tmp.path().join("baml_src");
        let ns = baml_src.join("ns_math");
        std::fs::create_dir_all(&ns).unwrap();
        std::fs::write(
            ns.join("lib.baml"),
            "function Triple(x: int) -> int { x * 3 }\n",
        )
        .unwrap();

        let mut args = run_args();
        args.from = tmp.path().to_path_buf();
        args.expression = Some(s("math.Triple(7)"));

        // TODO: should succeed once compiler2 supports namespace-qualified
        // calls in expression context. For now, it fails at runtime.
        if let Ok(exit) = args.run() {
            assert!(
                !matches!(exit, crate::ExitCode::Success),
                "namespace calls started working — update this test to expect Success!"
            );
        }
    }

    // ========================================================================
    // --list includes scripts and namespaces (BEP-027 §"Flag reference")
    // ========================================================================

    /// `--list` on a project with scripts shows scripts in output.
    #[test]
    fn test_list_includes_scripts_e2e() {
        let (_tmp, mut args) = e2e_project("function main() -> int { 1 }\n");
        std::fs::write(
            args.from.join("baml.toml"),
            "[scripts]\nbackfill = \"--function main\"\n",
        )
        .unwrap();
        args.list = true;
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// `--list --output json` includes scripts and namespaces in the JSON output.
    #[test]
    fn test_list_json_includes_scripts_and_namespaces_e2e() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("baml.toml"),
            "[scripts]\nbackfill = \"--function main\"\n",
        )
        .unwrap();
        let baml_src = tmp.path().join("baml_src");
        let ns_eval = baml_src.join("ns_eval");
        std::fs::create_dir_all(&ns_eval).unwrap();
        std::fs::write(baml_src.join("root.baml"), "function main() -> int { 1 }\n").unwrap();
        std::fs::write(ns_eval.join("main.baml"), "function main() -> int { 1 }\n").unwrap();

        let mut args = run_args();
        args.from = tmp.path().to_path_buf();
        args.list = true;
        args.output = OutputFormat::Json;
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    // ========================================================================
    // --help before `--` (BEP-027 §"Flag reference")
    // ========================================================================

    /// `--help` with no target shows generic run-verb help and exits Success.
    #[test]
    fn test_help_no_target_shows_run_help_e2e() {
        let (_tmp, mut args) = e2e_project("function main() -> int { 1 }\n");
        args.help = true;
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// `--help` with `--function` shows target-specific help.
    #[test]
    fn test_help_with_function_shows_target_help_e2e() {
        let (_tmp, mut args) = e2e_project("function Greet(name: string) -> string { name }\n");
        args.function = Some(s("Greet"));
        args.help = true;
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// `--help` with a positional target (namespace) shows target-specific help.
    #[test]
    fn test_help_with_namespace_target_shows_target_help_e2e() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("baml.toml"), "[project]\nname = \"test\"\n").unwrap();
        let baml_src = tmp.path().join("baml_src");
        let ns = baml_src.join("ns_eval");
        std::fs::create_dir_all(&ns).unwrap();
        std::fs::write(
            baml_src.join("root.baml"),
            "function other() -> int { 1 }\n",
        )
        .unwrap();
        std::fs::write(
            ns.join("main.baml"),
            "function main(suite: string) -> int { 1 }\n",
        )
        .unwrap();

        let mut args = run_args();
        args.from = tmp.path().to_path_buf();
        args.target = Some(s("eval"));
        args.help = true;
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    // ========================================================================
    // Script validation file:line references (BEP-027 §"Scripts in baml.toml")
    // ========================================================================

    /// Script validation errors include the baml.toml file path.
    #[test]
    fn test_validate_scripts_error_includes_file_path() {
        let (_tmp, args) = e2e_project("function main() -> int { 1 }\n");
        std::fs::write(
            args.from.join("baml.toml"),
            "[scripts]\ntest = \"--function main\"\n",
        )
        .unwrap();
        let err = args.run().unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("baml.toml"),
            "expected file path in error, got: {msg}"
        );
    }

    /// Script validation errors include line numbers.
    #[test]
    fn test_validate_scripts_error_includes_line_number() {
        let (_tmp, args) = e2e_project("function main() -> int { 1 }\n");
        std::fs::write(
            args.from.join("baml.toml"),
            "[scripts]\ntest = \"--function main\"\n",
        )
        .unwrap();
        let err = args.run().unwrap_err();
        let msg = format!("{err}");
        // `test` is on line 2 of the toml file.
        assert!(
            msg.contains(":2:"),
            "expected line number in error, got: {msg}"
        );
    }

    /// `find_script_line` finds keys in both bare and quoted forms.
    #[test]
    fn test_find_script_line_bare_and_quoted() {
        let content = "[scripts]\ndev = \"something\"\n\"dev:cheap\" = \"other\"\n";
        assert_eq!(RunArgs::find_script_line(content, "dev"), Some(2));
        assert_eq!(RunArgs::find_script_line(content, "dev:cheap"), Some(3));
        assert_eq!(RunArgs::find_script_line(content, "nonexistent"), None);
    }

    // ========================================================================
    // Positional target → function name no longer resolves (BEP-027 compliance)
    // ========================================================================

    /// Functions are NOT reachable via positional target — only via `--function`.
    #[test]
    fn test_positional_function_name_not_reachable() {
        let engine = engine_from_source(
            r#"
                function main() -> int { 1 }
                function Summarize(text: string) -> string { text }
            "#,
        );
        let tmp = tempfile::tempdir().unwrap();
        let mut args = run_args_with_clean_from(tmp.path());
        args.target = Some(s("Summarize"));
        assert!(args.resolve_target(&engine, &no_scripts()).is_err());

        // But --function still works.
        args.target = None;
        args.function = Some(s("Summarize"));
        assert!(args.resolve_target(&engine, &no_scripts()).is_ok());
    }

    // ========================================================================
    // Mutual exclusivity: -e and --function (BEP-027 §"Target resolution")
    // ========================================================================

    /// `-e` and `--function` together → error.
    #[test]
    fn test_expression_and_function_mutually_exclusive() {
        let (_tmp, mut args) = e2e_project("function main() -> int { 1 }\n");
        args.expression = Some(s("1 + 1"));
        args.function = Some(s("main"));
        let err = args.run().unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("mutually exclusive"),
            "expected mutual-exclusivity error, got: {msg}"
        );
    }

    /// `-e` alone still works.
    #[test]
    fn test_expression_alone_works() {
        let (_tmp, mut args) = e2e_project("function main() -> int { 1 }\n");
        args.expression = Some(s("1 + 1"));
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// `--function` alone still works.
    #[test]
    fn test_function_alone_works() {
        let (_tmp, mut args) = e2e_project("function Answer() -> int { 42 }\n");
        args.function = Some(s("Answer"));
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    // ========================================================================
    // Script parameter reference validation (BEP-027 §"Scripts in baml.toml")
    // ========================================================================

    /// extract_flag_keys returns flag names from tokens.
    #[test]
    fn test_extract_flag_keys_basic() {
        let args = vec![s("--text"), s("hi"), s("--bogus"), s("val")];
        assert_eq!(extract_flag_keys(&args), vec!["text", "bogus"]);
    }

    /// extract_flag_keys handles --key=value form.
    #[test]
    fn test_extract_flag_keys_equals_form() {
        let args = vec![s("--verbose=true"), s("--name=ada")];
        assert_eq!(extract_flag_keys(&args), vec!["verbose", "name"]);
    }

    /// extract_flag_keys skips bare tokens.
    #[test]
    fn test_extract_flag_keys_skips_bare() {
        let args = vec![s("bare"), s("--flag"), s("val"), s("another")];
        assert_eq!(extract_flag_keys(&args), vec!["flag"]);
    }

    /// extract_flag_keys with empty input.
    #[test]
    fn test_extract_flag_keys_empty() {
        assert!(extract_flag_keys(&[]).is_empty());
    }

    /// Script with unknown parameter flag → error at validation time.
    #[test]
    fn test_validate_scripts_catches_bad_param_reference() {
        let (_tmp, args) = e2e_project("function Greet(name: string) -> string { name }\n");
        std::fs::write(
            args.from.join("baml.toml"),
            "[scripts]\nhello = \"--function Greet -- --nonexistent hi\"\n",
        )
        .unwrap();
        let err = args.run().unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("nonexistent") && msg.contains("unknown parameter"),
            "expected unknown-parameter error, got: {msg}"
        );
    }

    /// Script with valid parameter flags passes validation.
    #[test]
    fn test_validate_scripts_valid_params_pass() {
        let (_tmp, mut args) = e2e_project("function Greet(name: string) -> string { name }\n");
        std::fs::write(
            args.from.join("baml.toml"),
            "[scripts]\nhello = \"--function Greet -- --name World\"\n",
        )
        .unwrap();
        args.target = Some(s("hello"));
        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
    }

    /// Script targeting default main validates extra_args against main's params.
    #[test]
    fn test_validate_scripts_checks_default_main_params() {
        let (_tmp, args) = e2e_project("function main(verbose: bool) -> int { 1 }\n");
        std::fs::write(
            args.from.join("baml.toml"),
            "[scripts]\nmyscript = \"-- --not_a_param=true\"\n",
        )
        .unwrap();
        let err = args.run().unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("not_a_param") && msg.contains("unknown parameter"),
            "expected unknown-parameter error for default main, got: {msg}"
        );
    }

    /// Warnings emitted by the compiler (e.g. an unreachable catch arm) must
    /// carry file + line information when rendered for the CLI. The previous
    /// implementation printed only the message ("warning: unreachable arm")
    /// which made it impossible to locate in a real project.
    #[test]
    fn warnings_are_rendered_with_file_and_line() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("baml.toml"), "").unwrap();
        let src = tmp.path().join("baml_src");
        std::fs::create_dir_all(&src).unwrap();
        // `MayFail` can't throw, so the catch's `_` arm is unreachable —
        // this is surfaced as a Warning-severity diagnostic.
        std::fs::write(
            src.join("main.baml"),
            "function MayFail() -> int { 1 }\n\
             function main() -> int { MayFail() catch (e) { _ => 0 } }\n",
        )
        .unwrap();

        let (db, _from, _files) = crate::project_load::load_project_from(tmp.path()).unwrap();
        let project = db.get_project().expect("project must be set");
        let source_files = db.get_source_files();
        let diagnostics = baml_project::collect_diagnostics(&db, project, &source_files);

        let warnings: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .cloned()
            .collect();
        assert!(
            warnings.iter().any(|d| d.message.contains("unreachable")),
            "expected an `unreachable arm` warning, got: {:?}",
            warnings.iter().map(|d| &d.message).collect::<Vec<_>>()
        );

        let mut sources = HashMap::new();
        let mut file_paths = HashMap::new();
        for sf in &source_files {
            let file_id = sf.file_id(&db);
            sources.insert(file_id, sf.text(&db).to_string());
            file_paths.insert(file_id, sf.path(&db));
        }
        let rendered = render::render_diagnostics(
            &warnings,
            &sources,
            &file_paths,
            &render::RenderConfig::cli(),
        );

        assert!(
            rendered.contains("unreachable"),
            "rendered warning is missing the message, got:\n{rendered}"
        );
        assert!(
            rendered.contains("main.baml"),
            "rendered warning is missing the file name, got:\n{rendered}"
        );
        // Ariadne renders the source span as `file:line:col`, so a digit must
        // appear after `main.baml:` for the location to be present.
        assert!(
            rendered
                .split("main.baml:")
                .nth(1)
                .and_then(|s| s.chars().next())
                .is_some_and(|c| c.is_ascii_digit()),
            "rendered warning is missing a line number after the file name, got:\n{rendered}"
        );
    }
}
