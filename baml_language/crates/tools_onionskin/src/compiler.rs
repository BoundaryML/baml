use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    fmt::Write,
    ops::Deref,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::Result;
use baml_compiler_diagnostics::{Diagnostic, DiagnosticPhase, RenderConfig, render_diagnostic};
use baml_compiler_hir::{ItemId, function_body, function_signature, function_signature_source_map};
use baml_compiler_syntax::{
    SyntaxElement, SyntaxNode, SyntaxToken, WalkEvent,
    ast::{Item as AstItem, SourceFile as AstSourceFile},
};
use baml_compiler_tir::{class_field_types, enum_variants, type_aliases, typing_context};
use baml_db::{
    FileId, SourceFile, baml_compiler_emit, baml_compiler_hir, baml_compiler_lexer,
    baml_compiler_parser, baml_compiler_syntax, baml_compiler_tir, baml_workspace,
};
use baml_project::{ProjectDatabase, collect_diagnostics};
use regex::Regex;
use rowan::{GreenNode, NodeCache, ast::AstNode};
use salsa::{Event, EventKind, Setter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum CompilerPhase {
    Lexer,
    Parser,
    Ast,
    Hir,
    Thir,
    TypedIr,
    Mir,
    Diagnostics,
    Codegen,
    VmRunner,
    Metrics,
}

impl CompilerPhase {
    pub(crate) const ALL: &'static [CompilerPhase] = &[
        CompilerPhase::Lexer,
        CompilerPhase::Parser,
        CompilerPhase::Ast,
        CompilerPhase::Hir,
        CompilerPhase::Thir,
        CompilerPhase::TypedIr,
        CompilerPhase::Mir,
        CompilerPhase::Diagnostics,
        CompilerPhase::Codegen,
        CompilerPhase::VmRunner,
        CompilerPhase::Metrics,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            CompilerPhase::Lexer => "Lexer (Tokens)",
            CompilerPhase::Parser => "Parser (CST)",
            CompilerPhase::Ast => "AST (Typed Nodes)",
            CompilerPhase::Hir => "HIR (High-level IR)",
            CompilerPhase::Thir => "THIR (Typed HIR)",
            CompilerPhase::TypedIr => "TypedIR (Expr-only)",
            CompilerPhase::Mir => "MIR (CFG)",
            CompilerPhase::Diagnostics => "Diagnostics",
            CompilerPhase::Codegen => "Codegen (Bytecode)",
            CompilerPhase::VmRunner => "VM Runner",
            CompilerPhase::Metrics => "Metrics (Incremental)",
        }
    }

    pub(crate) fn next(self) -> CompilerPhase {
        match self {
            CompilerPhase::Lexer => CompilerPhase::Parser,
            CompilerPhase::Parser => CompilerPhase::Ast,
            CompilerPhase::Ast => CompilerPhase::Hir,
            CompilerPhase::Hir => CompilerPhase::Thir,
            CompilerPhase::Thir => CompilerPhase::TypedIr,
            CompilerPhase::TypedIr => CompilerPhase::Mir,
            CompilerPhase::Mir => CompilerPhase::Diagnostics,
            CompilerPhase::Diagnostics => CompilerPhase::Codegen,
            CompilerPhase::Codegen => CompilerPhase::VmRunner,
            CompilerPhase::VmRunner => CompilerPhase::Metrics,
            CompilerPhase::Metrics => CompilerPhase::Lexer,
        }
    }

    pub(crate) fn prev(self) -> CompilerPhase {
        match self {
            CompilerPhase::Lexer => CompilerPhase::Metrics,
            CompilerPhase::Parser => CompilerPhase::Lexer,
            CompilerPhase::Ast => CompilerPhase::Parser,
            CompilerPhase::Hir => CompilerPhase::Ast,
            CompilerPhase::Thir => CompilerPhase::Hir,
            CompilerPhase::TypedIr => CompilerPhase::Thir,
            CompilerPhase::Mir => CompilerPhase::TypedIr,
            CompilerPhase::Diagnostics => CompilerPhase::Mir,
            CompilerPhase::Codegen => CompilerPhase::Diagnostics,
            CompilerPhase::VmRunner => CompilerPhase::Codegen,
            CompilerPhase::Metrics => CompilerPhase::VmRunner,
        }
    }

    /// Returns a short name suitable for CLI arguments
    pub(crate) fn cli_name(self) -> &'static str {
        match self {
            CompilerPhase::Lexer => "lexer",
            CompilerPhase::Parser => "parser",
            CompilerPhase::Ast => "ast",
            CompilerPhase::Hir => "hir",
            CompilerPhase::Thir => "thir",
            CompilerPhase::TypedIr => "typedir",
            CompilerPhase::Mir => "mir",
            CompilerPhase::Diagnostics => "diagnostics",
            CompilerPhase::Codegen => "codegen",
            CompilerPhase::VmRunner => "vmrunner",
            CompilerPhase::Metrics => "metrics",
        }
    }
}

pub(crate) struct CompilerRunner {
    db: ProjectDatabase,
    project_root: baml_workspace::Project,
    is_directory: bool,
    /// Source files currently in the database (path -> `SourceFile`)
    source_files: HashMap<PathBuf, SourceFile>,
    phase_outputs: HashMap<CompilerPhase, String>,
    phase_outputs_annotated: HashMap<CompilerPhase, Vec<(String, LineStatus)>>,
    // Track Salsa events to determine what's recomputed vs cached
    recomputed_queries: Arc<Mutex<HashSet<String>>>,
    cached_queries: Arc<Mutex<HashSet<String>>>,
    // Track which files were modified in the last compilation
    modified_files: HashSet<PathBuf>,
    node_cache: NodeCache,
    parser_cached_elements: HashMap<PathBuf, HashSet<GreenElementId>>,
    // THIR display mode
    thir_display_mode: ThirDisplayMode,
    // THIR interactive state
    thir_interactive_state: ThirInteractiveState,
    // Unified diagnostics collected during compilation
    diagnostics: Vec<Diagnostic>,
    // VM Runner state
    vm_runner_state: VmRunnerState,
}

/// State for the interactive THIR cursor mode
#[derive(Debug, Clone, Default)]
pub(crate) struct ThirInteractiveState {
    /// Current cursor line position (0-indexed)
    pub cursor_line: usize,
    /// Current cursor column position (0-indexed)
    pub cursor_col: usize,
    /// Total number of navigable lines
    pub total_lines: usize,
    /// Map from line index to (function_name, expr_id, type)
    pub line_info: Vec<ThirLineInfo>,
    /// The source text for display
    pub source_lines: Vec<String>,
}

/// State for the VM Runner tab
#[derive(Debug, Clone, Default)]
pub(crate) struct VmRunnerState {
    /// Available function names (sorted alphabetically)
    pub available_functions: Vec<String>,
    /// Currently selected function index
    pub selected_function: usize,
    /// Result of the last execution
    pub execution_result: Option<VmExecutionResult>,
}

/// Result of a VM execution
#[derive(Debug, Clone)]
pub(crate) enum VmExecutionResult {
    /// Execution completed successfully
    Success(String),
    /// Execution failed with an error
    Error(String),
    /// Function requires arguments (we can't run it without args)
    RequiresArgs { arity: usize },
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct ThirLineInfo {
    pub function_name: String,
    pub expr_type: Option<String>,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineStatus {
    Recomputed,
    Cached,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VisualizationMode {
    /// Show which files changed (diff-based coloring)
    Diff,
    /// Show which incremental queries were recomputed vs cached
    Incremental,
}

/// Display mode for the THIR tab
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ThirDisplayMode {
    /// Show the tree view (default)
    #[default]
    Tree,
    /// Interactive mode with cursor navigation
    Interactive,
}

impl ThirDisplayMode {
    /// Get the display name for this mode
    pub fn name(&self) -> &'static str {
        match self {
            ThirDisplayMode::Tree => "Tree",
            ThirDisplayMode::Interactive => "Interactive",
        }
    }
}

impl CompilerRunner {
    pub(crate) fn new(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        let is_directory = path.is_dir();

        // Create event tracking
        let recomputed_queries = Arc::new(Mutex::new(HashSet::new()));
        let cached_queries = Arc::new(Mutex::new(HashSet::new()));

        let recomputed_clone = recomputed_queries.clone();
        let cached_clone = cached_queries.clone();

        // Create database with event callback
        let db =
            ProjectDatabase::new_with_event_callback(Box::new(move |event: Event| {
                match event.kind {
                    EventKind::WillExecute { database_key } => {
                        recomputed_clone
                            .lock()
                            .unwrap()
                            .insert(format!("{database_key:?}"));
                    }
                    EventKind::DidValidateMemoizedValue { database_key } => {
                        cached_clone
                            .lock()
                            .unwrap()
                            .insert(format!("{database_key:?}"));
                    }
                    _ => {}
                }
            }));

        Self {
            project_root: baml_workspace::Project::new(&db, PathBuf::new(), vec![]),
            db,
            is_directory,
            source_files: HashMap::new(),
            phase_outputs: HashMap::new(),
            phase_outputs_annotated: HashMap::new(),
            recomputed_queries,
            cached_queries,
            modified_files: HashSet::new(),
            node_cache: NodeCache::default(),
            parser_cached_elements: HashMap::new(),
            thir_display_mode: ThirDisplayMode::default(),
            thir_interactive_state: ThirInteractiveState::default(),
            diagnostics: Vec::new(),
            vm_runner_state: VmRunnerState::default(),
        }
    }

    /// Compile files from a "fake filesystem" (`HashMap` of path -> content)
    /// If `snapshot_files` is provided, we:
    ///   1. Add snapshot files to DB first
    ///   2. Use .`set_text()` to update to `current_files`
    ///
    /// This allows Salsa to see what changed vs what's cached
    pub(crate) fn compile_from_filesystem(
        &mut self,
        current_files: &HashMap<PathBuf, String>,
        snapshot_files: Option<&HashMap<PathBuf, String>>,
    ) {
        // Clear event tracking
        self.recomputed_queries.lock().unwrap().clear();
        self.cached_queries.lock().unwrap().clear();

        // Create new database with event callback
        let recomputed_clone = self.recomputed_queries.clone();
        let cached_clone = self.cached_queries.clone();

        self.db =
            ProjectDatabase::new_with_event_callback(Box::new(move |event: Event| {
                match event.kind {
                    EventKind::WillExecute { database_key } => {
                        recomputed_clone
                            .lock()
                            .unwrap()
                            .insert(format!("{database_key:?}"));
                    }
                    EventKind::DidValidateMemoizedValue { database_key } => {
                        cached_clone
                            .lock()
                            .unwrap()
                            .insert(format!("{database_key:?}"));
                    }
                    _ => {}
                }
            }));

        // Set project root
        let project_path = if self.is_directory {
            current_files
                .keys()
                .next()
                .and_then(|p| p.parent())
                .unwrap_or_else(|| Path::new("."))
        } else {
            current_files
                .keys()
                .next()
                .and_then(|p| p.parent())
                .unwrap_or_else(|| Path::new("."))
        };
        self.project_root = self.db.set_project_root(project_path);

        // Clear the source files list and modified tracking
        self.source_files.clear();
        self.modified_files.clear();
        self.parser_cached_elements
            .retain(|path, _| current_files.contains_key(path));

        // If snapshot_files provided, use the "fake filesystem" approach
        if let Some(snapshot) = snapshot_files {
            // Step 1: Add snapshot files to DB
            let mut source_file_map = HashMap::new();
            for (path, content) in snapshot {
                let path_str = path.to_string_lossy().to_string();
                let source_file = self.db.add_file(&path_str, content);
                source_file_map.insert(path.clone(), source_file);
            }

            // Step 2: Use .set_text() to update to current files
            for (path, current_content) in current_files {
                if let Some(source_file) = source_file_map.get(path) {
                    // File exists in snapshot, check if it changed
                    let snapshot_content = snapshot.get(path).unwrap();
                    if snapshot_content != current_content {
                        // File changed - update it
                        source_file
                            .set_text(&mut self.db)
                            .to(current_content.clone());
                        self.modified_files.insert(path.clone());
                    }
                    self.source_files.insert(path.clone(), *source_file);
                } else {
                    // New file not in snapshot, add it
                    let path_str = path.to_string_lossy().to_string();
                    let source_file = self.db.add_file(&path_str, current_content);
                    self.source_files.insert(path.clone(), source_file);
                    self.modified_files.insert(path.clone());
                }
            }
        } else {
            // No snapshot, just add current files (all are "new")
            for (path, content) in current_files {
                let path_str = path.to_string_lossy().to_string();
                let source_file = self.db.add_file(&path_str, content);
                self.source_files.insert(path.clone(), source_file);
                self.modified_files.insert(path.clone());
            }
        }

        // Update project root with the list of files for proper Salsa tracking
        let file_list: Vec<_> = self.source_files.values().copied().collect();
        self.project_root.set_files(&mut self.db).to(file_list);

        // Run all compiler phases
        self.run_all_phases();
    }

    fn run_all_phases(&mut self) {
        self.phase_outputs.clear();
        self.phase_outputs_annotated.clear();
        self.diagnostics.clear();

        // Run frontend phases that don't panic
        for &phase in &[
            CompilerPhase::Lexer,
            CompilerPhase::Parser,
            CompilerPhase::Ast,
            CompilerPhase::Hir,
            CompilerPhase::Thir,
            CompilerPhase::TypedIr,
        ] {
            self.run_single_phase(phase);
        }

        // Collect diagnostics early to determine if we have errors
        self.run_single_phase(CompilerPhase::Diagnostics);

        // Only run MIR, Codegen, and VmRunner if there are no errors
        // These phases may panic on invalid input, so we skip them when errors exist
        if self.diagnostics.is_empty() {
            for &phase in &[
                CompilerPhase::Mir,
                CompilerPhase::Codegen,
                CompilerPhase::VmRunner,
            ] {
                self.run_single_phase(phase);
            }
        } else {
            // Insert placeholder outputs for skipped phases
            let skip_message = "(skipped due to errors)".to_string();
            for &phase in &[
                CompilerPhase::Mir,
                CompilerPhase::Codegen,
                CompilerPhase::VmRunner,
            ] {
                self.phase_outputs.insert(phase, skip_message.clone());
                self.phase_outputs_annotated
                    .insert(phase, vec![(skip_message.clone(), LineStatus::Unknown)]);
            }
        }

        self.run_single_phase(CompilerPhase::Metrics);
    }

    pub(crate) fn run_single_phase(&mut self, phase: CompilerPhase) {
        match phase {
            CompilerPhase::Lexer => self.run_lexer(),
            CompilerPhase::Parser => self.run_parser(),
            CompilerPhase::Ast => self.run_ast(),
            CompilerPhase::Hir => self.run_hir(),
            CompilerPhase::Thir => self.run_thir(),
            CompilerPhase::TypedIr => self.run_typed_ir(),
            CompilerPhase::Mir => self.run_mir(),
            CompilerPhase::Diagnostics => self.run_diagnostics(),
            CompilerPhase::Codegen => self.run_codegen(),
            CompilerPhase::VmRunner => self.run_vm_runner(),
            CompilerPhase::Metrics => self.run_metrics(),
        }
    }

    fn run_lexer(&mut self) {
        let mut output = String::new();
        let mut output_annotated = Vec::new();

        // Sort files alphabetically by path
        let mut sorted_files: Vec<_> = self.source_files.iter().collect();
        sorted_files.sort_by_key(|(path, _)| path.as_path());

        for (path, source_file) in sorted_files {
            let file_path = path.display().to_string();
            // Check if THIS specific file was modified
            let file_recomputed = self.modified_files.contains(path);

            writeln!(output, "File: {file_path}").ok();
            output_annotated.push((
                format!("File: {file_path}"),
                if file_recomputed {
                    LineStatus::Recomputed
                } else {
                    LineStatus::Unknown
                },
            ));

            let tokens = baml_compiler_lexer::lex_file(&self.db, *source_file);
            for token in tokens {
                let line = format!("{:?} {:?}", token.kind, token.text);
                writeln!(output, "{line}").ok();
                output_annotated.push((
                    line,
                    if file_recomputed {
                        LineStatus::Recomputed
                    } else {
                        LineStatus::Cached
                    },
                ));
            }
            writeln!(output).ok();
            output_annotated.push((String::new(), LineStatus::Unknown));
        }

        self.phase_outputs.insert(CompilerPhase::Lexer, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Lexer, output_annotated);
    }

    fn run_parser(&mut self) {
        let mut output = String::new();
        let mut output_annotated = Vec::new();
        let mut next_cached_elements: HashMap<PathBuf, HashSet<GreenElementId>> = HashMap::new();

        // Sort files alphabetically by path
        let mut sorted_files: Vec<_> = self.source_files.iter().collect();
        sorted_files.sort_by_key(|(path, _)| path.as_path());

        for (path, source_file) in sorted_files {
            let file_path = path.display().to_string();
            let file_recomputed = self.modified_files.contains(path);

            writeln!(output, "File: {file_path}").ok();
            output_annotated.push((
                format!("File: {file_path}"),
                if file_recomputed {
                    LineStatus::Recomputed
                } else {
                    LineStatus::Unknown
                },
            ));

            let tokens = baml_compiler_lexer::lex_file(&self.db, *source_file);
            let (green, _errors) =
                baml_compiler_parser::parse_file_with_cache(&tokens, &mut self.node_cache);
            let syntax_tree = baml_compiler_syntax::SyntaxNode::new_root(green.clone());

            // Note: Diagnostic collection moved to run_diagnostics() using collect_diagnostics()

            let (formatted_lines, cached_ids) =
                format_syntax_tree_with_cache(&syntax_tree, self.parser_cached_elements.get(path));
            next_cached_elements.insert(path.clone(), cached_ids);

            for (line, status) in formatted_lines {
                writeln!(output, "{line}").ok();
                output_annotated.push((line, status));
            }

            writeln!(output).ok();
            output_annotated.push((String::new(), LineStatus::Unknown));
        }

        self.parser_cached_elements = next_cached_elements;

        self.phase_outputs.insert(CompilerPhase::Parser, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Parser, output_annotated);
    }

    fn run_ast(&mut self) {
        let mut output = String::new();
        let mut output_annotated = Vec::new();

        // Sort files alphabetically by path
        let mut sorted_files: Vec<_> = self.source_files.iter().collect();
        sorted_files.sort_by_key(|(path, _)| path.as_path());

        for (path, source_file) in sorted_files {
            let file_path = path.display().to_string();
            let file_recomputed = self.modified_files.contains(path);

            writeln!(output, "File: {file_path}").ok();
            output_annotated.push((
                format!("File: {file_path}"),
                if file_recomputed {
                    LineStatus::Recomputed
                } else {
                    LineStatus::Unknown
                },
            ));

            // Parse and get CST root
            let tokens = baml_compiler_lexer::lex_file(&self.db, *source_file);
            let (green, _errors) =
                baml_compiler_parser::parse_file_with_cache(&tokens, &mut self.node_cache);
            let syntax_tree = baml_compiler_syntax::SyntaxNode::new_root(green.clone());

            // Cast to AST SourceFile to access typed API
            if let Some(ast_file) = AstSourceFile::cast(syntax_tree) {
                // Iterate over top-level items
                for item in ast_file.items() {
                    let ast_repr = format_ast_item(&item);
                    writeln!(output, "{ast_repr}").ok();
                    output_annotated.push((
                        ast_repr,
                        if file_recomputed {
                            LineStatus::Recomputed
                        } else {
                            LineStatus::Cached
                        },
                    ));
                }
            } else {
                let no_items = "  (unable to parse as AST SourceFile)".to_string();
                writeln!(output, "{no_items}").ok();
                output_annotated.push((
                    no_items,
                    if file_recomputed {
                        LineStatus::Recomputed
                    } else {
                        LineStatus::Cached
                    },
                ));
            }

            writeln!(output).ok();
            output_annotated.push((String::new(), LineStatus::Unknown));
        }

        self.phase_outputs.insert(CompilerPhase::Ast, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Ast, output_annotated);
    }

    fn run_hir(&mut self) {
        let mut output = String::new();
        let mut output_annotated = Vec::new();

        // Sort files alphabetically
        let mut sorted_files: Vec<_> = self.source_files.iter().collect();
        sorted_files.sort_by_key(|(path, _)| path.as_path());

        for (path, source_file) in sorted_files {
            let file_path = path.display().to_string();

            // Use real baml_compiler_hir for item extraction
            let item_tree = baml_compiler_hir::file_item_tree(&self.db, *source_file);
            let items_struct = baml_compiler_hir::file_items(&self.db, *source_file);
            let items = items_struct.items(&self.db);

            // Check if THIS specific file was modified
            let file_recomputed = self.modified_files.contains(path);
            let status = if file_recomputed {
                LineStatus::Recomputed
            } else {
                LineStatus::Cached
            };

            writeln!(output, "File: {file_path}").ok();
            output_annotated.push((format!("File: {file_path}"), LineStatus::Unknown));

            // Show real HIR items with pretty printing
            if items.is_empty() {
                let no_items = "  (no items)".to_string();
                writeln!(output, "{no_items}").ok();
                output_annotated.push((no_items, status));
            } else {
                for item in items {
                    match item {
                        ItemId::Function(func_loc) => {
                            let func = &item_tree[func_loc.id(&self.db)];
                            let signature = function_signature(&self.db, *func_loc);
                            let body = function_body(&self.db, *func_loc);

                            // Build function header
                            let params_str: Vec<String> = signature
                                .params
                                .iter()
                                .map(|p| {
                                    format!(
                                        "{}: {}",
                                        p.name,
                                        baml_compiler_hir::pretty::type_ref_to_str(&p.type_ref)
                                    )
                                })
                                .collect();
                            let return_str =
                                baml_compiler_hir::pretty::type_ref_to_str(&signature.return_type);

                            // Print body based on type
                            match &*body {
                                baml_compiler_hir::FunctionBody::Expr(expr_body, _source_map) => {
                                    let body_code = baml_compiler_hir::body_to_code(expr_body);
                                    // Combine header with body, putting { on same line
                                    let header = format!(
                                        "function {}({}) -> {} {{",
                                        func.name,
                                        params_str.join(", "),
                                        return_str
                                    );
                                    writeln!(output, "{header}").ok();
                                    output_annotated.push((header, status));

                                    // Skip the opening brace line from body_code and print rest
                                    let body_lines: Vec<&str> = body_code.lines().collect();
                                    // body_code starts with "{", so skip first line and last "}"
                                    for line in body_lines
                                        .iter()
                                        .skip(1)
                                        .take(body_lines.len().saturating_sub(2))
                                    {
                                        writeln!(output, "{line}").ok();
                                        output_annotated.push((line.to_string(), status));
                                    }
                                    let closing = "}".to_string();
                                    writeln!(output, "{closing}").ok();
                                    output_annotated.push((closing, status));
                                }
                                baml_compiler_hir::FunctionBody::Llm(_) => {
                                    let header = format!(
                                        "function {}({}) -> {}",
                                        func.name,
                                        params_str.join(", "),
                                        return_str
                                    );
                                    writeln!(output, "{header}").ok();
                                    output_annotated.push((header, status));
                                    let line = "  <LLM function>".to_string();
                                    writeln!(output, "{line}").ok();
                                    output_annotated.push((line, status));
                                }
                                baml_compiler_hir::FunctionBody::Missing => {
                                    let header = format!(
                                        "function {}({}) -> {}",
                                        func.name,
                                        params_str.join(", "),
                                        return_str
                                    );
                                    writeln!(output, "{header}").ok();
                                    output_annotated.push((header, status));
                                    let line = "  <missing body>".to_string();
                                    writeln!(output, "{line}").ok();
                                    output_annotated.push((line, status));
                                }
                            }
                            writeln!(output).ok();
                            output_annotated.push((String::new(), LineStatus::Unknown));
                        }
                        ItemId::Class(class_loc) => {
                            let class = &item_tree[class_loc.id(&self.db)];
                            let header = format!("class {}", class.name);
                            writeln!(output, "{header}").ok();
                            output_annotated.push((header, status));

                            for field in &class.fields {
                                let field_str = format!(
                                    "  {}: {}",
                                    field.name,
                                    baml_compiler_hir::pretty::type_ref_to_str(&field.type_ref)
                                );
                                writeln!(output, "{field_str}").ok();
                                output_annotated.push((field_str, status));
                            }
                            writeln!(output).ok();
                            output_annotated.push((String::new(), LineStatus::Unknown));
                        }
                        ItemId::Enum(enum_loc) => {
                            let enum_def = &item_tree[enum_loc.id(&self.db)];
                            let header = format!("enum {}", enum_def.name);
                            writeln!(output, "{header}").ok();
                            output_annotated.push((header, status));

                            for variant in &enum_def.variants {
                                let variant_str = format!("  {}", variant.name);
                                writeln!(output, "{variant_str}").ok();
                                output_annotated.push((variant_str, status));
                            }
                            writeln!(output).ok();
                            output_annotated.push((String::new(), LineStatus::Unknown));
                        }
                        ItemId::TypeAlias(alias_loc) => {
                            let alias = &item_tree[alias_loc.id(&self.db)];
                            let line = format!(
                                "type {} = {}",
                                alias.name,
                                baml_compiler_hir::pretty::type_ref_to_str(&alias.type_ref)
                            );
                            writeln!(output, "{line}").ok();
                            output_annotated.push((line, status));
                            writeln!(output).ok();
                            output_annotated.push((String::new(), LineStatus::Unknown));
                        }
                        ItemId::Client(client_loc) => {
                            let client = &item_tree[client_loc.id(&self.db)];
                            let line =
                                format!("client {} (provider: {})", client.name, client.provider);
                            writeln!(output, "{line}").ok();
                            output_annotated.push((line, status));
                            writeln!(output).ok();
                            output_annotated.push((String::new(), LineStatus::Unknown));
                        }
                        ItemId::Test(test_loc) => {
                            let test = &item_tree[test_loc.id(&self.db)];
                            let line = format!("test {}", test.name);
                            writeln!(output, "{line}").ok();
                            output_annotated.push((line, status));
                            writeln!(output).ok();
                            output_annotated.push((String::new(), LineStatus::Unknown));
                        }
                        ItemId::Generator(gen_loc) => {
                            let generator = &item_tree[gen_loc.id(&self.db)];
                            let output_type =
                                generator.output_type.as_deref().unwrap_or("<missing>");
                            let line = format!(
                                "generator {} (output_type: {})",
                                generator.name, output_type
                            );
                            writeln!(output, "{line}").ok();
                            output_annotated.push((line, status));
                            writeln!(output).ok();
                            output_annotated.push((String::new(), LineStatus::Unknown));
                        }
                    }
                }
            }

            writeln!(output).ok();
            output_annotated.push((String::new(), LineStatus::Unknown));
        }

        self.phase_outputs.insert(CompilerPhase::Hir, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Hir, output_annotated);
    }

    fn run_thir(&mut self) {
        let mut output = String::new();
        let mut output_annotated = Vec::new();
        let mut interactive_state = ThirInteractiveState::default();

        // Build initial typing context with all function types
        let globals = typing_context(&self.db, self.project_root)
            .functions(&self.db)
            .clone();
        let class_fields = class_field_types(&self.db, self.project_root)
            .classes(&self.db)
            .clone();
        let type_aliases_map = type_aliases(&self.db, self.project_root)
            .aliases(&self.db)
            .clone();
        let enum_variants_map = enum_variants(&self.db, self.project_root);
        let enum_variants_data = enum_variants_map.enums(&self.db).clone();

        let resolution_ctx =
            baml_compiler_tir::TypeResolutionContext::new(&self.db, self.project_root);

        // Sort files alphabetically
        let mut sorted_files: Vec<_> = self.source_files.iter().collect();
        sorted_files.sort_by_key(|(path, _)| path.as_path());

        for (path, source_file) in sorted_files {
            let file_path = path.display().to_string();
            let file_recomputed = self.modified_files.contains(path);

            writeln!(output, "File: {file_path}").ok();
            output_annotated.push((format!("File: {file_path}"), LineStatus::Unknown));
            interactive_state
                .source_lines
                .push(format!("File: {file_path}"));
            interactive_state.line_info.push(ThirLineInfo {
                function_name: String::new(),
                expr_type: None,
                description: "File header".to_string(),
            });

            // Get HIR items for this file
            let items_struct = baml_compiler_hir::file_items(&self.db, *source_file);
            let items = items_struct.items(&self.db);

            for item in items {
                if let ItemId::Function(func_id) = item {
                    let signature = function_signature(&self.db, *func_id);
                    let sig_source_map = function_signature_source_map(&self.db, *func_id);
                    let func_name = signature.name.to_string();
                    let body = function_body(&self.db, *func_id);

                    // Run type inference with global function types and type validation
                    let inference_result = baml_compiler_tir::infer_function(
                        &self.db,
                        &signature,
                        Some(&sig_source_map),
                        &body,
                        Some(globals.clone()),
                        Some(class_fields.clone()),
                        Some(type_aliases_map.clone()),
                        Some(enum_variants_data.clone()),
                        *func_id,
                    );

                    // Note: Type error collection moved to run_diagnostics() using collect_diagnostics()

                    // Use tree view for both modes - interactive mode parses this afterward
                    let tree_output = baml_compiler_tir::render_function_tree(
                        &self.db,
                        &resolution_ctx,
                        &func_name,
                        &signature,
                        &body,
                        &inference_result,
                    );

                    let status = if file_recomputed {
                        LineStatus::Recomputed
                    } else {
                        LineStatus::Cached
                    };

                    for line in tree_output.lines() {
                        writeln!(output, "{}", line).ok();
                        output_annotated.push((line.to_string(), status));
                    }
                    writeln!(output).ok();
                    output_annotated.push((String::new(), LineStatus::Unknown));
                }
            }

            writeln!(output).ok();
            output_annotated.push((String::new(), LineStatus::Unknown));
            interactive_state.source_lines.push(String::new());
            interactive_state.line_info.push(ThirLineInfo {
                function_name: String::new(),
                expr_type: None,
                description: String::new(),
            });
        }

        interactive_state.total_lines = interactive_state.line_info.len();
        self.thir_interactive_state = interactive_state;

        self.phase_outputs.insert(CompilerPhase::Thir, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Thir, output_annotated);
    }

    fn run_typed_ir(&mut self) {
        use baml_compiler_vir::{lower_from_hir, pretty_print};

        let mut output = String::new();
        let mut output_annotated = Vec::new();

        // Build typing context and class fields for inference
        let globals = typing_context(&self.db, self.project_root)
            .functions(&self.db)
            .clone();
        let class_fields = class_field_types(&self.db, self.project_root)
            .classes(&self.db)
            .clone();
        let type_aliases_map = type_aliases(&self.db, self.project_root)
            .aliases(&self.db)
            .clone();
        let enum_variants_map = enum_variants(&self.db, self.project_root);
        let enum_variants_data = enum_variants_map.enums(&self.db).clone();

        let resolution_ctx =
            baml_compiler_tir::TypeResolutionContext::new(&self.db, self.project_root);

        // Sort files alphabetically
        let mut sorted_files: Vec<_> = self.source_files.iter().collect();
        sorted_files.sort_by_key(|(path, _)| path.as_path());

        for (path, source_file) in sorted_files {
            let file_path = path.display().to_string();
            let file_recomputed = self.modified_files.contains(path);

            writeln!(output, "File: {file_path}").ok();
            output_annotated.push((format!("File: {file_path}"), LineStatus::Unknown));

            // Get HIR items for this file
            let items_struct = baml_compiler_hir::file_items(&self.db, *source_file);
            let items = items_struct.items(&self.db);

            for item in items {
                if let ItemId::Function(func_id) = item {
                    let signature = function_signature(&self.db, *func_id);
                    let sig_source_map = function_signature_source_map(&self.db, *func_id);
                    let func_name = signature.name.to_string();
                    let body = function_body(&self.db, *func_id);

                    // Skip non-expression bodies
                    let baml_compiler_hir::FunctionBody::Expr(_, _) = &*body else {
                        continue;
                    };

                    // Run type inference
                    let inference_result = baml_compiler_tir::infer_function(
                        &self.db,
                        &signature,
                        Some(&sig_source_map),
                        &body,
                        Some(globals.clone()),
                        Some(class_fields.clone()),
                        Some(type_aliases_map.clone()),
                        Some(enum_variants_data.clone()),
                        *func_id,
                    );

                    // Try to lower to TypedIR
                    let status = if file_recomputed {
                        LineStatus::Recomputed
                    } else {
                        LineStatus::Cached
                    };

                    let header = format!("=== Function: {} ===", func_name);
                    writeln!(output, "{}", header).ok();
                    output_annotated.push((header, status));

                    match lower_from_hir(&body, &inference_result, &resolution_ctx) {
                        Ok(typed_ir) => {
                            // Pretty print the TypedIR
                            let ir_output = pretty_print(&typed_ir);
                            for line in ir_output.lines() {
                                writeln!(output, "{}", line).ok();
                                output_annotated.push((line.to_string(), status));
                            }
                        }
                        Err(e) => {
                            // Show error if lowering failed
                            let error_line = format!("  <lowering failed: {}>", e);
                            writeln!(output, "{}", error_line).ok();
                            output_annotated.push((error_line, LineStatus::Recomputed));
                        }
                    }

                    writeln!(output).ok();
                    output_annotated.push((String::new(), LineStatus::Unknown));
                }
            }

            writeln!(output).ok();
            output_annotated.push((String::new(), LineStatus::Unknown));
        }

        self.phase_outputs.insert(CompilerPhase::TypedIr, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::TypedIr, output_annotated);
    }

    fn run_mir(&mut self) {
        let mut output = String::new();
        let mut output_annotated = Vec::new();

        // Build typing context and class fields map for MIR lowering
        let file_list: Vec<_> = self.source_files.values().copied().collect();
        let globals = typing_context(&self.db, self.project_root)
            .functions(&self.db)
            .clone();
        let class_field_types_map = class_field_types(&self.db, self.project_root)
            .classes(&self.db)
            .clone();

        // Build classes map (class name -> field name -> field index) for MIR lowering
        let mut classes: HashMap<String, HashMap<String, usize>> = HashMap::new();
        for file in &file_list {
            let item_tree = baml_compiler_hir::file_item_tree(&self.db, *file);
            let items_struct = baml_compiler_hir::file_items(&self.db, *file);
            for item in items_struct.items(&self.db) {
                if let ItemId::Class(class_loc) = item {
                    let class = &item_tree[class_loc.id(&self.db)];
                    let class_name = class.name.to_string();

                    let mut field_indices = HashMap::new();
                    for (idx, field) in class.fields.iter().enumerate() {
                        field_indices.insert(field.name.to_string(), idx);
                    }
                    classes.insert(class_name, field_indices);
                }
            }
        }

        let resolution_ctx =
            baml_compiler_tir::TypeResolutionContext::new(&self.db, self.project_root);

        // Sort files alphabetically
        let mut sorted_files: Vec<_> = self.source_files.iter().collect();
        sorted_files.sort_by_key(|(path, _)| path.as_path());

        for (path, source_file) in sorted_files {
            let file_path = path.display().to_string();
            let file_recomputed = self.modified_files.contains(path);

            writeln!(output, "File: {file_path}").ok();
            output_annotated.push((format!("File: {file_path}"), LineStatus::Unknown));

            // Get HIR items for this file
            let items_struct = baml_compiler_hir::file_items(&self.db, *source_file);
            let items = items_struct.items(&self.db);

            for item in items {
                if let ItemId::Function(func_id) = item {
                    let signature = function_signature(&self.db, *func_id);
                    let sig_source_map = function_signature_source_map(&self.db, *func_id);
                    let func_name = signature.name.to_string();
                    let body = function_body(&self.db, *func_id);

                    // Run type inference with global function types
                    let inference_result = baml_compiler_tir::infer_function(
                        &self.db,
                        &signature,
                        Some(&sig_source_map),
                        &body,
                        Some(globals.clone()),
                        Some(class_field_types_map.clone()),
                        None, // type_aliases
                        None, // enum_variants
                        *func_id,
                    );

                    // Lower HIR → VIR → MIR
                    let mir_output = match baml_compiler_vir::lower_from_hir(
                        &body,
                        &inference_result,
                        &resolution_ctx,
                    ) {
                        Ok(vir) => {
                            let mir = baml_compiler_mir::lower(
                                &signature,
                                &vir,
                                &self.db,
                                &classes,
                                &resolution_ctx,
                            );
                            baml_compiler_mir::pretty::display_function(&mir)
                        }
                        Err(baml_compiler_vir::LoweringError::LlmFunction) => {
                            "(LLM function - no MIR)".to_string()
                        }
                        Err(err) => {
                            format!("(no MIR due to errors: {})", err)
                        }
                    };

                    let status = if file_recomputed {
                        LineStatus::Recomputed
                    } else {
                        LineStatus::Cached
                    };

                    // Add function header
                    writeln!(output, "=== Function: {} ===", func_name).ok();
                    output_annotated.push((format!("=== Function: {} ===", func_name), status));

                    for line in mir_output.lines() {
                        writeln!(output, "{}", line).ok();
                        output_annotated.push((line.to_string(), status));
                    }
                    writeln!(output).ok();
                    output_annotated.push((String::new(), LineStatus::Unknown));
                }
            }

            writeln!(output).ok();
            output_annotated.push((String::new(), LineStatus::Unknown));
        }

        self.phase_outputs.insert(CompilerPhase::Mir, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Mir, output_annotated);
    }

    fn run_diagnostics(&mut self) {
        let mut output = String::new();
        let mut output_annotated = Vec::new();

        // Collect all diagnostics using the unified collect_diagnostics function
        let source_files: Vec<_> = self.source_files.values().copied().collect();
        self.diagnostics = collect_diagnostics(&self.db, self.project_root, &source_files);

        // Build sources and file_paths maps for rendering
        let mut sources: HashMap<FileId, String> = HashMap::new();
        let mut file_paths: HashMap<FileId, PathBuf> = HashMap::new();
        for (path, source_file) in &self.source_files {
            let file_id = source_file.file_id(&self.db);
            sources.insert(file_id, source_file.text(&self.db).to_string());
            file_paths.insert(file_id, path.clone());
        }

        // Group diagnostics by phase and file
        let mut parse_errors: Vec<&Diagnostic> = Vec::new();
        let mut hir_errors: Vec<&Diagnostic> = Vec::new();
        let mut validation_errors: Vec<&Diagnostic> = Vec::new();
        let mut type_errors: Vec<&Diagnostic> = Vec::new();

        for diag in &self.diagnostics {
            match diag.phase {
                DiagnosticPhase::Parse => parse_errors.push(diag),
                DiagnosticPhase::Hir => hir_errors.push(diag),
                DiagnosticPhase::Validation => validation_errors.push(diag),
                DiagnosticPhase::Type => type_errors.push(diag),
            }
        }

        let config = RenderConfig::test();

        // Render parse errors
        if !parse_errors.is_empty() {
            writeln!(output, "── Parse Errors ──").ok();
            output_annotated.push(("── Parse Errors ──".to_string(), LineStatus::Unknown));

            for diag in &parse_errors {
                let rendered = render_diagnostic(diag, &sources, &file_paths, &config);
                for line in rendered.lines() {
                    writeln!(output, "{}", line).ok();
                    output_annotated.push((line.to_string(), LineStatus::Recomputed));
                }
                writeln!(output).ok();
                output_annotated.push((String::new(), LineStatus::Unknown));
            }
        }

        // Render HIR errors
        if !hir_errors.is_empty() {
            writeln!(output, "── HIR Errors ──").ok();
            output_annotated.push(("── HIR Errors ──".to_string(), LineStatus::Unknown));

            for diag in &hir_errors {
                let rendered = render_diagnostic(diag, &sources, &file_paths, &config);
                for line in rendered.lines() {
                    writeln!(output, "{}", line).ok();
                    output_annotated.push((line.to_string(), LineStatus::Recomputed));
                }
                writeln!(output).ok();
                output_annotated.push((String::new(), LineStatus::Unknown));
            }
        }

        // Render validation errors (cross-file duplicates)
        if !validation_errors.is_empty() {
            writeln!(output, "── Validation Errors ──").ok();
            output_annotated.push(("── Validation Errors ──".to_string(), LineStatus::Unknown));

            for diag in &validation_errors {
                let rendered = render_diagnostic(diag, &sources, &file_paths, &config);
                for line in rendered.lines() {
                    writeln!(output, "{}", line).ok();
                    output_annotated.push((line.to_string(), LineStatus::Recomputed));
                }
                writeln!(output).ok();
                output_annotated.push((String::new(), LineStatus::Unknown));
            }
        }

        // Render type errors
        if !type_errors.is_empty() {
            writeln!(output, "── Type Errors ──").ok();
            output_annotated.push(("── Type Errors ──".to_string(), LineStatus::Unknown));

            for diag in &type_errors {
                let rendered = render_diagnostic(diag, &sources, &file_paths, &config);
                for line in rendered.lines() {
                    writeln!(output, "{}", line).ok();
                    output_annotated.push((line.to_string(), LineStatus::Recomputed));
                }
                writeln!(output).ok();
                output_annotated.push((String::new(), LineStatus::Unknown));
            }
        }

        let total_errors = self.diagnostics.len();

        if total_errors == 0 {
            let no_errors = "✓ No errors found".to_string();
            writeln!(output, "{}", no_errors).ok();
            output_annotated.push((no_errors, LineStatus::Cached));
        } else {
            let summary = "─────────────────────────────────────────".to_string();
            writeln!(output, "{}", summary).ok();
            output_annotated.push((summary, LineStatus::Unknown));

            let mut parts = Vec::new();
            if !parse_errors.is_empty() {
                parts.push(format!(
                    "{} parse error{}",
                    parse_errors.len(),
                    if parse_errors.len() == 1 { "" } else { "s" }
                ));
            }
            if !hir_errors.is_empty() {
                parts.push(format!(
                    "{} HIR error{}",
                    hir_errors.len(),
                    if hir_errors.len() == 1 { "" } else { "s" }
                ));
            }
            if !validation_errors.is_empty() {
                parts.push(format!(
                    "{} validation error{}",
                    validation_errors.len(),
                    if validation_errors.len() == 1 {
                        ""
                    } else {
                        "s"
                    }
                ));
            }
            if !type_errors.is_empty() {
                parts.push(format!(
                    "{} type error{}",
                    type_errors.len(),
                    if type_errors.len() == 1 { "" } else { "s" }
                ));
            }
            let total = format!("Total: {}", parts.join(", "));
            writeln!(output, "{}", total).ok();
            output_annotated.push((total, LineStatus::Unknown));
        }

        self.phase_outputs
            .insert(CompilerPhase::Diagnostics, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Diagnostics, output_annotated);
    }

    fn run_codegen(&mut self) {
        // Use compile_files directly with our source files instead of generate_project_bytecode,
        // because project_files(db, root) returns an empty vector (not yet implemented).
        let files: Vec<_> = self.source_files.values().copied().collect();

        let mut output = String::new();
        let mut output_annotated = Vec::new();

        let program = match baml_compiler_emit::compile_files(&self.db, &files) {
            Ok(p) => p,
            Err(err) => {
                writeln!(output, "=== NO CODEGEN DUE TO ERRORS ===").ok();
                output_annotated.push((
                    "=== NO CODEGEN DUE TO ERRORS ===".to_string(),
                    LineStatus::Unknown,
                ));
                writeln!(output, "Error: {:?}", err).ok();
                output_annotated.push((format!("Error: {:?}", err), LineStatus::Unknown));

                self.phase_outputs.insert(CompilerPhase::Codegen, output);
                self.phase_outputs_annotated
                    .insert(CompilerPhase::Codegen, output_annotated);
                return;
            }
        };

        let file_recomputed = !self.modified_files.is_empty();

        // Summary header
        writeln!(output, "=== BYTECODE ===").ok();
        output_annotated.push(("=== BYTECODE ===".to_string(), LineStatus::Unknown));

        writeln!(output, "Functions: {}", program.function_indices.len()).ok();
        output_annotated.push((
            format!("Functions: {}", program.function_indices.len()),
            LineStatus::Unknown,
        ));

        writeln!(output, "Objects: {}", program.objects.len()).ok();
        output_annotated.push((
            format!("Objects: {}", program.objects.len()),
            LineStatus::Unknown,
        ));

        writeln!(output, "Globals: {}", program.globals.len()).ok();
        output_annotated.push((
            format!("Globals: {}", program.globals.len()),
            LineStatus::Unknown,
        ));

        // Show functions and their bytecode using debug formatting (same as baml_test)
        let mut func_names: Vec<_> = program.function_indices.keys().collect();
        func_names.sort();
        for func_name in func_names {
            if let Some(&idx) = program.function_indices.get(func_name)
                && let Some(baml_compiler_emit::Object::Function(func)) = program.objects.get(idx)
            {
                let func_header = format!(
                    "\nFunction {} (arity: {}, kind: {:?}):",
                    func_name, func.arity, func.kind
                );
                writeln!(output, "{}", func_header).ok();
                output_annotated.push((func_header, LineStatus::Unknown));

                // Use empty GlobalPool for compile-time display (no heap available)
                // Pass ObjectPool and compile-time globals to resolve names
                let empty_globals = bex_vm_types::indexable::GlobalPool::new();
                let bytecode_table = bex_vm::debug::display_bytecode(
                    func,
                    &bex_vm::EvalStack::new(),
                    &empty_globals,
                    Some(&program.objects),
                    Some(&program.globals),
                    false, // no colors for static display
                );

                if bytecode_table.is_empty() {
                    writeln!(output, "  (no bytecode)").ok();
                    output_annotated.push((
                        "  (no bytecode)".to_string(),
                        if file_recomputed {
                            LineStatus::Recomputed
                        } else {
                            LineStatus::Cached
                        },
                    ));
                } else {
                    for line in bytecode_table.lines() {
                        let formatted_line = format!("  {}", line);
                        writeln!(output, "{}", formatted_line).ok();
                        output_annotated.push((
                            formatted_line,
                            if file_recomputed {
                                LineStatus::Recomputed
                            } else {
                                LineStatus::Cached
                            },
                        ));
                    }
                }
            }
        }

        self.phase_outputs.insert(CompilerPhase::Codegen, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Codegen, output_annotated);
    }

    fn run_vm_runner(&mut self) {
        use bex_vm_types::{FunctionKind, Object};

        let mut output = String::new();
        let mut output_annotated = Vec::new();

        // Compile the program
        let files: Vec<_> = self.source_files.values().copied().collect();
        let program = match baml_compiler_emit::compile_files(&self.db, &files) {
            Ok(p) => p,
            Err(err) => {
                writeln!(output, "=== VM RUNNER ===").ok();
                output_annotated.push(("=== VM RUNNER ===".to_string(), LineStatus::Unknown));
                writeln!(output).ok();
                output_annotated.push((String::new(), LineStatus::Unknown));
                writeln!(output, "Cannot run VM: codegen failed due to errors").ok();
                output_annotated.push((
                    "Cannot run VM: codegen failed due to errors".to_string(),
                    LineStatus::Unknown,
                ));
                writeln!(output, "Error: {:?}", err).ok();
                output_annotated.push((format!("Error: {:?}", err), LineStatus::Unknown));

                self.phase_outputs.insert(CompilerPhase::VmRunner, output);
                self.phase_outputs_annotated
                    .insert(CompilerPhase::VmRunner, output_annotated);
                return;
            }
        };

        // Extract available executable functions (exclude LLM functions)
        let mut exec_functions: Vec<(String, usize)> = Vec::new();
        for (name, &idx) in &program.function_indices {
            if let Some(Object::Function(func)) = program.objects.get(idx)
                && matches!(func.kind, FunctionKind::Bytecode)
            {
                exec_functions.push((name.clone(), func.arity));
            }
        }
        exec_functions.sort_by(|(a, _), (b, _)| a.cmp(b));

        // Update available functions in state
        self.vm_runner_state.available_functions =
            exec_functions.iter().map(|(n, _)| n.clone()).collect();

        // Ensure selected function index is valid
        if self.vm_runner_state.selected_function >= self.vm_runner_state.available_functions.len()
        {
            self.vm_runner_state.selected_function = 0;
        }

        // Header
        writeln!(output, "=== VM RUNNER ===").ok();
        output_annotated.push(("=== VM RUNNER ===".to_string(), LineStatus::Unknown));
        writeln!(output).ok();
        output_annotated.push((String::new(), LineStatus::Unknown));

        // Show available functions
        writeln!(output, "Available Functions (Exec only):").ok();
        output_annotated.push((
            "Available Functions (Exec only):".to_string(),
            LineStatus::Unknown,
        ));

        if exec_functions.is_empty() {
            writeln!(output, "  (no executable functions found)").ok();
            output_annotated.push((
                "  (no executable functions found)".to_string(),
                LineStatus::Unknown,
            ));
        } else {
            for (i, (name, arity)) in exec_functions.iter().enumerate() {
                let selected = if i == self.vm_runner_state.selected_function {
                    ">"
                } else {
                    " "
                };
                let line = format!("{} [{}] {}() - arity: {}", selected, i, name, arity);
                writeln!(output, "{}", line).ok();
                output_annotated.push((
                    line,
                    if i == self.vm_runner_state.selected_function {
                        LineStatus::Recomputed // Highlight selected
                    } else {
                        LineStatus::Unknown
                    },
                ));
            }
        }

        writeln!(output).ok();
        output_annotated.push((String::new(), LineStatus::Unknown));

        // Show execution result if any
        writeln!(output, "Execution Result:").ok();
        output_annotated.push(("Execution Result:".to_string(), LineStatus::Unknown));

        match &self.vm_runner_state.execution_result {
            None => {
                writeln!(output, "  Press [Enter] to run selected function").ok();
                output_annotated.push((
                    "  Press [Enter] to run selected function".to_string(),
                    LineStatus::Unknown,
                ));
            }
            Some(VmExecutionResult::Success(result)) => {
                writeln!(output, "  Result: {}", result).ok();
                output_annotated.push((format!("  Result: {}", result), LineStatus::Cached));
            }
            Some(VmExecutionResult::Error(err)) => {
                writeln!(output, "  Error: {}", err).ok();
                output_annotated.push((format!("  Error: {}", err), LineStatus::Recomputed));
            }
            Some(VmExecutionResult::RequiresArgs { arity }) => {
                writeln!(
                    output,
                    "  Function requires {} argument(s) - cannot run without args",
                    arity
                )
                .ok();
                output_annotated.push((
                    format!(
                        "  Function requires {} argument(s) - cannot run without args",
                        arity
                    ),
                    LineStatus::Unknown,
                ));
            }
        }

        writeln!(output).ok();
        output_annotated.push((String::new(), LineStatus::Unknown));
        writeln!(output, "Controls:").ok();
        output_annotated.push(("Controls:".to_string(), LineStatus::Unknown));
        writeln!(output, "  [j/k or Up/Down] - Select function").ok();
        output_annotated.push((
            "  [j/k or Up/Down] - Select function".to_string(),
            LineStatus::Unknown,
        ));
        writeln!(output, "  [Enter] - Run selected function (if arity = 0)").ok();
        output_annotated.push((
            "  [Enter] - Run selected function (if arity = 0)".to_string(),
            LineStatus::Unknown,
        ));

        self.phase_outputs.insert(CompilerPhase::VmRunner, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::VmRunner, output_annotated);
    }

    /// Execute the selected function in the VM
    pub(crate) fn execute_selected_function(&mut self) {
        use bex_vm::{BexVm, VmExecState};
        use bex_vm_types::Object;

        let files: Vec<_> = self.source_files.values().copied().collect();
        let program = match baml_compiler_emit::compile_files(&self.db, &files) {
            Ok(p) => p,
            Err(err) => {
                self.vm_runner_state.execution_result = Some(VmExecutionResult::Error(format!(
                    "Codegen failed: {:?}",
                    err
                )));
                return;
            }
        };

        let Some(func_name) = self
            .vm_runner_state
            .available_functions
            .get(self.vm_runner_state.selected_function)
        else {
            self.vm_runner_state.execution_result =
                Some(VmExecutionResult::Error("No function selected".to_string()));
            return;
        };

        let Some(func_index) = program.function_index(func_name) else {
            self.vm_runner_state.execution_result = Some(VmExecutionResult::Error(format!(
                "Function '{}' not found",
                func_name
            )));
            return;
        };

        // Check function arity
        if let Some(Object::Function(func)) = program.objects.get(func_index)
            && func.arity > 0
        {
            self.vm_runner_state.execution_result =
                Some(VmExecutionResult::RequiresArgs { arity: func.arity });
            return;
        }

        // Create VM and run
        let mut vm = match BexVm::from_program(program) {
            Ok(vm) => vm,
            Err(err) => {
                self.vm_runner_state.execution_result =
                    Some(VmExecutionResult::Error(format!("{:?}", err)));
                return;
            }
        };
        // Convert compile-time index to runtime HeapPtr
        let func_ptr = vm.heap.compile_time_ptr(func_index);
        vm.set_entry_point(func_ptr, &[]);

        match vm.exec() {
            Ok(VmExecState::Complete(value)) => {
                let result_str = format_vm_value(&value, &vm);
                self.vm_runner_state.execution_result =
                    Some(VmExecutionResult::Success(result_str));
            }
            Ok(VmExecState::Await(_)) => {
                self.vm_runner_state.execution_result = Some(VmExecutionResult::Error(
                    "Function awaits a future (not supported in VM Runner)".to_string(),
                ));
            }
            Ok(VmExecState::ScheduleFuture(_)) => {
                self.vm_runner_state.execution_result = Some(VmExecutionResult::Error(
                    "Function schedules a future (not supported in VM Runner)".to_string(),
                ));
            }
            Ok(VmExecState::Notify(_)) => {
                self.vm_runner_state.execution_result = Some(VmExecutionResult::Error(
                    "Function sent a watch notification (not supported in VM Runner)".to_string(),
                ));
            }
            Err(e) => {
                self.vm_runner_state.execution_result =
                    Some(VmExecutionResult::Error(format!("{:?}", e)));
            }
        }

        // Regenerate output to show the result
        self.run_vm_runner();
    }

    /// Get mutable VM runner state for UI
    pub(crate) fn vm_runner_state_mut(&mut self) -> &mut VmRunnerState {
        &mut self.vm_runner_state
    }

    fn run_metrics(&mut self) {
        let mut output = String::new();

        let recomputed = self.recomputed_queries.lock().unwrap();
        let cached = self.cached_queries.lock().unwrap();

        writeln!(output, "Recomputed Queries: {}", recomputed.len()).ok();
        writeln!(output, "Cached Queries: {}", cached.len()).ok();
        writeln!(output).ok();

        if !recomputed.is_empty() {
            writeln!(output, "Recomputed:").ok();
            for query in recomputed.iter() {
                writeln!(output, "  • {query}").ok();
            }
            writeln!(output).ok();
        }

        if !cached.is_empty() {
            writeln!(output, "Cached:").ok();
            for query in cached.iter() {
                writeln!(output, "  • {query}").ok();
            }
        }

        let output_annotated: Vec<_> = output
            .lines()
            .map(|line| (line.to_string(), LineStatus::Unknown))
            .collect();

        self.phase_outputs.insert(CompilerPhase::Metrics, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Metrics, output_annotated);
    }

    pub(crate) fn get_phase_output(&self, phase: CompilerPhase) -> Option<&str> {
        self.phase_outputs
            .get(&phase)
            .map(std::string::String::as_str)
    }

    pub(crate) fn parser_cache_snapshot(&self) -> HashMap<PathBuf, HashSet<GreenElementId>> {
        self.parser_cached_elements.clone()
    }

    pub(crate) fn set_parser_cache_baseline(
        &mut self,
        baseline: &HashMap<PathBuf, HashSet<GreenElementId>>,
    ) {
        self.parser_cached_elements = baseline.clone();
    }

    pub(crate) fn get_recomputation_status(&self, _phase: CompilerPhase) -> RecomputationStatus {
        let recomputed_count = self.recomputed_queries.lock().unwrap().len();
        let cached_count = self.cached_queries.lock().unwrap().len();
        RecomputationStatus::Summary {
            recomputed_count,
            cached_count,
        }
    }

    pub(crate) fn get_annotated_output(&self, phase: CompilerPhase) -> Vec<(String, LineStatus)> {
        self.phase_outputs_annotated
            .get(&phase)
            .cloned()
            .unwrap_or_default()
    }

    /// Get the current THIR display mode
    pub(crate) fn thir_display_mode(&self) -> ThirDisplayMode {
        self.thir_display_mode
    }

    /// Set the THIR display mode
    pub(crate) fn set_thir_display_mode(&mut self, mode: ThirDisplayMode) {
        self.thir_display_mode = mode;
    }

    /// Get the THIR interactive state
    pub(crate) fn thir_interactive_state(&self) -> &ThirInteractiveState {
        &self.thir_interactive_state
    }

    /// Get mutable reference to THIR interactive state
    pub(crate) fn thir_interactive_state_mut(&mut self) -> &mut ThirInteractiveState {
        &mut self.thir_interactive_state
    }

    /// Format THIR output for interactive mode
    pub(crate) fn format_thir_interactive(&mut self) {
        // Get the THIR tree output and parse it into interactive state
        if let Some(output) = self.phase_outputs.get(&CompilerPhase::Thir) {
            let lines: Vec<String> = output.lines().map(|s| s.to_string()).collect();
            self.thir_interactive_state.source_lines = lines.clone();
            self.thir_interactive_state.total_lines = lines.len();
            // Reset cursor if needed
            if self.thir_interactive_state.cursor_line >= self.thir_interactive_state.total_lines {
                self.thir_interactive_state.cursor_line = 0;
            }
        }
    }

    /// Get annotated output with mode-specific coloring
    pub(crate) fn get_annotated_output_with_mode(
        &self,
        phase: CompilerPhase,
        mode: VisualizationMode,
    ) -> Vec<(String, LineStatus)> {
        match mode {
            VisualizationMode::Incremental => {
                // In incremental mode, use the original annotations (recomputed vs cached)
                self.get_annotated_output(phase)
            }
            VisualizationMode::Diff => {
                if let Some(lines) = self.phase_outputs_annotated.get(&phase) {
                    let mut current_file_modified = false;
                    let mut saw_file_header = false;
                    let mut diff_lines = Vec::with_capacity(lines.len());

                    for (text, _status) in lines {
                        if let Some(path_str) = text.strip_prefix("File: ") {
                            saw_file_header = true;
                            let path = PathBuf::from(path_str);
                            current_file_modified = self.modified_files.contains(&path);
                            let header_status = if current_file_modified {
                                LineStatus::Recomputed
                            } else {
                                LineStatus::Unknown
                            };
                            diff_lines.push((text.clone(), header_status));
                            continue;
                        }

                        if text.is_empty() {
                            diff_lines.push((text.clone(), LineStatus::Unknown));
                            continue;
                        }

                        let status = if current_file_modified {
                            LineStatus::Recomputed
                        } else {
                            LineStatus::Cached
                        };
                        diff_lines.push((text.clone(), status));
                    }

                    if saw_file_header {
                        diff_lines
                    } else {
                        lines
                            .iter()
                            .map(|(text, status)| (text.clone(), *status))
                            .collect()
                    }
                } else {
                    Vec::new()
                }
            }
        }
    }

    pub(crate) fn get_metrics_output(&mut self) -> String {
        self.run_metrics();
        self.phase_outputs
            .get(&CompilerPhase::Metrics)
            .cloned()
            .unwrap_or_default()
    }
}

/// Format an AST item into a tree-based string representation
fn format_ast_item(item: &AstItem) -> String {
    let mut output = String::new();
    format_item_tree(item, &mut output, 0);
    output
}

/// Recursively format an AST item as a tree
fn format_item_tree(item: &AstItem, output: &mut String, indent: usize) {
    use baml_compiler_syntax::ast::*;

    match item {
        Item::Function(func) => format_function(func, output, indent),
        Item::Class(class) => format_class(class, output, indent),
        Item::Enum(enum_def) => format_enum(enum_def, output, indent),
        Item::Client(client) => format_client(client, output, indent),
        Item::Test(test) => format_test(test, output, indent),
        Item::RetryPolicy(policy) => format_retry_policy(policy, output, indent),
        Item::TemplateString(template) => format_template_string(template, output, indent),
        Item::TypeAlias(alias) => format_type_alias(alias, output, indent),
    }
}

fn write_indent(output: &mut String, indent: usize) {
    output.push_str(&"  ".repeat(indent));
}

fn format_function(
    func: &baml_compiler_syntax::ast::FunctionDef,
    output: &mut String,
    indent: usize,
) {
    write_indent(output, indent);
    writeln!(output, "FUNCTION").ok();

    // Function name
    if let Some(name) = func.name() {
        write_indent(output, indent + 1);
        writeln!(output, "NAME {}", name.text()).ok();
    }

    // Parameters
    if let Some(param_list) = func.param_list() {
        let params: Vec<_> = param_list.params().collect();
        if !params.is_empty() {
            write_indent(output, indent + 1);
            writeln!(output, "PARAMS").ok();
            for param in params {
                format_parameter(&param, output, indent + 2);
            }
        }
    }

    // Return type
    if let Some(return_type) = func.return_type() {
        write_indent(output, indent + 1);
        writeln!(output, "RETURN_TYPE {}", return_type.syntax().text()).ok();
    }

    // Body
    if let Some(expr_body) = func.expr_body() {
        write_indent(output, indent + 1);
        writeln!(output, "BODY").ok();
        format_expr_function_body(&expr_body, output, indent + 2);
    } else if let Some(llm_body) = func.llm_body() {
        write_indent(output, indent + 1);
        writeln!(output, "BODY").ok();
        format_llm_function_body(&llm_body, output, indent + 2);
    }
}

fn format_parameter(
    param: &baml_compiler_syntax::ast::Parameter,
    output: &mut String,
    indent: usize,
) {
    write_indent(output, indent);
    writeln!(output, "PARAM").ok();

    // Parameter name
    if let Some(name_token) = param
        .syntax()
        .children_with_tokens()
        .filter_map(|n| n.into_token())
        .find(|t| t.kind() == baml_compiler_syntax::SyntaxKind::WORD)
    {
        write_indent(output, indent + 1);
        writeln!(output, "NAME {}", name_token.text()).ok();
    }

    // Parameter type
    if let Some(ty) = param
        .syntax()
        .children()
        .find_map(baml_compiler_syntax::ast::TypeExpr::cast)
    {
        write_indent(output, indent + 1);
        writeln!(output, "TYPE {}", ty.syntax().text()).ok();
    }
}

fn format_expr_function_body(
    body: &baml_compiler_syntax::ast::ExprFunctionBody,
    output: &mut String,
    indent: usize,
) {
    use baml_compiler_syntax::ast::*;

    // Look for block expression or other expression types
    if let Some(block) = body.syntax().children().find_map(BlockExpr::cast) {
        write_indent(output, indent);
        writeln!(output, "EXPR_BLOCK").ok();
        format_block_expr(&block, output, indent + 1);
    } else if let Some(expr) = body.syntax().children().find_map(Expr::cast) {
        format_expr(&expr, output, indent);
    } else {
        // Fallback: show raw syntax
        write_indent(output, indent);
        writeln!(output, "EXPR {}", body.syntax().text()).ok();
    }
}

fn format_llm_function_body(
    body: &baml_compiler_syntax::ast::LlmFunctionBody,
    output: &mut String,
    indent: usize,
) {
    write_indent(output, indent);
    writeln!(output, "LLM_BODY").ok();

    // Show config items
    for config_item in body
        .syntax()
        .children()
        .filter_map(baml_compiler_syntax::ast::ConfigItem::cast)
    {
        format_config_item(&config_item, output, indent + 1);
    }
}

fn format_config_item(
    item: &baml_compiler_syntax::ast::ConfigItem,
    output: &mut String,
    indent: usize,
) {
    write_indent(output, indent);
    let text = item.syntax().text().to_string();
    // Truncate long config values
    if text.len() > 60 {
        writeln!(output, "CONFIG {}...", &text[..60]).ok();
    } else {
        writeln!(output, "CONFIG {}", text).ok();
    }
}

fn format_block_expr(
    block: &baml_compiler_syntax::ast::BlockExpr,
    output: &mut String,
    indent: usize,
) {
    use baml_compiler_syntax::ast::*;

    // Iterate through statements in the block
    for child in block.syntax().children() {
        if let Some(let_stmt) = LetStmt::cast(child.clone()) {
            format_let_stmt(&let_stmt, output, indent);
        } else if let Some(if_expr) = IfExpr::cast(child.clone()) {
            format_if_expr(&if_expr, output, indent);
        } else if let Some(expr) = Expr::cast(child.clone()) {
            format_expr(&expr, output, indent);
        }
    }
}

fn format_let_stmt(stmt: &baml_compiler_syntax::ast::LetStmt, output: &mut String, indent: usize) {
    use baml_compiler_syntax::ast::*;

    write_indent(output, indent);
    writeln!(output, "STMT_LET").ok();

    // Find the identifier name
    if let Some(name_token) = stmt
        .syntax()
        .children_with_tokens()
        .filter_map(|n| n.into_token())
        .find(|t| t.kind() == baml_compiler_syntax::SyntaxKind::WORD)
    {
        write_indent(output, indent + 1);
        writeln!(output, "NAME {}", name_token.text()).ok();
    }

    // Find the value expression
    if let Some(expr) = stmt.syntax().children().find_map(Expr::cast) {
        write_indent(output, indent + 1);
        writeln!(output, "VALUE").ok();
        format_expr(&expr, output, indent + 2);
    }
}

fn format_if_expr(if_expr: &baml_compiler_syntax::ast::IfExpr, output: &mut String, indent: usize) {
    write_indent(output, indent);
    writeln!(output, "EXPR_IF").ok();

    // Condition
    write_indent(output, indent + 1);
    writeln!(output, "CONDITION").ok();
    if let Some(cond) = if_expr
        .syntax()
        .children()
        .find_map(baml_compiler_syntax::ast::Expr::cast)
    {
        format_expr(&cond, output, indent + 2);
    }

    // Then branch
    write_indent(output, indent + 1);
    writeln!(output, "THEN").ok();
    if let Some(then_block) = if_expr
        .syntax()
        .children()
        .filter_map(baml_compiler_syntax::ast::BlockExpr::cast)
        .next()
    {
        format_block_expr(&then_block, output, indent + 2);
    }
}

fn format_expr(expr: &baml_compiler_syntax::ast::Expr, output: &mut String, indent: usize) {
    let text = expr.syntax().text().to_string();

    // If expression is simple (< 40 chars), inline it
    if text.len() < 40 && !text.contains('\n') {
        write_indent(output, indent);
        writeln!(output, "EXPR {}", text.trim()).ok();
    } else {
        // Complex expression: show structure
        write_indent(output, indent);
        writeln!(output, "EXPR").ok();
        write_indent(output, indent + 1);
        writeln!(output, "{}", text.trim()).ok();
    }
}

fn format_class(class: &baml_compiler_syntax::ast::ClassDef, output: &mut String, indent: usize) {
    write_indent(output, indent);
    writeln!(output, "CLASS").ok();

    // Class name
    if let Some(name) = class.name() {
        write_indent(output, indent + 1);
        writeln!(output, "NAME {}", name.text()).ok();
    }

    // Fields
    let fields: Vec<_> = class.fields().collect();
    if !fields.is_empty() {
        write_indent(output, indent + 1);
        writeln!(output, "FIELDS").ok();
        for field in fields {
            format_field(&field, output, indent + 2);
        }
    }
}

fn format_field(field: &baml_compiler_syntax::ast::Field, output: &mut String, indent: usize) {
    write_indent(output, indent);
    writeln!(output, "FIELD").ok();

    // Field name
    if let Some(name) = field.name() {
        write_indent(output, indent + 1);
        writeln!(output, "NAME {}", name.text()).ok();
    }

    // Field type
    if let Some(ty) = field.ty() {
        write_indent(output, indent + 1);
        writeln!(output, "TYPE {}", ty.syntax().text()).ok();
    }
}

fn format_enum(enum_def: &baml_compiler_syntax::ast::EnumDef, output: &mut String, indent: usize) {
    write_indent(output, indent);
    writeln!(output, "ENUM").ok();

    // Enum name
    if let Some(name) = enum_def
        .syntax()
        .children_with_tokens()
        .filter_map(|n| n.into_token())
        .filter(|t| t.kind() == baml_compiler_syntax::SyntaxKind::WORD)
        .nth(1)
    {
        write_indent(output, indent + 1);
        writeln!(output, "NAME {}", name.text()).ok();
    }
}

fn format_client(
    client: &baml_compiler_syntax::ast::ClientDef,
    output: &mut String,
    indent: usize,
) {
    write_indent(output, indent);
    writeln!(output, "CLIENT").ok();

    // Client name
    if let Some(name) = client
        .syntax()
        .children_with_tokens()
        .filter_map(|n| n.into_token())
        .filter(|t| t.kind() == baml_compiler_syntax::SyntaxKind::WORD)
        .nth(1)
    {
        write_indent(output, indent + 1);
        writeln!(output, "NAME {}", name.text()).ok();
    }
}

fn format_test(test: &baml_compiler_syntax::ast::TestDef, output: &mut String, indent: usize) {
    write_indent(output, indent);
    writeln!(output, "TEST").ok();

    // Test name
    if let Some(name) = test
        .syntax()
        .children_with_tokens()
        .filter_map(|n| n.into_token())
        .filter(|t| t.kind() == baml_compiler_syntax::SyntaxKind::WORD)
        .nth(1)
    {
        write_indent(output, indent + 1);
        writeln!(output, "NAME {}", name.text()).ok();
    }
}

fn format_retry_policy(
    policy: &baml_compiler_syntax::ast::RetryPolicyDef,
    output: &mut String,
    indent: usize,
) {
    write_indent(output, indent);
    writeln!(output, "RETRY_POLICY").ok();

    // Policy name
    if let Some(name) = policy
        .syntax()
        .children_with_tokens()
        .filter_map(|n| n.into_token())
        .filter(|t| t.kind() == baml_compiler_syntax::SyntaxKind::WORD)
        .nth(1)
    {
        write_indent(output, indent + 1);
        writeln!(output, "NAME {}", name.text()).ok();
    }
}

fn format_template_string(
    template: &baml_compiler_syntax::ast::TemplateStringDef,
    output: &mut String,
    indent: usize,
) {
    write_indent(output, indent);
    writeln!(output, "TEMPLATE_STRING").ok();

    // Template name
    if let Some(name) = template
        .syntax()
        .children_with_tokens()
        .filter_map(|n| n.into_token())
        .filter(|t| t.kind() == baml_compiler_syntax::SyntaxKind::WORD)
        .nth(1)
    {
        write_indent(output, indent + 1);
        writeln!(output, "NAME {}", name.text()).ok();
    }
}

fn format_type_alias(
    alias: &baml_compiler_syntax::ast::TypeAliasDef,
    output: &mut String,
    indent: usize,
) {
    write_indent(output, indent);
    writeln!(output, "TYPE_ALIAS").ok();

    // Alias name
    if let Some(name) = alias
        .syntax()
        .children_with_tokens()
        .filter_map(|n| n.into_token())
        .filter(|t| t.kind() == baml_compiler_syntax::SyntaxKind::WORD)
        .nth(1)
    {
        write_indent(output, indent + 1);
        writeln!(output, "NAME {}", name.text()).ok();
    }
}

fn format_syntax_tree_with_cache(
    syntax_tree: &SyntaxNode,
    previous: Option<&HashSet<GreenElementId>>,
) -> (Vec<(String, LineStatus)>, HashSet<GreenElementId>) {
    let mut indent_level = 0usize;
    let mut lines = Vec::new();
    let mut current_ids = HashSet::new();
    let mut owned_nodes: Vec<GreenNode> = Vec::new();

    for event in syntax_tree.preorder_with_tokens() {
        match event {
            WalkEvent::Enter(element) => {
                let indent = "  ".repeat(indent_level);
                match element {
                    SyntaxElement::Node(node) => {
                        let (id, was_borrowed) = GreenElementId::from_node(&node, &mut owned_nodes);
                        let status = line_status_for(&id, previous);
                        current_ids.insert(id);
                        let raw_line = format!("{indent}{:?}", node);
                        let mut line = remove_span_ranges(&raw_line);
                        if !was_borrowed {
                            line.push_str("  /* owned */");
                        }
                        lines.push((line, status));
                    }
                    SyntaxElement::Token(token) => {
                        let id = GreenElementId::from_token(&token);
                        let status = line_status_for(&id, previous);
                        current_ids.insert(id);
                        let raw_line = format!("{indent}{:?}", token);
                        let line = remove_span_ranges(&raw_line);
                        lines.push((line, status));
                    }
                }
                indent_level += 1;
            }
            WalkEvent::Leave(_) => {
                indent_level = indent_level.saturating_sub(1);
            }
        }
    }

    (lines, current_ids)
}

fn line_status_for(id: &GreenElementId, previous: Option<&HashSet<GreenElementId>>) -> LineStatus {
    if previous.is_some_and(|set| set.contains(id)) {
        LineStatus::Cached
    } else {
        LineStatus::Recomputed
    }
}

#[derive(Debug, Clone)]
pub(crate) enum RecomputationStatus {
    Summary {
        recomputed_count: usize,
        cached_count: usize,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct GreenElementId {
    ptr: *const (),
    kind: GreenElementKind,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum GreenElementKind {
    Node,
    Token,
}

impl GreenElementId {
    fn from_node(node: &SyntaxNode, owned_nodes: &mut Vec<GreenNode>) -> (Self, bool) {
        match node.green() {
            Cow::Borrowed(data) => (
                Self {
                    ptr: data as *const _ as *const (),
                    kind: GreenElementKind::Node,
                },
                true,
            ),
            Cow::Owned(green) => {
                owned_nodes.push(green);
                let data = owned_nodes
                    .last()
                    .map(|node| node.deref() as *const _ as *const ())
                    .unwrap();
                (
                    Self {
                        ptr: data,
                        kind: GreenElementKind::Node,
                    },
                    false,
                )
            }
        }
    }

    fn from_token(token: &SyntaxToken) -> Self {
        Self {
            ptr: token.green() as *const _ as *const (),
            kind: GreenElementKind::Token,
        }
    }
}

/// Helper to remove span ranges like @0..69 from CST output
fn remove_span_ranges(text: &str) -> String {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"@\d+\.\.\d+").unwrap());
    re.replace_all(text, "").to_string()
}

/// Helper to read files from disk into a `HashMap`
pub(crate) fn read_files_from_disk(path: &Path) -> Result<HashMap<PathBuf, String>> {
    let mut files = HashMap::new();

    if path.is_dir() {
        let discovered = baml_workspace::discover_baml_files(path);
        for file_path in discovered {
            if let Ok(content) = std::fs::read_to_string(&file_path) {
                files.insert(file_path, content);
            }
        }
    } else {
        let content = std::fs::read_to_string(path)?;
        files.insert(path.to_path_buf(), content);
    }

    Ok(files)
}

pub(crate) fn normalize_files_to_virtual_root(
    files: HashMap<PathBuf, String>,
    root: &Path,
) -> HashMap<PathBuf, String> {
    let virtual_root = Path::new("/baml_src");
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());

    files
        .into_iter()
        .map(|(path, contents)| {
            let relative = if let Ok(rel) = path.strip_prefix(root) {
                rel.to_path_buf()
            } else if let Ok(canonical_path) = path.canonicalize() {
                canonical_path
                    .strip_prefix(&canonical_root)
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| {
                        path.file_name()
                            .map(PathBuf::from)
                            .unwrap_or_else(|| PathBuf::from("unknown.baml"))
                    })
            } else {
                path.file_name()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("unknown.baml"))
            };

            (virtual_root.join(relative), contents)
        })
        .collect()
}

/// Format a VM value for display
fn format_vm_value(value: &bex_vm_types::Value, vm: &bex_vm::BexVm) -> String {
    use bex_vm_types::{Object, Value};

    match value {
        Value::Null => "null".to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Object(idx) => {
            let obj = vm.get_object(*idx);
            match obj {
                Object::String(s) => format!("\"{}\"", s),
                Object::Array(arr) => {
                    let items: Vec<String> = arr.iter().map(|v| format_vm_value(v, vm)).collect();
                    format!("[{}]", items.join(", "))
                }
                Object::Map(map) => {
                    let items: Vec<String> = map
                        .iter()
                        .map(|(k, v)| format!("\"{}\": {}", k, format_vm_value(v, vm)))
                        .collect();
                    format!("{{{}}}", items.join(", "))
                }
                Object::Instance(inst) => {
                    let class_obj = vm.get_object(inst.class);
                    if let Object::Class(class) = class_obj {
                        let fields: Vec<String> = class
                            .field_names
                            .iter()
                            .zip(inst.fields.iter())
                            .map(|(name, val)| format!("{}: {}", name, format_vm_value(val, vm)))
                            .collect();
                        format!("{}{{ {} }}", class.name, fields.join(", "))
                    } else {
                        "<instance>".to_string()
                    }
                }
                Object::Variant(var) => {
                    let enum_obj = vm.get_object(var.enm);
                    if let Object::Enum(enm) = enum_obj {
                        if let Some(name) = enm.variant_names.get(var.index) {
                            format!("{}::{}", enm.name, name)
                        } else {
                            format!("{}::variant_{}", enm.name, var.index)
                        }
                    } else {
                        "<variant>".to_string()
                    }
                }
                Object::Function(f) => format!("<fn {}>", f.name),
                Object::Class(c) => format!("<class {}>", c.name),
                Object::Media(m) => format!("<type {}>", m.kind),
                Object::Enum(e) => format!("<enum {}>", e.name),
                Object::Future(_) => "<future>".to_string(),
                Object::Resource(r) => format!("<resource: {}>", r),
                Object::PromptAst(_) => "<prompt_ast>".to_string(),
                #[cfg(feature = "heap_debug")]
                Object::Sentinel(_) => "<sentinel>".to_string(),
            }
        }
    }
}
