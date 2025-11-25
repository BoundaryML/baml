use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    fmt::Write,
    ops::Deref,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::Result;
use baml_base::Diagnostic;
use baml_db::{
    RootDatabase, SourceFile, baml_codegen, baml_hir, baml_lexer, baml_parser, baml_syntax,
    baml_thir, baml_workspace, function_body, function_signature,
};
use baml_hir::{Expr, ExprBody, ExprId, FunctionBody, ItemId, Pattern, Stmt, StmtId};
use baml_syntax::{
    SyntaxElement, SyntaxNode, SyntaxToken, WalkEvent,
    ast::{Item as AstItem, SourceFile as AstSourceFile},
};
use baml_thir::{InferenceResult, Ty};
use regex::Regex;
use rowan::{GreenNode, NodeCache, ast::AstNode};
use salsa::{Event, EventKind, Setter};

/// Display mode for the THIR phase
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ThirDisplayMode {
    /// Tree view showing expression structure with types
    #[default]
    Tree,
    /// Interactive cursor mode for exploring types
    Interactive,
}

impl ThirDisplayMode {
    pub(crate) fn name(self) -> &'static str {
        match self {
            ThirDisplayMode::Tree => "Tree",
            ThirDisplayMode::Interactive => "Interactive",
        }
    }

    pub(crate) fn toggle(self) -> Self {
        match self {
            ThirDisplayMode::Tree => ThirDisplayMode::Interactive,
            ThirDisplayMode::Interactive => ThirDisplayMode::Tree,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum CompilerPhase {
    Lexer,
    Parser,
    Ast,
    Hir,
    Thir,
    Diagnostics,
    Codegen,
    Metrics,
}

impl CompilerPhase {
    pub(crate) const ALL: &'static [CompilerPhase] = &[
        CompilerPhase::Lexer,
        CompilerPhase::Parser,
        CompilerPhase::Ast,
        CompilerPhase::Hir,
        CompilerPhase::Thir,
        CompilerPhase::Diagnostics,
        CompilerPhase::Codegen,
        CompilerPhase::Metrics,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            CompilerPhase::Lexer => "Lexer (Tokens)",
            CompilerPhase::Parser => "Parser (CST)",
            CompilerPhase::Ast => "AST (Typed Nodes)",
            CompilerPhase::Hir => "HIR (High-level IR)",
            CompilerPhase::Thir => "THIR (Typed IR)",
            CompilerPhase::Diagnostics => "Diagnostics",
            CompilerPhase::Codegen => "Codegen (Bytecode)",
            CompilerPhase::Metrics => "Metrics (Incremental)",
        }
    }

    pub(crate) fn next(self) -> CompilerPhase {
        match self {
            CompilerPhase::Lexer => CompilerPhase::Parser,
            CompilerPhase::Parser => CompilerPhase::Ast,
            CompilerPhase::Ast => CompilerPhase::Hir,
            CompilerPhase::Hir => CompilerPhase::Thir,
            CompilerPhase::Thir => CompilerPhase::Diagnostics,
            CompilerPhase::Diagnostics => CompilerPhase::Codegen,
            CompilerPhase::Codegen => CompilerPhase::Metrics,
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
            CompilerPhase::Diagnostics => CompilerPhase::Thir,
            CompilerPhase::Codegen => CompilerPhase::Diagnostics,
            CompilerPhase::Metrics => CompilerPhase::Codegen,
        }
    }
}

pub(crate) struct CompilerRunner {
    db: RootDatabase,
    project_root: baml_workspace::ProjectRoot,
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

#[derive(Debug, Clone)]
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
            RootDatabase::new_with_event_callback(Box::new(move |event: Event| match event.kind {
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
            }));

        Self {
            project_root: baml_workspace::ProjectRoot::new(&db, PathBuf::new()),
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
        }
    }

    pub(crate) fn thir_display_mode(&self) -> ThirDisplayMode {
        self.thir_display_mode
    }

    pub(crate) fn set_thir_display_mode(&mut self, mode: ThirDisplayMode) {
        self.thir_display_mode = mode;
    }

    pub(crate) fn thir_interactive_state(&self) -> &ThirInteractiveState {
        &self.thir_interactive_state
    }

    pub(crate) fn thir_interactive_state_mut(&mut self) -> &mut ThirInteractiveState {
        &mut self.thir_interactive_state
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
            RootDatabase::new_with_event_callback(Box::new(move |event: Event| match event.kind {
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

        // Run all compiler phases
        self.run_all_phases();
    }

    fn run_all_phases(&mut self) {
        self.phase_outputs.clear();
        self.phase_outputs_annotated.clear();

        for &phase in &[
            CompilerPhase::Lexer,
            CompilerPhase::Parser,
            CompilerPhase::Ast,
            CompilerPhase::Hir,
            CompilerPhase::Thir,
            CompilerPhase::Diagnostics,
            CompilerPhase::Codegen,
        ] {
            self.run_single_phase(phase);
        }

        self.run_single_phase(CompilerPhase::Metrics);
    }

    fn run_single_phase(&mut self, phase: CompilerPhase) {
        match phase {
            CompilerPhase::Lexer => self.run_lexer(),
            CompilerPhase::Parser => self.run_parser(),
            CompilerPhase::Ast => self.run_ast(),
            CompilerPhase::Hir => self.run_hir(),
            CompilerPhase::Thir => self.run_thir(),
            CompilerPhase::Diagnostics => self.run_diagnostics(),
            CompilerPhase::Codegen => self.run_codegen(),
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

            let tokens = baml_lexer::lex_file(&self.db, *source_file);
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

            let tokens = baml_lexer::lex_file(&self.db, *source_file);
            let (green, _errors) =
                baml_parser::parse_file_with_cache(&tokens, &mut self.node_cache);
            let syntax_tree = baml_syntax::SyntaxNode::new_root(green.clone());

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
            let tokens = baml_lexer::lex_file(&self.db, *source_file);
            let (green, _errors) =
                baml_parser::parse_file_with_cache(&tokens, &mut self.node_cache);
            let syntax_tree = baml_syntax::SyntaxNode::new_root(green.clone());

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

            // Use real baml_hir for item extraction
            let items_struct = baml_hir::file_items(&self.db, *source_file);
            let items = items_struct.items(&self.db);

            // Check if THIS specific file was modified
            let file_recomputed = self.modified_files.contains(path);

            writeln!(output, "File: {file_path}").ok();
            output_annotated.push((format!("File: {file_path}"), LineStatus::Unknown));

            // Show real HIR items
            if !items.is_empty() {
                for item in items {
                    let item_line = format!("  {item:?}");
                    writeln!(output, "{item_line}").ok();
                    output_annotated.push((
                        item_line,
                        if file_recomputed {
                            LineStatus::Recomputed
                        } else {
                            LineStatus::Cached
                        },
                    ));
                }
            } else {
                let no_items = "  (no items)".to_string();
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

        self.phase_outputs.insert(CompilerPhase::Hir, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Hir, output_annotated);
    }

    fn run_thir(&mut self) {
        let mut output = String::new();
        let mut output_annotated = Vec::new();
        let mut interactive_state = ThirInteractiveState::default();

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
            let items_struct = baml_hir::file_items(&self.db, *source_file);
            let items = items_struct.items(&self.db);

            for item in items {
                if let ItemId::Function(func_id) = item {
                    let signature = function_signature(&self.db, *source_file, *func_id);
                    let func_name = signature.name.to_string();
                    let body = function_body(&self.db, *source_file, *func_id);

                    // Run type inference
                    let inference_result = baml_thir::infer_function(&self.db, &signature, &body);

                    match self.thir_display_mode {
                        ThirDisplayMode::Tree => {
                            // Tree view: use baml_thir's render_function_tree
                            let tree_output = baml_thir::render_function_tree(
                                &self.db,
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
                        ThirDisplayMode::Interactive => {
                            // Interactive view: show source-like representation
                            self.format_thir_interactive(
                                &func_name,
                                &signature,
                                &body,
                                &inference_result,
                                &mut output,
                                &mut output_annotated,
                                &mut interactive_state,
                                file_recomputed,
                            );
                        }
                    }
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

    /// Format THIR for interactive mode (source-like with cursor support)
    #[allow(clippy::too_many_arguments)]
    fn format_thir_interactive(
        &self,
        func_name: &str,
        signature: &baml_hir::FunctionSignature,
        body: &FunctionBody,
        result: &InferenceResult<'_>,
        output: &mut String,
        output_annotated: &mut Vec<(String, LineStatus)>,
        state: &mut ThirInteractiveState,
        file_recomputed: bool,
    ) {
        let status = if file_recomputed {
            LineStatus::Recomputed
        } else {
            LineStatus::Cached
        };

        // Function header
        let return_type = baml_thir::lower_type_ref(&self.db, &signature.return_type);
        let params_str: Vec<String> = signature
            .params
            .iter()
            .map(|p| {
                let ty = baml_thir::lower_type_ref(&self.db, &p.type_ref);
                format!("{}: {ty}", p.name)
            })
            .collect();
        let header = format!(
            "function {func_name}({}) -> {return_type} {{",
            params_str.join(", ")
        );
        writeln!(output, "{header}").ok();
        output_annotated.push((header.clone(), status));
        state.source_lines.push(header);
        state.line_info.push(ThirLineInfo {
            function_name: func_name.to_string(),
            expr_type: Some(return_type.to_string()),
            description: format!("Function signature: returns {return_type}"),
        });

        // Format body
        match body {
            FunctionBody::Expr(expr_body) => {
                if let Some(root_expr) = expr_body.root_expr {
                    self.format_expr_interactive(
                        root_expr,
                        expr_body,
                        result,
                        output,
                        output_annotated,
                        state,
                        func_name,
                        1,
                        status,
                    );
                }
            }
            FunctionBody::Llm(llm_body) => {
                let client = llm_body
                    .client
                    .as_ref()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "none".to_string());
                let line = format!("  client {client}");
                writeln!(output, "{line}").ok();
                output_annotated.push((line.clone(), status));
                state.source_lines.push(line);
                state.line_info.push(ThirLineInfo {
                    function_name: func_name.to_string(),
                    expr_type: None,
                    description: format!("LLM client: {client}"),
                });
            }
            FunctionBody::Missing => {
                let line = "  <missing body>".to_string();
                writeln!(output, "{line}").ok();
                output_annotated.push((line.clone(), status));
                state.source_lines.push(line);
                state.line_info.push(ThirLineInfo {
                    function_name: func_name.to_string(),
                    expr_type: None,
                    description: "Missing function body".to_string(),
                });
            }
        }

        // Closing brace
        let close = "}".to_string();
        writeln!(output, "{close}").ok();
        output_annotated.push((close.clone(), status));
        state.source_lines.push(close);
        state.line_info.push(ThirLineInfo {
            function_name: func_name.to_string(),
            expr_type: None,
            description: "End of function".to_string(),
        });

        // Show errors if any
        if !result.errors.is_empty() {
            let errors_header = "  // Errors:".to_string();
            writeln!(output, "{errors_header}").ok();
            output_annotated.push((errors_header.clone(), LineStatus::Recomputed));
            state.source_lines.push(errors_header);
            state.line_info.push(ThirLineInfo {
                function_name: func_name.to_string(),
                expr_type: None,
                description: "Type errors".to_string(),
            });

            for error in &result.errors {
                let error_line = format!("  // • {}", error.message());
                writeln!(output, "{error_line}").ok();
                output_annotated.push((error_line.clone(), LineStatus::Recomputed));
                state.source_lines.push(error_line);
                state.line_info.push(ThirLineInfo {
                    function_name: func_name.to_string(),
                    expr_type: None,
                    description: error.message(),
                });
            }
        }

        writeln!(output).ok();
        output_annotated.push((String::new(), LineStatus::Unknown));
        state.source_lines.push(String::new());
        state.line_info.push(ThirLineInfo {
            function_name: String::new(),
            expr_type: None,
            description: String::new(),
        });
    }

    /// Format an expression for interactive mode
    #[allow(clippy::too_many_arguments)]
    fn format_expr_interactive(
        &self,
        expr_id: ExprId,
        body: &ExprBody,
        result: &InferenceResult<'_>,
        output: &mut String,
        output_annotated: &mut Vec<(String, LineStatus)>,
        state: &mut ThirInteractiveState,
        func_name: &str,
        indent: usize,
        status: LineStatus,
    ) {
        let expr = &body.exprs[expr_id];
        let ty = result
            .expr_types
            .get(&expr_id)
            .cloned()
            .unwrap_or(Ty::Unknown);
        let ty_str = ty.to_string();
        let indent_str = "  ".repeat(indent);

        match expr {
            Expr::Block { stmts, tail_expr } => {
                // Don't add extra braces for top-level block, just format contents
                for stmt_id in stmts {
                    self.format_stmt_interactive(
                        *stmt_id,
                        body,
                        result,
                        output,
                        output_annotated,
                        state,
                        func_name,
                        indent,
                        status,
                    );
                }
                if let Some(tail) = tail_expr {
                    self.format_expr_interactive(
                        *tail,
                        body,
                        result,
                        output,
                        output_annotated,
                        state,
                        func_name,
                        indent,
                        status,
                    );
                }
            }
            Expr::Literal(lit) => {
                let lit_str = match lit {
                    baml_hir::Literal::Int(n) => n.to_string(),
                    baml_hir::Literal::Float(s) => s.clone(),
                    baml_hir::Literal::String(s) => format!("\"{s}\""),
                    baml_hir::Literal::Bool(b) => b.to_string(),
                    baml_hir::Literal::Null => "null".to_string(),
                };
                let line = format!("{indent_str}{lit_str}");
                writeln!(output, "{line}").ok();
                output_annotated.push((line.clone(), status));
                state.source_lines.push(line);
                state.line_info.push(ThirLineInfo {
                    function_name: func_name.to_string(),
                    expr_type: Some(ty_str.clone()),
                    description: format!("Literal: {ty_str}"),
                });
            }
            Expr::Path(name) => {
                let line = format!("{indent_str}{name}");
                writeln!(output, "{line}").ok();
                output_annotated.push((line.clone(), status));
                state.source_lines.push(line);
                state.line_info.push(ThirLineInfo {
                    function_name: func_name.to_string(),
                    expr_type: Some(ty_str.clone()),
                    description: format!("Variable '{name}': {ty_str}"),
                });
            }
            Expr::Binary { op, lhs, rhs } => {
                let lhs_str = self.expr_to_inline_string(*lhs, body, result);
                let rhs_str = self.expr_to_inline_string(*rhs, body, result);
                let op_str = format!("{op:?}").to_lowercase();
                let line = format!("{indent_str}{lhs_str} {op_str} {rhs_str}");
                writeln!(output, "{line}").ok();
                output_annotated.push((line.clone(), status));
                state.source_lines.push(line);
                state.line_info.push(ThirLineInfo {
                    function_name: func_name.to_string(),
                    expr_type: Some(ty_str.clone()),
                    description: format!("Binary {op_str}: {ty_str}"),
                });
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond_str = self.expr_to_inline_string(*condition, body, result);
                let if_line = format!("{indent_str}if ({cond_str}) {{");
                writeln!(output, "{if_line}").ok();
                output_annotated.push((if_line.clone(), status));
                state.source_lines.push(if_line);
                state.line_info.push(ThirLineInfo {
                    function_name: func_name.to_string(),
                    expr_type: Some(ty_str.clone()),
                    description: format!("If expression: {ty_str}"),
                });

                self.format_expr_interactive(
                    *then_branch,
                    body,
                    result,
                    output,
                    output_annotated,
                    state,
                    func_name,
                    indent + 1,
                    status,
                );

                if let Some(else_expr) = else_branch {
                    let else_line = format!("{indent_str}}} else {{");
                    writeln!(output, "{else_line}").ok();
                    output_annotated.push((else_line.clone(), status));
                    state.source_lines.push(else_line);
                    state.line_info.push(ThirLineInfo {
                        function_name: func_name.to_string(),
                        expr_type: None,
                        description: "Else branch".to_string(),
                    });

                    self.format_expr_interactive(
                        *else_expr,
                        body,
                        result,
                        output,
                        output_annotated,
                        state,
                        func_name,
                        indent + 1,
                        status,
                    );
                }

                let close_line = format!("{indent_str}}}");
                writeln!(output, "{close_line}").ok();
                output_annotated.push((close_line.clone(), status));
                state.source_lines.push(close_line);
                state.line_info.push(ThirLineInfo {
                    function_name: func_name.to_string(),
                    expr_type: None,
                    description: "End of if".to_string(),
                });
            }
            Expr::Call { callee, args } => {
                let callee_str = self.expr_to_inline_string(*callee, body, result);
                let args_str: Vec<String> = args
                    .iter()
                    .map(|a| self.expr_to_inline_string(*a, body, result))
                    .collect();
                let line = format!("{indent_str}{callee_str}({})", args_str.join(", "));
                writeln!(output, "{line}").ok();
                output_annotated.push((line.clone(), status));
                state.source_lines.push(line);
                state.line_info.push(ThirLineInfo {
                    function_name: func_name.to_string(),
                    expr_type: Some(ty_str.clone()),
                    description: format!("Function call: {ty_str}"),
                });
            }
            Expr::Array { elements } => {
                let elems_str: Vec<String> = elements
                    .iter()
                    .map(|e| self.expr_to_inline_string(*e, body, result))
                    .collect();
                let line = format!("{indent_str}[{}]", elems_str.join(", "));
                writeln!(output, "{line}").ok();
                output_annotated.push((line.clone(), status));
                state.source_lines.push(line);
                state.line_info.push(ThirLineInfo {
                    function_name: func_name.to_string(),
                    expr_type: Some(ty_str.clone()),
                    description: format!("Array literal: {ty_str}"),
                });
            }
            _ => {
                // Fallback for other expressions
                let line = format!("{indent_str}<expr>: {ty_str}");
                writeln!(output, "{line}").ok();
                output_annotated.push((line.clone(), status));
                state.source_lines.push(line);
                state.line_info.push(ThirLineInfo {
                    function_name: func_name.to_string(),
                    expr_type: Some(ty_str.clone()),
                    description: format!("Expression: {ty_str}"),
                });
            }
        }
    }

    /// Format a statement for interactive mode
    #[allow(clippy::too_many_arguments)]
    fn format_stmt_interactive(
        &self,
        stmt_id: StmtId,
        body: &ExprBody,
        result: &InferenceResult<'_>,
        output: &mut String,
        output_annotated: &mut Vec<(String, LineStatus)>,
        state: &mut ThirInteractiveState,
        func_name: &str,
        indent: usize,
        status: LineStatus,
    ) {
        let stmt = &body.stmts[stmt_id];
        let indent_str = "  ".repeat(indent);

        match stmt {
            Stmt::Let {
                pattern,
                type_annotation,
                initializer,
            } => {
                let pat = &body.patterns[*pattern];
                let var_name = match pat {
                    Pattern::Binding(name) => name.to_string(),
                };

                let ty_str = if let Some(type_ref) = type_annotation {
                    let ty = baml_thir::lower_type_ref(&self.db, type_ref);
                    ty.to_string()
                } else if let Some(init) = initializer {
                    result
                        .expr_types
                        .get(init)
                        .map(|t| t.to_string())
                        .unwrap_or_else(|| "?".to_string())
                } else {
                    "?".to_string()
                };

                let init_str = initializer
                    .map(|i| self.expr_to_inline_string(i, body, result))
                    .unwrap_or_default();

                let line = if init_str.is_empty() {
                    format!("{indent_str}let {var_name}: {ty_str};")
                } else {
                    format!("{indent_str}let {var_name}: {ty_str} = {init_str};")
                };

                writeln!(output, "{line}").ok();
                output_annotated.push((line.clone(), status));
                state.source_lines.push(line);
                state.line_info.push(ThirLineInfo {
                    function_name: func_name.to_string(),
                    expr_type: Some(ty_str.clone()),
                    description: format!("Variable '{var_name}': {ty_str}"),
                });
            }
            Stmt::Return(expr) => {
                let ret_str = expr
                    .map(|e| self.expr_to_inline_string(e, body, result))
                    .unwrap_or_default();
                let ty_str = expr
                    .and_then(|e| result.expr_types.get(&e))
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "void".to_string());

                let line = if ret_str.is_empty() {
                    format!("{indent_str}return;")
                } else {
                    format!("{indent_str}return {ret_str};")
                };

                writeln!(output, "{line}").ok();
                output_annotated.push((line.clone(), status));
                state.source_lines.push(line);
                state.line_info.push(ThirLineInfo {
                    function_name: func_name.to_string(),
                    expr_type: Some(ty_str.clone()),
                    description: format!("Return: {ty_str}"),
                });
            }
            Stmt::Expr(expr_id) => {
                self.format_expr_interactive(
                    *expr_id,
                    body,
                    result,
                    output,
                    output_annotated,
                    state,
                    func_name,
                    indent,
                    status,
                );
            }
            Stmt::While {
                condition,
                body: while_body,
            } => {
                let cond_str = self.expr_to_inline_string(*condition, body, result);
                let line = format!("{indent_str}while ({cond_str}) {{");
                writeln!(output, "{line}").ok();
                output_annotated.push((line.clone(), status));
                state.source_lines.push(line);
                state.line_info.push(ThirLineInfo {
                    function_name: func_name.to_string(),
                    expr_type: None,
                    description: "While loop".to_string(),
                });

                self.format_expr_interactive(
                    *while_body,
                    body,
                    result,
                    output,
                    output_annotated,
                    state,
                    func_name,
                    indent + 1,
                    status,
                );

                let close_line = format!("{indent_str}}}");
                writeln!(output, "{close_line}").ok();
                output_annotated.push((close_line.clone(), status));
                state.source_lines.push(close_line);
                state.line_info.push(ThirLineInfo {
                    function_name: func_name.to_string(),
                    expr_type: None,
                    description: "End of while".to_string(),
                });
            }
            Stmt::ForIn {
                pattern,
                iterator,
                body: for_body,
            } => {
                let pat = &body.patterns[*pattern];
                let var_name = match pat {
                    Pattern::Binding(name) => name.to_string(),
                };
                let iter_str = self.expr_to_inline_string(*iterator, body, result);
                let line = format!("{indent_str}for (let {var_name} in {iter_str}) {{");
                writeln!(output, "{line}").ok();
                output_annotated.push((line.clone(), status));
                state.source_lines.push(line);
                state.line_info.push(ThirLineInfo {
                    function_name: func_name.to_string(),
                    expr_type: None,
                    description: format!("For-in loop over '{var_name}'"),
                });

                self.format_expr_interactive(
                    *for_body,
                    body,
                    result,
                    output,
                    output_annotated,
                    state,
                    func_name,
                    indent + 1,
                    status,
                );

                let close_line = format!("{indent_str}}}");
                writeln!(output, "{close_line}").ok();
                output_annotated.push((close_line.clone(), status));
                state.source_lines.push(close_line);
                state.line_info.push(ThirLineInfo {
                    function_name: func_name.to_string(),
                    expr_type: None,
                    description: "End of for".to_string(),
                });
            }
            _ => {
                let line = format!("{indent_str}<stmt>;");
                writeln!(output, "{line}").ok();
                output_annotated.push((line.clone(), status));
                state.source_lines.push(line);
                state.line_info.push(ThirLineInfo {
                    function_name: func_name.to_string(),
                    expr_type: None,
                    description: "Statement".to_string(),
                });
            }
        }
    }

    /// Convert an expression to inline string representation
    fn expr_to_inline_string(
        &self,
        expr_id: ExprId,
        body: &ExprBody,
        result: &InferenceResult<'_>,
    ) -> String {
        let expr = &body.exprs[expr_id];

        match expr {
            Expr::Literal(lit) => match lit {
                baml_hir::Literal::Int(n) => n.to_string(),
                baml_hir::Literal::Float(s) => s.clone(),
                baml_hir::Literal::String(s) => format!("\"{s}\""),
                baml_hir::Literal::Bool(b) => b.to_string(),
                baml_hir::Literal::Null => "null".to_string(),
            },
            Expr::Path(name) => name.to_string(),
            Expr::Binary { op, lhs, rhs } => {
                let lhs_str = self.expr_to_inline_string(*lhs, body, result);
                let rhs_str = self.expr_to_inline_string(*rhs, body, result);
                let op_str = match op {
                    baml_hir::BinaryOp::Add => "+",
                    baml_hir::BinaryOp::Sub => "-",
                    baml_hir::BinaryOp::Mul => "*",
                    baml_hir::BinaryOp::Div => "/",
                    baml_hir::BinaryOp::Mod => "%",
                    baml_hir::BinaryOp::Eq => "==",
                    baml_hir::BinaryOp::Ne => "!=",
                    baml_hir::BinaryOp::Lt => "<",
                    baml_hir::BinaryOp::Le => "<=",
                    baml_hir::BinaryOp::Gt => ">",
                    baml_hir::BinaryOp::Ge => ">=",
                    baml_hir::BinaryOp::And => "&&",
                    baml_hir::BinaryOp::Or => "||",
                    baml_hir::BinaryOp::BitAnd => "&",
                    baml_hir::BinaryOp::BitOr => "|",
                    baml_hir::BinaryOp::BitXor => "^",
                    baml_hir::BinaryOp::Shl => "<<",
                    baml_hir::BinaryOp::Shr => ">>",
                };
                format!("{lhs_str} {op_str} {rhs_str}")
            }
            Expr::Unary { op, expr: inner } => {
                let inner_str = self.expr_to_inline_string(*inner, body, result);
                let op_str = match op {
                    baml_hir::UnaryOp::Not => "!",
                    baml_hir::UnaryOp::Neg => "-",
                };
                format!("{op_str}{inner_str}")
            }
            Expr::Call { callee, args } => {
                let callee_str = self.expr_to_inline_string(*callee, body, result);
                let args_str: Vec<String> = args
                    .iter()
                    .map(|a| self.expr_to_inline_string(*a, body, result))
                    .collect();
                format!("{callee_str}({})", args_str.join(", "))
            }
            Expr::FieldAccess { base, field } => {
                let base_str = self.expr_to_inline_string(*base, body, result);
                format!("{base_str}.{field}")
            }
            Expr::Index { base, index } => {
                let base_str = self.expr_to_inline_string(*base, body, result);
                let index_str = self.expr_to_inline_string(*index, body, result);
                format!("{base_str}[{index_str}]")
            }
            Expr::Array { elements } => {
                let elems: Vec<String> = elements
                    .iter()
                    .map(|e| self.expr_to_inline_string(*e, body, result))
                    .collect();
                format!("[{}]", elems.join(", "))
            }
            _ => "<expr>".to_string(),
        }
    }

    fn run_diagnostics(&mut self) {
        // Diagnostics not yet implemented as a tracked function
        let output = "Diagnostics not yet implemented".to_string();

        let output_annotated: Vec<_> = output
            .lines()
            .map(|line| (line.to_string(), LineStatus::Unknown))
            .collect();

        self.phase_outputs
            .insert(CompilerPhase::Diagnostics, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Diagnostics, output_annotated);
    }

    fn run_codegen(&mut self) {
        let bytecode = baml_codegen::generate_project_bytecode(&self.db, self.project_root);
        let output = format!("{bytecode:#?}");

        let file_recomputed = self.was_query_recomputed("generate_project_bytecode(");
        let output_annotated: Vec<_> = output
            .lines()
            .map(|line| {
                (
                    line.to_string(),
                    if file_recomputed {
                        LineStatus::Recomputed
                    } else {
                        LineStatus::Cached
                    },
                )
            })
            .collect();

        self.phase_outputs.insert(CompilerPhase::Codegen, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Codegen, output_annotated);
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

    fn was_query_recomputed(&self, query_pattern: &str) -> bool {
        self.recomputed_queries
            .lock()
            .unwrap()
            .iter()
            .any(|q| q.contains(query_pattern))
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
    use baml_syntax::ast::*;

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

fn format_function(func: &baml_syntax::ast::FunctionDef, output: &mut String, indent: usize) {
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

fn format_parameter(param: &baml_syntax::ast::Parameter, output: &mut String, indent: usize) {
    write_indent(output, indent);
    writeln!(output, "PARAM").ok();

    // Parameter name
    if let Some(name_token) = param
        .syntax()
        .children_with_tokens()
        .filter_map(|n| n.into_token())
        .find(|t| t.kind() == baml_syntax::SyntaxKind::WORD)
    {
        write_indent(output, indent + 1);
        writeln!(output, "NAME {}", name_token.text()).ok();
    }

    // Parameter type
    if let Some(ty) = param
        .syntax()
        .children()
        .find_map(baml_syntax::ast::TypeExpr::cast)
    {
        write_indent(output, indent + 1);
        writeln!(output, "TYPE {}", ty.syntax().text()).ok();
    }
}

fn format_expr_function_body(
    body: &baml_syntax::ast::ExprFunctionBody,
    output: &mut String,
    indent: usize,
) {
    use baml_syntax::ast::*;

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
    body: &baml_syntax::ast::LlmFunctionBody,
    output: &mut String,
    indent: usize,
) {
    write_indent(output, indent);
    writeln!(output, "LLM_BODY").ok();

    // Show config items
    for config_item in body
        .syntax()
        .children()
        .filter_map(baml_syntax::ast::ConfigItem::cast)
    {
        format_config_item(&config_item, output, indent + 1);
    }
}

fn format_config_item(item: &baml_syntax::ast::ConfigItem, output: &mut String, indent: usize) {
    write_indent(output, indent);
    let text = item.syntax().text().to_string();
    // Truncate long config values
    if text.len() > 60 {
        writeln!(output, "CONFIG {}...", &text[..60]).ok();
    } else {
        writeln!(output, "CONFIG {}", text).ok();
    }
}

fn format_block_expr(block: &baml_syntax::ast::BlockExpr, output: &mut String, indent: usize) {
    use baml_syntax::ast::*;

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

fn format_let_stmt(stmt: &baml_syntax::ast::LetStmt, output: &mut String, indent: usize) {
    use baml_syntax::ast::*;

    write_indent(output, indent);
    writeln!(output, "STMT_LET").ok();

    // Find the identifier name
    if let Some(name_token) = stmt
        .syntax()
        .children_with_tokens()
        .filter_map(|n| n.into_token())
        .find(|t| t.kind() == baml_syntax::SyntaxKind::WORD)
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

fn format_if_expr(if_expr: &baml_syntax::ast::IfExpr, output: &mut String, indent: usize) {
    write_indent(output, indent);
    writeln!(output, "EXPR_IF").ok();

    // Condition
    write_indent(output, indent + 1);
    writeln!(output, "CONDITION").ok();
    if let Some(cond) = if_expr
        .syntax()
        .children()
        .find_map(baml_syntax::ast::Expr::cast)
    {
        format_expr(&cond, output, indent + 2);
    }

    // Then branch
    write_indent(output, indent + 1);
    writeln!(output, "THEN").ok();
    if let Some(then_block) = if_expr
        .syntax()
        .children()
        .filter_map(baml_syntax::ast::BlockExpr::cast)
        .next()
    {
        format_block_expr(&then_block, output, indent + 2);
    }
}

fn format_expr(expr: &baml_syntax::ast::Expr, output: &mut String, indent: usize) {
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

fn format_class(class: &baml_syntax::ast::ClassDef, output: &mut String, indent: usize) {
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

fn format_field(field: &baml_syntax::ast::Field, output: &mut String, indent: usize) {
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

fn format_enum(enum_def: &baml_syntax::ast::EnumDef, output: &mut String, indent: usize) {
    write_indent(output, indent);
    writeln!(output, "ENUM").ok();

    // Enum name
    if let Some(name) = enum_def
        .syntax()
        .children_with_tokens()
        .filter_map(|n| n.into_token())
        .filter(|t| t.kind() == baml_syntax::SyntaxKind::WORD)
        .nth(1)
    {
        write_indent(output, indent + 1);
        writeln!(output, "NAME {}", name.text()).ok();
    }
}

fn format_client(client: &baml_syntax::ast::ClientDef, output: &mut String, indent: usize) {
    write_indent(output, indent);
    writeln!(output, "CLIENT").ok();

    // Client name
    if let Some(name) = client
        .syntax()
        .children_with_tokens()
        .filter_map(|n| n.into_token())
        .filter(|t| t.kind() == baml_syntax::SyntaxKind::WORD)
        .nth(1)
    {
        write_indent(output, indent + 1);
        writeln!(output, "NAME {}", name.text()).ok();
    }
}

fn format_test(test: &baml_syntax::ast::TestDef, output: &mut String, indent: usize) {
    write_indent(output, indent);
    writeln!(output, "TEST").ok();

    // Test name
    if let Some(name) = test
        .syntax()
        .children_with_tokens()
        .filter_map(|n| n.into_token())
        .filter(|t| t.kind() == baml_syntax::SyntaxKind::WORD)
        .nth(1)
    {
        write_indent(output, indent + 1);
        writeln!(output, "NAME {}", name.text()).ok();
    }
}

fn format_retry_policy(
    policy: &baml_syntax::ast::RetryPolicyDef,
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
        .filter(|t| t.kind() == baml_syntax::SyntaxKind::WORD)
        .nth(1)
    {
        write_indent(output, indent + 1);
        writeln!(output, "NAME {}", name.text()).ok();
    }
}

fn format_template_string(
    template: &baml_syntax::ast::TemplateStringDef,
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
        .filter(|t| t.kind() == baml_syntax::SyntaxKind::WORD)
        .nth(1)
    {
        write_indent(output, indent + 1);
        writeln!(output, "NAME {}", name.text()).ok();
    }
}

fn format_type_alias(alias: &baml_syntax::ast::TypeAliasDef, output: &mut String, indent: usize) {
    write_indent(output, indent);
    writeln!(output, "TYPE_ALIAS").ok();

    // Alias name
    if let Some(name) = alias
        .syntax()
        .children_with_tokens()
        .filter_map(|n| n.into_token())
        .filter(|t| t.kind() == baml_syntax::SyntaxKind::WORD)
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
