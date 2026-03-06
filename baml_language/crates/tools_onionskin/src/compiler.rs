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

/// Format compiler2 AST `TypeExpr` for HIR2 display.
fn hir2_type_expr_to_string(ty: &baml_compiler2_ast::TypeExpr) -> String {
    use baml_compiler2_ast::TypeExpr;
    match ty {
        TypeExpr::Path(segments) => segments
            .iter()
            .map(|n| n.as_str())
            .collect::<Vec<_>>()
            .join("."),
        TypeExpr::Int => "int".into(),
        TypeExpr::Float => "float".into(),
        TypeExpr::String => "string".into(),
        TypeExpr::Bool => "bool".into(),
        TypeExpr::Null => "null".into(),
        TypeExpr::Never => "never".into(),
        TypeExpr::Media(k) => format!("{:?}", k).to_lowercase(),
        TypeExpr::Optional(inner) => format!("{}?", hir2_type_expr_to_string(inner)),
        TypeExpr::List(inner) => format!("{}[]", hir2_type_expr_to_string(inner)),
        TypeExpr::Map { key, value } => format!(
            "map<{}, {}>",
            hir2_type_expr_to_string(key),
            hir2_type_expr_to_string(value)
        ),
        TypeExpr::Union(members) => members
            .iter()
            .map(hir2_type_expr_to_string)
            .collect::<Vec<_>>()
            .join(" | "),
        TypeExpr::Literal(lit) => lit.to_string(),
        TypeExpr::Function { params, ret } => {
            let ps: Vec<String> = params
                .iter()
                .map(|p| {
                    p.name
                        .as_ref()
                        .map(|n| format!("{}: {}", n.as_str(), hir2_type_expr_to_string(&p.ty)))
                        .unwrap_or_else(|| hir2_type_expr_to_string(&p.ty))
                })
                .collect();
            format!("({}) -> {}", ps.join(", "), hir2_type_expr_to_string(ret))
        }
        TypeExpr::BuiltinUnknown => "unknown".into(),
        TypeExpr::Type => "type".into(),
        TypeExpr::Rust => "$rust_type".into(),
        TypeExpr::Error => "error".into(),
        TypeExpr::Unknown => "?".into(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum CompilerPhase {
    Lexer,
    Parser,
    Ast,
    Hir,
    Hir2,
    Tir2,
    Thir,
    TypedIr,
    ControlFlow,
    Mir,
    Diagnostics,
    Codegen,
    VmRunner,
    Metrics,
    Formatter,
}

impl CompilerPhase {
    pub(crate) const ALL: &'static [CompilerPhase] = &[
        CompilerPhase::Lexer,
        CompilerPhase::Parser,
        CompilerPhase::Ast,
        CompilerPhase::Hir,
        CompilerPhase::Hir2,
        CompilerPhase::Tir2,
        CompilerPhase::Thir,
        CompilerPhase::TypedIr,
        CompilerPhase::ControlFlow,
        CompilerPhase::Mir,
        CompilerPhase::Diagnostics,
        CompilerPhase::Codegen,
        CompilerPhase::VmRunner,
        CompilerPhase::Metrics,
        CompilerPhase::Formatter,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            CompilerPhase::Lexer => "Lexer (Tokens)",
            CompilerPhase::Parser => "Parser (CST)",
            CompilerPhase::Ast => "AST (Typed Nodes)",
            CompilerPhase::Hir => "HIR (High-level IR)",
            CompilerPhase::Hir2 => "HIR2 (Scope Tree)",
            CompilerPhase::Tir2 => "TIR2 (Type Inference)",
            CompilerPhase::Thir => "THIR (Typed HIR)",
            CompilerPhase::TypedIr => "TypedIR (Expr-only)",
            CompilerPhase::ControlFlow => "Control Flow",
            CompilerPhase::Mir => "MIR (CFG)",
            CompilerPhase::Diagnostics => "Diagnostics",
            CompilerPhase::Codegen => "Codegen (Bytecode)",
            CompilerPhase::VmRunner => "VM Runner",
            CompilerPhase::Metrics => "Metrics (Incremental)",
            CompilerPhase::Formatter => "Formatter",
        }
    }

    pub(crate) fn next(self) -> CompilerPhase {
        match self {
            CompilerPhase::Lexer => CompilerPhase::Parser,
            CompilerPhase::Parser => CompilerPhase::Ast,
            CompilerPhase::Ast => CompilerPhase::Hir,
            CompilerPhase::Hir => CompilerPhase::Hir2,
            CompilerPhase::Hir2 => CompilerPhase::Tir2,
            CompilerPhase::Tir2 => CompilerPhase::Thir,
            CompilerPhase::Thir => CompilerPhase::TypedIr,
            CompilerPhase::TypedIr => CompilerPhase::ControlFlow,
            CompilerPhase::ControlFlow => CompilerPhase::Mir,
            CompilerPhase::Mir => CompilerPhase::Diagnostics,
            CompilerPhase::Diagnostics => CompilerPhase::Codegen,
            CompilerPhase::Codegen => CompilerPhase::VmRunner,
            CompilerPhase::VmRunner => CompilerPhase::Metrics,
            CompilerPhase::Metrics => CompilerPhase::Formatter,
            CompilerPhase::Formatter => CompilerPhase::Lexer,
        }
    }

    pub(crate) fn prev(self) -> CompilerPhase {
        match self {
            CompilerPhase::Lexer => CompilerPhase::Formatter,
            CompilerPhase::Parser => CompilerPhase::Lexer,
            CompilerPhase::Ast => CompilerPhase::Parser,
            CompilerPhase::Hir => CompilerPhase::Ast,
            CompilerPhase::Hir2 => CompilerPhase::Hir,
            CompilerPhase::Tir2 => CompilerPhase::Hir2,
            CompilerPhase::Thir => CompilerPhase::Tir2,
            CompilerPhase::TypedIr => CompilerPhase::Thir,
            CompilerPhase::ControlFlow => CompilerPhase::TypedIr,
            CompilerPhase::Mir => CompilerPhase::ControlFlow,
            CompilerPhase::Diagnostics => CompilerPhase::Mir,
            CompilerPhase::Codegen => CompilerPhase::Diagnostics,
            CompilerPhase::VmRunner => CompilerPhase::Codegen,
            CompilerPhase::Metrics => CompilerPhase::VmRunner,
            CompilerPhase::Formatter => CompilerPhase::Metrics,
        }
    }

    /// Returns a short name suitable for CLI arguments
    pub(crate) fn cli_name(self) -> &'static str {
        match self {
            CompilerPhase::Lexer => "lexer",
            CompilerPhase::Parser => "parser",
            CompilerPhase::Ast => "ast",
            CompilerPhase::Hir => "hir",
            CompilerPhase::Hir2 => "hir2",
            CompilerPhase::Tir2 => "tir2",
            CompilerPhase::Thir => "thir",
            CompilerPhase::TypedIr => "typedir",
            CompilerPhase::ControlFlow => "controlflow",
            CompilerPhase::Mir => "mir",
            CompilerPhase::Diagnostics => "diagnostics",
            CompilerPhase::Codegen => "codegen",
            CompilerPhase::VmRunner => "vmrunner",
            CompilerPhase::Metrics => "metrics",
            CompilerPhase::Formatter => "formatter",
        }
    }
}

pub(crate) struct CompilerRunner {
    db: ProjectDatabase,
    project_root: baml_workspace::Project,
    is_directory: bool,
    /// Source files currently in the database (path -> `SourceFile`)
    source_files: HashMap<PathBuf, SourceFile>,
    /// Builtin BAML files (loaded once, always included in project file list)
    builtin_files: Vec<SourceFile>,
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
    // HIR2 column browser data + navigation state
    hir2_column_data: Hir2ColumnData,
    hir2_column_state: Hir2ColumnState,
    // TIR2 column browser data + navigation state
    tir2_column_data: Tir2ColumnData,
    tir2_column_state: Tir2ColumnState,
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

// ── HIR2 Column Browser ──────────────────────────────────────────────────────

/// Structured data for the HIR2 column browser (macOS Finder-style columns).
#[derive(Debug, Clone, Default)]
pub(crate) struct Hir2ColumnData {
    pub packages: Vec<Hir2Package>,
}

#[derive(Debug, Clone)]
pub(crate) struct Hir2Package {
    pub name: String,
    pub namespace: String,
    pub files: Vec<Hir2FileEntry>,
    pub namespace_summary: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct Hir2FileEntry {
    pub name: String,
    pub summary: String,
    pub items: Vec<Hir2ItemEntry>,
    pub detail_lines: Vec<String>,
    pub error_count: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct Hir2ItemEntry {
    pub name: String,
    pub kind: String,
    pub signature: String,
    pub detail_lines: Vec<String>,
    pub has_errors: bool,
}

/// Navigation state for the HIR2 column browser.
#[derive(Debug, Clone, Default)]
pub(crate) struct Hir2ColumnState {
    /// Which column has focus (0=packages, 1=files, 2=items)
    pub active_column: usize,
    /// Selected index in each column [packages, files, items]
    pub selected: [usize; 3],
    /// Scroll offset for the detail pane
    pub detail_scroll: usize,
}

// ── Rich detail line model ──────────────────────────────────────────────────

/// A span within a detail line, carrying styling intent.
#[derive(Debug, Clone)]
pub(crate) enum DetailSpan {
    /// Normal code text (default foreground).
    Code(String),
    /// Inferred type annotation (rendered dim/gray).
    TypeAnnotation(String),
    /// Error text (rendered red).
    Error(String),
}

/// A single line in the detail panel, composed of styled spans.
pub(crate) type DetailLine = Vec<DetailSpan>;

/// Helper: wrap a plain string as a single `Code` span line.
pub(crate) fn plain(s: impl Into<String>) -> DetailLine {
    vec![DetailSpan::Code(s.into())]
}

/// Flatten a `DetailLine` into a plain string (for text-based searching).
pub(crate) fn detail_line_text(line: &DetailLine) -> String {
    line.iter()
        .map(|span| match span {
            DetailSpan::Code(s) | DetailSpan::TypeAnnotation(s) | DetailSpan::Error(s) => {
                s.as_str()
            }
        })
        .collect()
}

// ── TIR2 Column Browser ─────────────────────────────────────────────────────

/// Structured data for the TIR2 column browser (type-checked IR).
#[derive(Debug, Clone, Default)]
pub(crate) struct Tir2ColumnData {
    pub packages: Vec<Tir2Package>,
}

#[derive(Debug, Clone)]
pub(crate) struct Tir2Package {
    pub name: String,
    pub namespace: String,
    pub files: Vec<Tir2FileEntry>,
    pub namespace_summary: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct Tir2FileEntry {
    pub name: String,
    pub summary: String,
    pub items: Vec<Tir2ItemEntry>,
    pub detail_lines: Vec<String>,
    pub error_count: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct Tir2ItemEntry {
    pub name: String,
    pub kind: String,
    pub signature: String,
    pub detail_lines: Vec<DetailLine>,
    pub has_errors: bool,
}

/// Navigation state for the TIR2 column browser.
#[derive(Debug, Clone, Default)]
pub(crate) struct Tir2ColumnState {
    pub active_column: usize,
    pub selected: [usize; 3],
    pub detail_scroll: usize,
}

// ── TIR2 expression rendering helpers ───────────────────────────────────────

fn binop_sym(op: &baml_compiler2_ast::BinaryOp) -> &'static str {
    use baml_compiler2_ast::BinaryOp;
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "%",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
        BinaryOp::Shl => "<<",
        BinaryOp::Shr => ">>",
        BinaryOp::Instanceof => "instanceof",
    }
}

fn assignop_sym(op: &baml_compiler2_ast::AssignOp) -> &'static str {
    use baml_compiler2_ast::AssignOp;
    match op {
        AssignOp::Add => "+=",
        AssignOp::Sub => "-=",
        AssignOp::Mul => "*=",
        AssignOp::Div => "/=",
        AssignOp::Mod => "%=",
        AssignOp::BitAnd => "&=",
        AssignOp::BitOr => "|=",
        AssignOp::BitXor => "^=",
        AssignOp::Shl => "<<=",
        AssignOp::Shr => ">>=",
    }
}

fn catch_clause_kind_sym(kind: baml_compiler2_ast::CatchClauseKind) -> &'static str {
    match kind {
        baml_compiler2_ast::CatchClauseKind::Catch => "catch",
        baml_compiler2_ast::CatchClauseKind::CatchAll => "catch_all",
        baml_compiler2_ast::CatchClauseKind::CatchAllPanics => "catch_all_panics",
    }
}

/// Render an expression as rich `DetailSpan`s with inline type annotations on
/// every sub-expression. E.g. `Add1(Add2(x: int): int): int`.
fn expr_desc_spans<'db>(
    expr_id: baml_compiler2_ast::ExprId,
    body: &baml_compiler2_ast::ExprBody,
    inference: &baml_compiler2_tir::inference::ScopeInference<'db>,
) -> Vec<DetailSpan> {
    use baml_compiler2_ast::{Expr, Literal, UnaryOp};

    let mut spans = Vec::new();
    let expr = &body.exprs[expr_id];

    match expr {
        Expr::Literal(lit) => {
            let s = match lit {
                Literal::String(s) => {
                    if s.len() > 40 {
                        format!("\"{}...\"", &s[..37])
                    } else {
                        format!("\"{s}\"")
                    }
                }
                Literal::Int(i) => i.to_string(),
                Literal::Float(f) => f.clone(),
                Literal::Bool(b) => b.to_string(),
            };
            spans.push(DetailSpan::Code(s));
        }
        Expr::Null => {
            spans.push(DetailSpan::Code("null".into()));
        }
        Expr::Path(segments) => {
            spans.push(DetailSpan::Code(
                segments
                    .iter()
                    .map(|n| n.as_str())
                    .collect::<Vec<_>>()
                    .join("."),
            ));
        }
        Expr::Binary { op, lhs, rhs } => {
            spans.extend(expr_desc_spans(*lhs, body, inference));
            spans.push(DetailSpan::Code(format!(" {} ", binop_sym(op))));
            spans.extend(expr_desc_spans(*rhs, body, inference));
        }
        Expr::Unary { op, expr: inner } => {
            let sym = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
            };
            spans.push(DetailSpan::Code(sym.into()));
            spans.extend(expr_desc_spans(*inner, body, inference));
        }
        Expr::Call { callee, args } => {
            spans.extend(expr_desc_spans(*callee, body, inference));
            spans.push(DetailSpan::Code("(".into()));
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    spans.push(DetailSpan::Code(", ".into()));
                }
                spans.extend(expr_desc_spans(*arg, body, inference));
            }
            spans.push(DetailSpan::Code(")".into()));
        }
        Expr::Object {
            type_name, fields, ..
        } => {
            let tn = type_name.as_ref().map(|n| n.as_str()).unwrap_or("_");
            spans.push(DetailSpan::Code(format!("{tn} {{ ")));
            for (i, (name, val)) in fields.iter().enumerate() {
                if i > 0 {
                    spans.push(DetailSpan::Code(", ".into()));
                }
                spans.push(DetailSpan::Code(format!("{}: ", name)));
                spans.extend(expr_desc_spans(*val, body, inference));
            }
            spans.push(DetailSpan::Code(" }".into()));
        }
        Expr::Array { elements } => {
            spans.push(DetailSpan::Code("[".into()));
            for (i, e) in elements.iter().enumerate() {
                if i > 0 {
                    spans.push(DetailSpan::Code(", ".into()));
                }
                spans.extend(expr_desc_spans(*e, body, inference));
            }
            spans.push(DetailSpan::Code("]".into()));
        }
        Expr::Map { entries } => {
            spans.push(DetailSpan::Code("{".into()));
            for (i, (k, v)) in entries.iter().enumerate() {
                if i > 0 {
                    spans.push(DetailSpan::Code(", ".into()));
                }
                spans.extend(expr_desc_spans(*k, body, inference));
                spans.push(DetailSpan::Code(": ".into()));
                spans.extend(expr_desc_spans(*v, body, inference));
            }
            spans.push(DetailSpan::Code("}".into()));
        }
        Expr::Block { stmts, tail_expr } => {
            let tail = if tail_expr.is_some() { " + tail" } else { "" };
            spans.push(DetailSpan::Code(format!(
                "{{ {} stmts{tail} }}",
                stmts.len()
            )));
        }
        Expr::FieldAccess { base, field } => {
            spans.extend(expr_desc_spans(*base, body, inference));
            spans.push(DetailSpan::Code(format!(".{field}")));
        }
        Expr::Index { base, index } => {
            spans.extend(expr_desc_spans(*base, body, inference));
            spans.push(DetailSpan::Code("[".into()));
            spans.extend(expr_desc_spans(*index, body, inference));
            spans.push(DetailSpan::Code("]".into()));
        }
        Expr::If { condition, .. } => {
            spans.push(DetailSpan::Code("if (".into()));
            spans.extend(expr_desc_spans(*condition, body, inference));
            spans.push(DetailSpan::Code(") { ... }".into()));
        }
        Expr::Match { scrutinee, .. } => {
            spans.push(DetailSpan::Code("match (".into()));
            spans.extend(expr_desc_spans(*scrutinee, body, inference));
            spans.push(DetailSpan::Code(") { ... }".into()));
        }
        Expr::Catch { base, clauses } => {
            spans.extend(expr_desc_spans(*base, body, inference));
            for clause in clauses {
                spans.push(DetailSpan::Code(format!(
                    " {}(...)",
                    catch_clause_kind_sym(clause.kind)
                )));
            }
        }
        Expr::Throw { value } => {
            spans.push(DetailSpan::Code("throw ".into()));
            spans.extend(expr_desc_spans(*value, body, inference));
        }
        Expr::Missing => {
            spans.push(DetailSpan::Code("<missing>".into()));
        }
    }

    if let Some(ty) = inference.expression_type(expr_id) {
        spans.push(DetailSpan::TypeAnnotation(format!(": {ty}")));
    }

    spans
}

fn pat_desc(pat_id: baml_compiler2_ast::PatId, body: &baml_compiler2_ast::ExprBody) -> String {
    use baml_compiler2_ast::Pattern;
    let pat = &body.patterns[pat_id];
    match pat {
        Pattern::Binding(n) => n.to_string(),
        Pattern::TypedBinding { name, ty } => {
            format!("{name}: {}", hir2_type_expr_to_string(ty))
        }
        Pattern::Literal(lit) => lit.to_string(),
        Pattern::Null => "null".into(),
        Pattern::EnumVariant { enum_name, variant } => format!("{enum_name}.{variant}"),
        Pattern::Union(pats) => pats
            .iter()
            .map(|p| pat_desc(*p, body))
            .collect::<Vec<_>>()
            .join(" | "),
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
            builtin_files: Vec::new(),
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
            hir2_column_data: Hir2ColumnData::default(),
            hir2_column_state: Hir2ColumnState::default(),
            tir2_column_data: Tir2ColumnData::default(),
            tir2_column_state: Tir2ColumnState::default(),
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

        // Capture builtin files (loaded by set_project_root) on first compilation
        if self.builtin_files.is_empty() {
            self.builtin_files = self.project_root.files(&self.db).to_vec();
        }

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

        // Update project root with user files + builtins for proper Salsa tracking
        let mut file_list: Vec<_> = self.source_files.values().copied().collect();
        file_list.extend(&self.builtin_files);
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
            CompilerPhase::Hir2,
            CompilerPhase::Tir2,
            CompilerPhase::Thir,
            CompilerPhase::TypedIr,
            CompilerPhase::ControlFlow,
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
        self.run_single_phase(CompilerPhase::Formatter);
    }

    pub(crate) fn run_single_phase(&mut self, phase: CompilerPhase) {
        match phase {
            CompilerPhase::Lexer => self.run_lexer(),
            CompilerPhase::Parser => self.run_parser(),
            CompilerPhase::Ast => self.run_ast(),
            CompilerPhase::Hir => self.run_hir(),
            CompilerPhase::Hir2 => self.run_hir2(),
            CompilerPhase::Tir2 => self.run_tir2(),
            CompilerPhase::Thir => self.run_thir(),
            CompilerPhase::TypedIr => self.run_typed_ir(),
            CompilerPhase::ControlFlow => self.run_control_flow(),
            CompilerPhase::Mir => self.run_mir(),
            CompilerPhase::Diagnostics => self.run_diagnostics(),
            CompilerPhase::Codegen => self.run_codegen(),
            CompilerPhase::VmRunner => self.run_vm_runner(),
            CompilerPhase::Metrics => self.run_metrics(),
            CompilerPhase::Formatter => self.run_formatter(),
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
                        ItemId::TemplateString(ts_loc) => {
                            let ts = &item_tree[ts_loc.id(&self.db)];
                            let line = format!("template_string {}", ts.name);
                            writeln!(output, "{line}").ok();
                            output_annotated.push((line, status));
                            writeln!(output).ok();
                            output_annotated.push((String::new(), LineStatus::Unknown));
                        }
                        ItemId::RetryPolicy(rp_loc) => {
                            let rp = &item_tree[rp_loc.id(&self.db)];
                            let line = format!("retry_policy {}", rp.name);
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

    fn run_hir2(&mut self) {
        use baml_compiler2_ast::FunctionBodyDef;
        use baml_compiler2_hir::{
            file_package::file_package,
            namespace::{NamespaceId, namespace_items},
            scope::ScopeKind,
        };

        let mut output = String::new();
        let mut output_annotated = Vec::new();

        // Sort files alphabetically
        let mut sorted_files: Vec<_> = self.source_files.iter().collect();
        sorted_files.sort_by_key(|(path, _)| path.as_path());

        // ── Group files by (package, namespace) ─────────────────────────
        struct PkgGroup {
            package: String,
            namespace: String,
            ns_id_key: (String, Vec<String>),
            files: Vec<(std::path::PathBuf, baml_base::SourceFile)>,
        }
        let mut groups: Vec<PkgGroup> = Vec::new();
        for (path, source_file) in &sorted_files {
            let pkg_info = file_package(&self.db, **source_file);
            let pkg_name = pkg_info.package.as_str().to_string();
            let ns_path: Vec<String> = pkg_info
                .namespace_path
                .iter()
                .map(|n| n.as_str().to_string())
                .collect();
            let key = (pkg_name.clone(), ns_path.clone());
            let ns_str = if ns_path.is_empty() {
                "[]".to_string()
            } else {
                format!("[\"{}\"]", ns_path.join("\", \""))
            };
            if let Some(group) = groups.iter_mut().find(|g| g.ns_id_key == key) {
                group.files.push(((*path).clone(), **source_file));
            } else {
                groups.push(PkgGroup {
                    package: pkg_name,
                    namespace: ns_str,
                    ns_id_key: key,
                    files: vec![((*path).clone(), **source_file)],
                });
            }
        }

        // ── Build column data + flat text ───────────────────────────────
        let mut column_data = Hir2ColumnData {
            packages: Vec::new(),
        };

        for group in &groups {
            let pkg_header = format!("Package: {}, Namespace: {}", group.package, group.namespace);
            writeln!(output, "{pkg_header}").ok();
            output_annotated.push((pkg_header, LineStatus::Unknown));

            // Fetch merged namespace items once per package
            let first_file = group.files[0].1;
            let pkg_info = file_package(&self.db, first_file);
            let ns_id = NamespaceId::new(
                &self.db,
                pkg_info.package.clone(),
                pkg_info.namespace_path.clone(),
            );
            let ns_items = namespace_items(&self.db, ns_id);

            // Build namespace summary for column view
            let mut ns_summary_lines: Vec<String> = Vec::new();
            let mut sorted_types: Vec<_> = ns_items.types.iter().collect();
            sorted_types.sort_by_key(|(name, _)| name.as_str().to_string());
            let mut sorted_values: Vec<_> = ns_items.values.iter().collect();
            sorted_values.sort_by_key(|(name, _)| name.as_str().to_string());
            ns_summary_lines.push(format!(
                "{} types, {} values",
                ns_items.types.len(),
                ns_items.values.len()
            ));
            ns_summary_lines.push(String::new());
            if !ns_items.types.is_empty() {
                ns_summary_lines.push("Types:".to_string());
                for (name, def) in &sorted_types {
                    ns_summary_lines.push(format!("  {} {}", def.kind_name(), name));
                }
                ns_summary_lines.push(String::new());
            }
            if !ns_items.values.is_empty() {
                ns_summary_lines.push("Values:".to_string());
                for (name, def) in &sorted_values {
                    ns_summary_lines.push(format!("  {} {}", def.kind_name(), name));
                }
            }
            // Conflicts
            for conflict in ns_items.conflicts() {
                ns_summary_lines.push(String::new());
                ns_summary_lines.push(format!(
                    "!! `{}` defined {} times across files",
                    conflict.name,
                    conflict.entries.len()
                ));
                for (i, entry) in conflict.entries.iter().enumerate() {
                    let fpath = entry.definition.file(&self.db).path(&self.db);
                    let fname = fpath
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| fpath.display().to_string());
                    ns_summary_lines.push(format!(
                        "  {}. {} in {}",
                        i + 1,
                        entry.definition.kind_name(),
                        fname
                    ));
                }
            }

            let mut col_pkg = Hir2Package {
                name: group.package.clone(),
                namespace: group.namespace.clone(),
                files: Vec::new(),
                namespace_summary: ns_summary_lines,
            };

            // ── Per-file rendering ──────────────────────────────────
            for (path, source_file) in &group.files {
                let file_path = path.display().to_string();
                let file_name = path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| file_path.clone());
                let file_recomputed = self.modified_files.contains(path);
                let status = if file_recomputed {
                    LineStatus::Recomputed
                } else {
                    LineStatus::Cached
                };

                let index = baml_compiler2_hir::file_semantic_index(&self.db, *source_file);
                let items = &index.item_tree;

                // Build file summary string
                let mut parts = Vec::new();
                if items.functions.len() > 0 {
                    parts.push(format!("{} fn", items.functions.len()));
                }
                if items.classes.len() > 0 {
                    parts.push(format!("{} class", items.classes.len()));
                }
                if items.enums.len() > 0 {
                    parts.push(format!("{} enum", items.enums.len()));
                }
                if items.type_aliases.len() > 0 {
                    parts.push(format!("{} type", items.type_aliases.len()));
                }
                if items.clients.len() > 0 {
                    parts.push(format!("{} client", items.clients.len()));
                }
                if items.tests.len() > 0 {
                    parts.push(format!("{} test", items.tests.len()));
                }
                if items.generators.len() > 0 {
                    parts.push(format!("{} gen", items.generators.len()));
                }
                if items.template_strings.len() > 0 {
                    parts.push(format!("{} tmpl", items.template_strings.len()));
                }
                if items.retry_policies.len() > 0 {
                    parts.push(format!("{} retry", items.retry_policies.len()));
                }
                let file_summary = parts.join(", ");

                // Flat text: file header
                writeln!(output, "  File: {file_name}  ({file_summary})").ok();
                output_annotated.push((
                    format!("  File: {file_name}  ({file_summary})"),
                    LineStatus::Unknown,
                ));

                // Build file detail lines for column view
                let mut file_detail: Vec<String> = Vec::new();
                file_detail.push(format!("File: {file_name}"));
                file_detail.push(format!("Path: {file_path}"));
                file_detail.push(format!("Items: {file_summary}"));
                file_detail.push(String::new());

                // Contributions
                let contrib = &index.symbol_contributions;
                if !contrib.types.is_empty() || !contrib.values.is_empty() {
                    file_detail.push("Contributions:".to_string());
                    for (name, c) in &contrib.types {
                        file_detail.push(format!("  type {} {}", c.definition.kind_name(), name));
                    }
                    for (name, c) in &contrib.values {
                        file_detail.push(format!("  value {} {}", c.definition.kind_name(), name));
                    }
                    file_detail.push(String::new());
                }

                // Scope tree (skip Project/Package/Namespace wrapper scopes)
                file_detail.push("Scope Tree:".to_string());
                for (i, scope) in index.scopes.iter().enumerate() {
                    // Skip structural wrapper scopes
                    match scope.kind {
                        ScopeKind::Project | ScopeKind::Package | ScopeKind::Namespace => continue,
                        _ => {}
                    }
                    let depth = {
                        let mut d = 0u32;
                        let mut cur = scope.parent;
                        while let Some(p) = cur {
                            let pk = &index.scopes[p.index() as usize].kind;
                            match pk {
                                ScopeKind::Project | ScopeKind::Package | ScopeKind::Namespace => {}
                                _ => d += 1,
                            }
                            cur = index.scopes[p.index() as usize].parent;
                        }
                        d
                    };
                    let indent = "  ".repeat(depth as usize + 1);
                    let kind_str = match &scope.kind {
                        ScopeKind::File => "File",
                        ScopeKind::Class => "Class",
                        ScopeKind::Enum => "Enum",
                        ScopeKind::Function => "Function",
                        ScopeKind::TypeAlias => "TypeAlias",
                        ScopeKind::Block => "Block",
                        ScopeKind::Lambda => "Lambda",
                        ScopeKind::Item => "Item",
                        _ => unreachable!(),
                    };
                    let name_str = scope
                        .name
                        .as_ref()
                        .map(|n| format!(" \"{}\"", n))
                        .unwrap_or_default();
                    let range = scope.range;
                    file_detail.push(format!(
                        "{indent}[{i}] {kind_str}{name_str}  {}..{}",
                        u32::from(range.start()),
                        u32::from(range.end()),
                    ));
                    let bindings = &index.scope_bindings[i];
                    for (name, idx) in &bindings.params {
                        file_detail.push(format!("{indent}  param[{idx}]: {name}"));
                    }
                    for (name, _site, range) in &bindings.bindings {
                        file_detail.push(format!(
                            "{indent}  let {name}  {}..{}",
                            u32::from(range.start()),
                            u32::from(range.end()),
                        ));
                    }
                }

                // Diagnostics
                let per_file_count = index
                    .extra
                    .as_ref()
                    .map(|e| e.diagnostics.len())
                    .unwrap_or(0);
                if per_file_count > 0 {
                    file_detail.push(String::new());
                    file_detail.push(format!("Diagnostics ({per_file_count}):"));
                    if let Some(extra) = &index.extra {
                        for diag in &extra.diagnostics {
                            let baml_compiler2_hir::diagnostic::Hir2Diagnostic::DuplicateDefinition {
                                name,
                                scope,
                                sites,
                            } = diag;
                            let use_dot = sites[0].kind.is_member();
                            let qualified = match (scope, use_dot) {
                                (Some(s), true) => format!("{}.{}", s, name),
                                _ => name.to_string(),
                            };
                            let in_scope = match (scope, use_dot) {
                                (Some(s), false) => format!(" in `{}`", s),
                                _ => String::new(),
                            };
                            file_detail
                                .push(format!("  !! duplicate `{}`{}", qualified, in_scope,));
                        }
                    }
                }

                // ── Build column items from the item tree ───────────
                let mut col_items: Vec<Hir2ItemEntry> = Vec::new();

                // Helper to build function signature string
                let build_fn_sig =
                    |f: &baml_compiler2_hir::item_tree::Function| -> (String, String, Vec<String>) {
                        let params_str: Vec<String> = f
                            .params
                            .iter()
                            .map(|p| {
                                p.type_expr
                                    .as_ref()
                                    .map(|te| {
                                        format!(
                                            "{}: {}",
                                            p.name.as_str(),
                                            hir2_type_expr_to_string(&te.expr)
                                        )
                                    })
                                    .unwrap_or_else(|| p.name.as_str().to_string())
                            })
                            .collect();
                        let ret_str = f
                            .return_type
                            .as_ref()
                            .map(|te| hir2_type_expr_to_string(&te.expr))
                            .unwrap_or_else(|| "?".to_string());
                        let body_kind = match &f.body {
                            Some(FunctionBodyDef::Expr(_, _)) => "expr",
                            Some(FunctionBodyDef::Llm(_)) => "llm",
                            Some(FunctionBodyDef::Builtin(_)) => "builtin",
                            None => "-",
                        };
                        let sig = format!("({}) -> {}", params_str.join(", "), ret_str);
                        let mut detail = vec![
                            format!("function {}", f.name),
                            format!("  ({}) -> {}", params_str.join(", "), ret_str),
                            format!("  body: {}", body_kind),
                        ];
                        for p in &f.params {
                            if let Some(te) = &p.type_expr {
                                detail.push(format!(
                                    "  param {}: {}",
                                    p.name,
                                    hir2_type_expr_to_string(&te.expr)
                                ));
                            }
                        }
                        (sig, body_kind.to_string(), detail)
                    };

                // Collect per-file diagnostics for matching against items
                let file_diagnostics = index.diagnostics();

                // Helper: collect errors relevant to a specific item name
                let item_errors = |item_name: &str| -> Vec<String> {
                    let mut errors = Vec::new();
                    // Per-file Hir2Diagnostics (e.g. duplicate members within a scope)
                    for diag in file_diagnostics {
                        let baml_compiler2_hir::diagnostic::Hir2Diagnostic::DuplicateDefinition {
                            name,
                            scope,
                            sites,
                        } = diag;
                        let matches = match scope {
                            Some(s) => s.as_str() == item_name,
                            None => name.as_str() == item_name,
                        };
                        if matches {
                            let use_dot = sites[0].kind.is_member();
                            let qualified = match (scope, use_dot) {
                                (Some(s), true) => format!("{}.{}", s, name),
                                _ => name.to_string(),
                            };
                            errors.push(format!("!! duplicate `{}`", qualified));
                            for (i, site) in sites.iter().enumerate() {
                                errors.push(format!(
                                    "   {}. {} at {}..{}",
                                    i + 1,
                                    site.kind.as_str(),
                                    u32::from(site.range.start()),
                                    u32::from(site.range.end()),
                                ));
                            }
                        }
                    }
                    // Namespace-level conflicts (cross-file duplicates)
                    for conflict in ns_items.conflicts() {
                        if conflict.name.as_str() == item_name {
                            errors.push(format!(
                                "!! `{}` defined {} times across files",
                                conflict.name,
                                conflict.entries.len()
                            ));
                            for (i, entry) in conflict.entries.iter().enumerate() {
                                let fpath = entry.definition.file(&self.db).path(&self.db);
                                let fname = fpath
                                    .file_name()
                                    .map(|f| f.to_string_lossy().to_string())
                                    .unwrap_or_else(|| fpath.display().to_string());
                                errors.push(format!(
                                    "   {}. {} in {}",
                                    i + 1,
                                    entry.definition.kind_name(),
                                    fname
                                ));
                            }
                        }
                    }
                    errors
                };

                for (_, f) in &items.functions {
                    let (sig, body_kind, mut detail) = build_fn_sig(f);
                    let errors = item_errors(f.name.as_str());
                    let err_count = errors.iter().filter(|l| l.starts_with("!!")).count();
                    if !errors.is_empty() {
                        detail.push(String::new());
                        detail.push(format!("Errors ({err_count}):"));
                        detail.extend(errors);
                    }
                    let flat_line = format!("    fn {} {}  [{}]", f.name, sig, body_kind);
                    writeln!(output, "{flat_line}").ok();
                    output_annotated.push((flat_line, status));

                    col_items.push(Hir2ItemEntry {
                        name: f.name.to_string(),
                        kind: "fn".to_string(),
                        signature: sig,
                        detail_lines: detail,
                        has_errors: err_count > 0,
                    });
                }
                for (_, c) in &items.classes {
                    let flat_line = format!("    class {}", c.name);
                    writeln!(output, "{flat_line}").ok();
                    output_annotated.push((flat_line, status));

                    let mut detail = vec![format!("class {}", c.name)];
                    if !c.fields.is_empty() {
                        detail.push(format!("  {} fields:", c.fields.len()));
                        for field in &c.fields {
                            let ty_str = field
                                .type_expr
                                .as_ref()
                                .map(|te| hir2_type_expr_to_string(&te.expr))
                                .unwrap_or_else(|| "?".to_string());
                            detail.push(format!("    {}: {}", field.name, ty_str));
                        }
                    }
                    let errors = item_errors(c.name.as_str());
                    let err_count = errors.iter().filter(|l| l.starts_with("!!")).count();
                    if !errors.is_empty() {
                        detail.push(String::new());
                        detail.push(format!("Errors ({err_count}):"));
                        detail.extend(errors);
                    }

                    col_items.push(Hir2ItemEntry {
                        name: c.name.to_string(),
                        kind: "class".to_string(),
                        signature: format!("{{{} fields}}", c.fields.len()),
                        detail_lines: detail,
                        has_errors: err_count > 0,
                    });
                }
                for (_, e) in &items.enums {
                    let flat_line = format!("    enum {}", e.name);
                    writeln!(output, "{flat_line}").ok();
                    output_annotated.push((flat_line, status));

                    let mut detail = vec![format!("enum {}", e.name)];
                    if !e.variants.is_empty() {
                        detail.push(format!("  {} variants:", e.variants.len()));
                        for v in &e.variants {
                            detail.push(format!("    {}", v.name));
                        }
                    }
                    let errors = item_errors(e.name.as_str());
                    let err_count = errors.iter().filter(|l| l.starts_with("!!")).count();
                    if !errors.is_empty() {
                        detail.push(String::new());
                        detail.push(format!("Errors ({err_count}):"));
                        detail.extend(errors);
                    }

                    col_items.push(Hir2ItemEntry {
                        name: e.name.to_string(),
                        kind: "enum".to_string(),
                        signature: format!("{{{} variants}}", e.variants.len()),
                        detail_lines: detail,
                        has_errors: err_count > 0,
                    });
                }
                for (_, ta) in &items.type_aliases {
                    let flat_line = format!("    type {}", ta.name);
                    writeln!(output, "{flat_line}").ok();
                    output_annotated.push((flat_line, status));

                    let ty_str = ta
                        .type_expr
                        .as_ref()
                        .map(|te| hir2_type_expr_to_string(&te.expr))
                        .unwrap_or_else(|| "?".to_string());
                    let mut detail = vec![format!("type {}", ta.name), format!("  = {}", ty_str)];
                    let errors = item_errors(ta.name.as_str());
                    let err_count = errors.iter().filter(|l| l.starts_with("!!")).count();
                    if !errors.is_empty() {
                        detail.push(String::new());
                        detail.push(format!("Errors ({err_count}):"));
                        detail.extend(errors);
                    }

                    col_items.push(Hir2ItemEntry {
                        name: ta.name.to_string(),
                        kind: "type".to_string(),
                        signature: format!("= {}", ty_str),
                        detail_lines: detail,
                        has_errors: err_count > 0,
                    });
                }

                // Bubble item-level errors into the file detail
                let items_with_errors: Vec<&Hir2ItemEntry> =
                    col_items.iter().filter(|item| item.has_errors).collect();
                if !items_with_errors.is_empty() {
                    file_detail.push(String::new());
                    file_detail.push(format!(
                        "Errors ({} items with errors):",
                        items_with_errors.len()
                    ));
                    for item in &items_with_errors {
                        for line in &item.detail_lines {
                            if line.starts_with("!!") || line.starts_with("Errors (") {
                                file_detail.push(format!("  [{}] {}", item.name, line));
                            } else if line.starts_with("   ") && line.contains(". ") {
                                file_detail.push(format!("  {}", line.trim()));
                            }
                        }
                    }
                }

                let file_error_count = per_file_count
                    + items_with_errors.len()
                    + ns_items
                        .conflicts()
                        .iter()
                        .filter(|c| {
                            c.entries.iter().any(|e| {
                                e.definition.file(&self.db).path(&self.db) == path.as_path()
                            })
                        })
                        .count();

                col_pkg.files.push(Hir2FileEntry {
                    name: file_name,
                    summary: file_summary,
                    items: col_items,
                    detail_lines: file_detail,
                    error_count: file_error_count,
                });

                writeln!(output).ok();
                output_annotated.push((String::new(), LineStatus::Unknown));
            }

            // Flat text: show merged namespace items once per package
            if !ns_items.types.is_empty() || !ns_items.values.is_empty() {
                writeln!(output, "  Namespace Items (merged):").ok();
                output_annotated.push((
                    "  Namespace Items (merged):".to_string(),
                    LineStatus::Unknown,
                ));
                let mut sorted_types: Vec<_> = ns_items.types.iter().collect();
                sorted_types.sort_by_key(|(name, _)| name.as_str().to_string());
                for (name, def) in &sorted_types {
                    let line = format!("    type {} {name}", def.kind_name());
                    writeln!(output, "{line}").ok();
                    output_annotated.push((line.clone(), LineStatus::Cached));
                }
                let mut sorted_values: Vec<_> = ns_items.values.iter().collect();
                sorted_values.sort_by_key(|(name, _)| name.as_str().to_string());
                for (name, def) in &sorted_values {
                    let line = format!("    value {} {name}", def.kind_name());
                    writeln!(output, "{line}").ok();
                    output_annotated.push((line.clone(), LineStatus::Cached));
                }
                writeln!(output).ok();
                output_annotated.push((String::new(), LineStatus::Unknown));
            }

            // Bubble file/item errors into the package namespace_summary
            let files_with_errors: Vec<&Hir2FileEntry> =
                col_pkg.files.iter().filter(|f| f.error_count > 0).collect();
            if !files_with_errors.is_empty() {
                let total_errors: usize = files_with_errors.iter().map(|f| f.error_count).sum();
                col_pkg.namespace_summary.push(String::new());
                col_pkg.namespace_summary.push(format!(
                    "Errors ({total_errors} across {} files):",
                    files_with_errors.len()
                ));
                for file in &files_with_errors {
                    col_pkg
                        .namespace_summary
                        .push(format!("  {} ({} errors):", file.name, file.error_count));
                    for line in &file.detail_lines {
                        if line.starts_with("Errors (") || line.starts_with("  [") {
                            col_pkg
                                .namespace_summary
                                .push(format!("    {}", line.trim()));
                        }
                    }
                }
            }

            column_data.packages.push(col_pkg);
        }

        // Clamp column selections to valid ranges
        let state = &mut self.hir2_column_state;
        if !column_data.packages.is_empty() {
            state.selected[0] = state.selected[0].min(column_data.packages.len() - 1);
            let pkg = &column_data.packages[state.selected[0]];
            if !pkg.files.is_empty() {
                state.selected[1] = state.selected[1].min(pkg.files.len() - 1);
                let file = &pkg.files[state.selected[1]];
                if !file.items.is_empty() {
                    state.selected[2] = state.selected[2].min(file.items.len() - 1);
                }
            }
        }

        self.hir2_column_data = column_data;
        self.phase_outputs.insert(CompilerPhase::Hir2, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Hir2, output_annotated);
    }

    fn run_tir2(&mut self) {
        use baml_compiler2_ast::{Expr, ExprBody, Literal, Pattern, Stmt};
        use baml_compiler2_hir::scope::ScopeKind;
        use baml_compiler2_tir::ty::Ty;

        fn pat_desc(pat_id: baml_compiler2_ast::PatId, body: &ExprBody) -> String {
            let pat = &body.patterns[pat_id];
            match pat {
                Pattern::Binding(n) => n.to_string(),
                Pattern::TypedBinding { name, ty } => {
                    format!("{name}: {}", hir2_type_expr_to_string(ty))
                }
                Pattern::Literal(lit) => lit.to_string(),
                Pattern::Null => "null".into(),
                Pattern::EnumVariant { enum_name, variant } => format!("{enum_name}.{variant}"),
                Pattern::Union(pats) => pats
                    .iter()
                    .map(|p| pat_desc(*p, body))
                    .collect::<Vec<_>>()
                    .join(" | "),
            }
        }

        /// Compact description of an expression from the ExprBody arena.
        fn expr_desc(expr_id: baml_compiler2_ast::ExprId, body: &ExprBody) -> String {
            let expr = &body.exprs[expr_id];
            match expr {
                Expr::Literal(lit) => match lit {
                    Literal::String(s) => {
                        let truncated = if s.len() > 20 {
                            format!("{}...", &s[..17])
                        } else {
                            s.clone()
                        };
                        format!("\"{}\"", truncated)
                    }
                    Literal::Int(i) => i.to_string(),
                    Literal::Float(f) => f.clone(),
                    Literal::Bool(b) => b.to_string(),
                },
                Expr::Null => "null".into(),
                Expr::Path(segments) => segments
                    .iter()
                    .map(|n| n.as_str())
                    .collect::<Vec<_>>()
                    .join("."),
                Expr::If { .. } => "if ...".into(),
                Expr::Match { .. } => "match ...".into(),
                Expr::Catch { .. } => "catch ...".into(),
                Expr::Throw { value } => format!("throw {}", expr_desc(*value, body)),
                Expr::Binary { op, .. } => format!("... {op:?} ..."),
                Expr::Unary { op, expr: inner } => format!("{op:?} {}", expr_desc(*inner, body)),
                Expr::Call { callee, args } => {
                    let callee_str = expr_desc(*callee, body);
                    format!("{callee_str}({})", if args.is_empty() { "" } else { "..." })
                }
                Expr::Object {
                    type_name, fields, ..
                } => {
                    let tn = type_name.as_ref().map(|n| n.as_str()).unwrap_or("_");
                    format!("{tn} {{ {} fields }}", fields.len())
                }
                Expr::Array { elements } => format!("[{} items]", elements.len()),
                Expr::Map { entries } => format!("map {{ {} entries }}", entries.len()),
                Expr::Block { stmts, tail_expr } => {
                    let tail = if tail_expr.is_some() { " + tail" } else { "" };
                    format!("{{ {} stmts{tail} }}", stmts.len())
                }
                Expr::FieldAccess { base, field } => format!("{}.{field}", expr_desc(*base, body)),
                Expr::Index { base, .. } => format!("{}[...]", expr_desc(*base, body)),
                Expr::Missing => "<missing>".into(),
            }
        }

        /// Build a fully qualified name by walking parent scopes.
        /// e.g. "user::Foo::Bar" for method Bar inside class Foo in package user.
        fn qualified_name(scopes: &[baml_compiler2_hir::scope::Scope], scope_idx: usize) -> String {
            let mut parts = Vec::new();
            let mut cur = scope_idx;
            loop {
                let s = &scopes[cur];
                match s.kind {
                    // Skip structural scopes that aren't interesting
                    ScopeKind::Project => break,
                    ScopeKind::File => {}
                    _ => {
                        if let Some(ref name) = s.name {
                            parts.push(name.to_string());
                        }
                    }
                }
                if let Some(parent) = s.parent {
                    cur = parent.index() as usize;
                } else {
                    break;
                }
            }
            parts.reverse();
            parts.join(".")
        }

        let mut output = String::new();
        let mut output_annotated = Vec::new();

        let mut sorted_files: Vec<_> = self.source_files.iter().collect();
        sorted_files.sort_by_key(|(path, _)| path.as_path());

        for (path, source_file) in sorted_files {
            let file_path = path.display().to_string();
            let file_recomputed = self.modified_files.contains(path);
            let status = if file_recomputed {
                LineStatus::Recomputed
            } else {
                LineStatus::Cached
            };

            writeln!(output, "File: {file_path}").ok();
            output_annotated.push((format!("File: {file_path}"), LineStatus::Unknown));

            let index = baml_compiler2_hir::file_semantic_index(&self.db, *source_file);

            for (i, scope) in index.scopes.iter().enumerate() {
                let scope_id = index.scope_ids[i];
                let kind_str = match &scope.kind {
                    ScopeKind::Function => "function",
                    ScopeKind::Lambda => "lambda",
                    ScopeKind::Block => "block",
                    ScopeKind::Class => "class",
                    ScopeKind::Enum => "enum",
                    ScopeKind::TypeAlias => "type",
                    _ => continue,
                };
                let fqn = qualified_name(&index.scopes, i);

                // ── Structural scopes (class/enum/type alias) ───────────
                if matches!(
                    scope.kind,
                    ScopeKind::Class | ScopeKind::Enum | ScopeKind::TypeAlias
                ) {
                    let contrib = &index.symbol_contributions;
                    match &scope.kind {
                        ScopeKind::Class => {
                            for (name, c) in &contrib.types {
                                if scope.name.as_ref() == Some(name) {
                                    if let baml_compiler2_hir::contributions::Definition::Class(
                                        class_loc,
                                    ) = c.definition
                                    {
                                        let resolved =
                                            baml_compiler2_tir::inference::resolve_class_fields(
                                                &self.db, class_loc,
                                            );
                                        let header = format!("  {kind_str} {fqn} {{");
                                        writeln!(output, "{header}").ok();
                                        output_annotated.push((header, status));
                                        for (fname, fty) in &resolved.fields {
                                            let line = format!("    {fname}: {fty}");
                                            writeln!(output, "{line}").ok();
                                            output_annotated.push((line, status));
                                        }
                                        let closing = "  }".to_string();
                                        writeln!(output, "{closing}").ok();
                                        output_annotated.push((closing, status));
                                        break;
                                    }
                                }
                            }
                        }
                        ScopeKind::TypeAlias => {
                            for (name, c) in &contrib.types {
                                if scope.name.as_ref() == Some(name) {
                                    if let baml_compiler2_hir::contributions::Definition::TypeAlias(alias_loc) = c.definition {
                                        let resolved = baml_compiler2_tir::inference::resolve_type_alias(&self.db, alias_loc);
                                        let line = format!("  {kind_str} {fqn} = {}", resolved.ty);
                                        writeln!(output, "{line}").ok();
                                        output_annotated.push((line, status));
                                        break;
                                    }
                                }
                            }
                        }
                        ScopeKind::Enum => {
                            let line = format!("  {kind_str} {fqn}");
                            writeln!(output, "{line}").ok();
                            output_annotated.push((line, status));
                        }
                        _ => {}
                    }
                    continue;
                }

                // ── Function/Lambda/Block scopes ────────────────────────
                let inference =
                    baml_compiler2_tir::inference::infer_scope_types(&self.db, scope_id);

                // Find the function body by matching scope range against item_tree functions.
                // This works for both top-level functions AND class methods.
                let mut func_body: Option<std::sync::Arc<baml_compiler2_hir::body::FunctionBody>> =
                    None;
                let mut sig_display = String::new();
                if matches!(scope.kind, ScopeKind::Function) {
                    let item_tree = &index.item_tree;
                    for (local_id, func_data) in &item_tree.functions {
                        if func_data.span == scope.range {
                            let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(
                                &self.db,
                                *source_file,
                                *local_id,
                            );
                            func_body =
                                Some(baml_compiler2_hir::body::function_body(&self.db, func_loc));
                            let sig = baml_compiler2_hir::signature::function_signature(
                                &self.db, func_loc,
                            );
                            let params: Vec<String> = sig
                                .params
                                .iter()
                                .map(|(pname, ptype)| {
                                    format!("{}: {}", pname, hir2_type_expr_to_string(ptype))
                                })
                                .collect();
                            let ret = sig
                                .return_type
                                .as_ref()
                                .map(|t| hir2_type_expr_to_string(t))
                                .unwrap_or_else(|| "?".into());
                            sig_display = format!("({}) -> {ret}", params.join(", "));
                            break;
                        }
                    }
                }

                // Collect expression types for this scope
                let mut expr_types: Vec<_> = Vec::new();
                for (expr_id, owner_scope) in &index.expr_scopes {
                    if owner_scope.index() as usize == i {
                        let ty = inference
                            .expression_type(*expr_id)
                            .cloned()
                            .unwrap_or(Ty::Unknown);
                        expr_types.push((*expr_id, ty));
                    }
                }

                if expr_types.is_empty() {
                    continue;
                }

                // Get the ExprBody for expression descriptions
                let expr_body = func_body.as_ref().and_then(|fb| {
                    if let baml_compiler2_hir::body::FunctionBody::Expr(body) = fb.as_ref() {
                        Some(body)
                    } else {
                        None
                    }
                });

                let header = format!("  {kind_str} {fqn}{sig_display} {{");
                writeln!(output, "{header}").ok();
                output_annotated.push((header, status));

                // Recursive renderer that expands Block expressions into their
                // statements and tail, showing types at each level.
                fn render_expr(
                    expr_id: baml_compiler2_ast::ExprId,
                    body: &ExprBody,
                    inference: &baml_compiler2_tir::inference::ScopeInference,
                    indent: usize,
                    output: &mut String,
                    output_annotated: &mut Vec<(String, LineStatus)>,
                    status: LineStatus,
                ) {
                    let pad = " ".repeat(indent);
                    let ty = inference
                        .expression_type(expr_id)
                        .map(|t| t.to_string())
                        .unwrap_or_else(|| "unknown".into());
                    let expr = &body.exprs[expr_id];

                    match expr {
                        Expr::Block { stmts, tail_expr } => {
                            let line = format!("{pad}{{ : {ty}");
                            writeln!(output, "{line}").ok();
                            output_annotated.push((line, status));

                            for stmt_id in stmts {
                                render_stmt(
                                    *stmt_id,
                                    body,
                                    inference,
                                    indent + 2,
                                    output,
                                    output_annotated,
                                    status,
                                );
                            }
                            if let Some(tail) = tail_expr {
                                render_expr(
                                    *tail,
                                    body,
                                    inference,
                                    indent + 2,
                                    output,
                                    output_annotated,
                                    status,
                                );
                            }

                            let closing = format!("{pad}}}");
                            writeln!(output, "{closing}").ok();
                            output_annotated.push((closing, status));
                        }
                        Expr::If {
                            condition,
                            then_branch,
                            else_branch,
                        } => {
                            let cond_desc = expr_desc(*condition, body);
                            let line = format!("{pad}if {cond_desc} : {ty}");
                            writeln!(output, "{line}").ok();
                            output_annotated.push((line, status));
                            render_expr(
                                *then_branch,
                                body,
                                inference,
                                indent + 2,
                                output,
                                output_annotated,
                                status,
                            );
                            if let Some(else_expr) = else_branch {
                                let else_line = format!("{pad}else");
                                writeln!(output, "{else_line}").ok();
                                output_annotated.push((else_line, status));
                                render_expr(
                                    *else_expr,
                                    body,
                                    inference,
                                    indent + 2,
                                    output,
                                    output_annotated,
                                    status,
                                );
                            }
                        }
                        Expr::Match {
                            scrutinee, arms, ..
                        } => {
                            let scrut_desc = expr_desc(*scrutinee, body);
                            let line = format!("{pad}match {scrut_desc} : {ty}");
                            writeln!(output, "{line}").ok();
                            output_annotated.push((line, status));
                            for arm_id in arms {
                                let arm = &body.match_arms[*arm_id];
                                let pat = pat_desc(arm.pattern, body);
                                let guard = arm
                                    .guard
                                    .map(|g| format!(" if {}", expr_desc(g, body)))
                                    .unwrap_or_default();
                                let arm_line = format!("{pad}  {pat}{guard} =>");
                                writeln!(output, "{arm_line}").ok();
                                output_annotated.push((arm_line, status));
                                render_expr(
                                    arm.body,
                                    body,
                                    inference,
                                    indent + 4,
                                    output,
                                    output_annotated,
                                    status,
                                );
                            }
                        }
                        _ => {
                            let desc = expr_desc(expr_id, body);
                            let line = format!("{pad}{desc} : {ty}");
                            writeln!(output, "{line}").ok();
                            output_annotated.push((line, status));
                        }
                    }
                }

                fn render_stmt(
                    stmt_id: baml_compiler2_ast::StmtId,
                    body: &ExprBody,
                    inference: &baml_compiler2_tir::inference::ScopeInference,
                    indent: usize,
                    output: &mut String,
                    output_annotated: &mut Vec<(String, LineStatus)>,
                    status: LineStatus,
                ) {
                    let pad = " ".repeat(indent);
                    let stmt = &body.stmts[stmt_id];
                    match stmt {
                        Stmt::Let {
                            pattern,
                            initializer,
                            ..
                        } => {
                            let pat_name = match &body.patterns[*pattern] {
                                Pattern::Binding(n) => n.to_string(),
                                Pattern::TypedBinding { name, ty } => {
                                    format!("{name}: {}", hir2_type_expr_to_string(ty))
                                }
                                other => format!("{other:?}"),
                            };
                            if let Some(init) = initializer {
                                let init_ty = inference
                                    .expression_type(*init)
                                    .map(|t| t.to_string())
                                    .unwrap_or_else(|| "unknown".into());
                                let binding_ty =
                                    inference.binding_type(*pattern).map(|t| t.to_string());
                                let init_desc = expr_desc(*init, body);
                                // Show both expr type and binding type when they differ
                                let ty_display = match &binding_ty {
                                    Some(bt) if *bt != init_ty => format!("{init_ty} -> {bt}"),
                                    _ => init_ty,
                                };
                                let line =
                                    format!("{pad}let {pat_name} = {init_desc} : {ty_display}");
                                writeln!(output, "{line}").ok();
                                output_annotated.push((line, status));
                                // If the initializer is a block, expand it
                                if matches!(&body.exprs[*init], Expr::Block { .. }) {
                                    render_expr(
                                        *init,
                                        body,
                                        inference,
                                        indent + 2,
                                        output,
                                        output_annotated,
                                        status,
                                    );
                                }
                            } else {
                                let line = format!("{pad}let {pat_name}");
                                writeln!(output, "{line}").ok();
                                output_annotated.push((line, status));
                            }
                        }
                        Stmt::Return(Some(expr_id)) => {
                            let ty = inference
                                .expression_type(*expr_id)
                                .map(|t| t.to_string())
                                .unwrap_or_else(|| "unknown".into());
                            let desc = expr_desc(*expr_id, body);
                            let line = format!("{pad}return {desc} : {ty}");
                            writeln!(output, "{line}").ok();
                            output_annotated.push((line, status));
                            if matches!(&body.exprs[*expr_id], Expr::Block { .. }) {
                                render_expr(
                                    *expr_id,
                                    body,
                                    inference,
                                    indent + 2,
                                    output,
                                    output_annotated,
                                    status,
                                );
                            }
                        }
                        Stmt::Return(None) => {
                            let line = format!("{pad}return");
                            writeln!(output, "{line}").ok();
                            output_annotated.push((line, status));
                        }
                        Stmt::Throw { value } => {
                            let ty = inference
                                .expression_type(*value)
                                .map(|t| t.to_string())
                                .unwrap_or_else(|| "unknown".into());
                            let desc = expr_desc(*value, body);
                            let line = format!("{pad}throw {desc} : {ty}");
                            writeln!(output, "{line}").ok();
                            output_annotated.push((line, status));
                            if matches!(&body.exprs[*value], Expr::Block { .. }) {
                                render_expr(
                                    *value,
                                    body,
                                    inference,
                                    indent + 2,
                                    output,
                                    output_annotated,
                                    status,
                                );
                            }
                        }
                        Stmt::Expr(expr_id) => {
                            render_expr(
                                *expr_id,
                                body,
                                inference,
                                indent,
                                output,
                                output_annotated,
                                status,
                            );
                        }
                        Stmt::While {
                            condition,
                            body: body_expr,
                            ..
                        } => {
                            let cond_desc = expr_desc(*condition, body);
                            let line = format!("{pad}while {cond_desc}");
                            writeln!(output, "{line}").ok();
                            output_annotated.push((line, status));
                            render_expr(
                                *body_expr,
                                body,
                                inference,
                                indent + 2,
                                output,
                                output_annotated,
                                status,
                            );
                        }
                        Stmt::Assign { target, value } => {
                            let target_desc = expr_desc(*target, body);
                            let val_desc = expr_desc(*value, body);
                            let val_ty = inference
                                .expression_type(*value)
                                .map(|t| t.to_string())
                                .unwrap_or_else(|| "unknown".into());
                            let line = format!("{pad}{target_desc} = {val_desc} : {val_ty}");
                            writeln!(output, "{line}").ok();
                            output_annotated.push((line, status));
                        }
                        Stmt::AssignOp { target, op, value } => {
                            let target_desc = expr_desc(*target, body);
                            let val_desc = expr_desc(*value, body);
                            let val_ty = inference
                                .expression_type(*value)
                                .map(|t| t.to_string())
                                .unwrap_or_else(|| "unknown".into());
                            let line = format!("{pad}{target_desc} {op:?}= {val_desc} : {val_ty}");
                            writeln!(output, "{line}").ok();
                            output_annotated.push((line, status));
                        }
                        Stmt::Assert { condition } => {
                            let desc = expr_desc(*condition, body);
                            let line = format!("{pad}assert {desc}");
                            writeln!(output, "{line}").ok();
                            output_annotated.push((line, status));
                        }
                        Stmt::Break => {
                            let line = format!("{pad}break");
                            writeln!(output, "{line}").ok();
                            output_annotated.push((line, status));
                        }
                        Stmt::Continue => {
                            let line = format!("{pad}continue");
                            writeln!(output, "{line}").ok();
                            output_annotated.push((line, status));
                        }
                        Stmt::HeaderComment { name, level } => {
                            let line = format!("{pad}// [{level}] {name}");
                            writeln!(output, "{line}").ok();
                            output_annotated.push((line, status));
                        }
                        Stmt::Missing => {
                            let line = format!("{pad}<missing stmt>");
                            writeln!(output, "{line}").ok();
                            output_annotated.push((line, status));
                        }
                    }
                }

                if let Some(body) = expr_body {
                    // Find the root expression and render it recursively
                    if let Some(root) = body.root_expr {
                        render_expr(
                            root,
                            body,
                            &inference,
                            4,
                            &mut output,
                            &mut output_annotated,
                            status,
                        );
                    }
                } else {
                    // No body available — fall back to flat listing
                    for (expr_id, ty) in &expr_types {
                        let line = format!("    {expr_id:?} : {ty}");
                        writeln!(output, "{line}").ok();
                        output_annotated.push((line, status));
                    }
                }

                // Per-scope diagnostics — render right after the body
                let scope_id = index.scope_ids[i];
                let rendered =
                    baml_compiler2_tir::inference::render_scope_diagnostics(&self.db, scope_id);
                if !rendered.is_empty() {
                    for rd in &rendered {
                        let line = format!("    !! {rd}");
                        writeln!(output, "{line}").ok();
                        output_annotated.push((line, status));
                    }
                }

                let closing = "  }".to_string();
                writeln!(output, "{closing}").ok();
                output_annotated.push((closing, status));
            }

            writeln!(output).ok();
            output_annotated.push((String::new(), LineStatus::Unknown));
        }

        self.phase_outputs.insert(CompilerPhase::Tir2, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Tir2, output_annotated);

        // ── Build TIR2 column data ──────────────────────────────────────────
        self.build_tir2_column_data();
    }

    /// Build structured column data for the TIR2 browser.
    /// Reuses HIR2's package grouping, then enriches items with type-checked info.
    fn build_tir2_column_data(&mut self) {
        use baml_compiler2_hir::{
            file_package::file_package,
            namespace::{NamespaceId, namespace_items},
            scope::ScopeKind,
        };

        let mut sorted_files: Vec<_> = self.source_files.iter().collect();
        sorted_files.sort_by_key(|(path, _)| path.as_path());

        // Group files by (package, namespace)
        struct PkgGroup {
            package: String,
            namespace: String,
            ns_id_key: (String, Vec<String>),
            files: Vec<(std::path::PathBuf, baml_base::SourceFile)>,
        }
        let mut groups: Vec<PkgGroup> = Vec::new();
        for (path, source_file) in &sorted_files {
            let pkg_info = file_package(&self.db, **source_file);
            let pkg_name = pkg_info.package.as_str().to_string();
            let ns_path: Vec<String> = pkg_info
                .namespace_path
                .iter()
                .map(|n| n.as_str().to_string())
                .collect();
            let key = (pkg_name.clone(), ns_path.clone());
            let ns_str = if ns_path.is_empty() {
                "[]".to_string()
            } else {
                format!("[\"{}\"]", ns_path.join("\", \""))
            };
            if let Some(group) = groups.iter_mut().find(|g| g.ns_id_key == key) {
                group.files.push(((*path).clone(), **source_file));
            } else {
                groups.push(PkgGroup {
                    package: pkg_name,
                    namespace: ns_str,
                    ns_id_key: key,
                    files: vec![((*path).clone(), **source_file)],
                });
            }
        }

        let mut column_data = Tir2ColumnData {
            packages: Vec::new(),
        };

        for group in &groups {
            let first_file = group.files[0].1;
            let pkg_info = file_package(&self.db, first_file);
            let ns_id = NamespaceId::new(
                &self.db,
                pkg_info.package.clone(),
                pkg_info.namespace_path.clone(),
            );
            let ns_items = namespace_items(&self.db, ns_id);

            // Package summary with resolved types
            let mut ns_summary: Vec<String> = Vec::new();
            ns_summary.push(format!(
                "{} types, {} values",
                ns_items.types.len(),
                ns_items.values.len()
            ));
            ns_summary.push(String::new());

            // Show resolved types in the package summary
            if !ns_items.types.is_empty() {
                ns_summary.push("Types (resolved):".to_string());
                let mut sorted_types: Vec<_> = ns_items.types.iter().collect();
                sorted_types.sort_by_key(|(name, _)| name.as_str().to_string());
                for (name, def) in &sorted_types {
                    match **def {
                        baml_compiler2_hir::contributions::Definition::Class(class_loc) => {
                            let resolved = baml_compiler2_tir::inference::resolve_class_fields(
                                &self.db, class_loc,
                            );
                            ns_summary.push(format!(
                                "  class {} ({} fields)",
                                name,
                                resolved.fields.len()
                            ));
                        }
                        baml_compiler2_hir::contributions::Definition::TypeAlias(alias_loc) => {
                            let resolved = baml_compiler2_tir::inference::resolve_type_alias(
                                &self.db, alias_loc,
                            );
                            ns_summary.push(format!("  type {} = {}", name, resolved.ty));
                        }
                        _ => {
                            ns_summary.push(format!("  {} {}", def.kind_name(), name));
                        }
                    }
                }
                ns_summary.push(String::new());
            }
            if !ns_items.values.is_empty() {
                ns_summary.push("Values:".to_string());
                let mut sorted_values: Vec<_> = ns_items.values.iter().collect();
                sorted_values.sort_by_key(|(name, _)| name.as_str().to_string());
                for (name, def) in &sorted_values {
                    ns_summary.push(format!("  {} {}", def.kind_name(), name));
                }
            }

            let mut col_pkg = Tir2Package {
                name: group.package.clone(),
                namespace: group.namespace.clone(),
                files: Vec::new(),
                namespace_summary: ns_summary,
            };

            for (path, source_file) in &group.files {
                let file_name = path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string());

                let index = baml_compiler2_hir::file_semantic_index(&self.db, *source_file);
                let items = &index.item_tree;

                // Build summary parts
                let mut parts = Vec::new();
                if !items.functions.is_empty() {
                    parts.push(format!("{} fn", items.functions.len()));
                }
                if !items.classes.is_empty() {
                    parts.push(format!("{} class", items.classes.len()));
                }
                if !items.enums.is_empty() {
                    parts.push(format!("{} enum", items.enums.len()));
                }
                if !items.type_aliases.is_empty() {
                    parts.push(format!("{} type", items.type_aliases.len()));
                }
                let file_summary = parts.join(", ");

                let mut col_items: Vec<Tir2ItemEntry> = Vec::new();
                let mut file_detail: Vec<String> = Vec::new();
                let mut file_error_count: usize = 0;

                file_detail.push(format!("File: {file_name}"));
                file_detail.push(format!("Items: {file_summary}"));
                file_detail.push(String::new());

                let contrib = &index.symbol_contributions;

                // ── Functions: show inferred types + diagnostics ─────────
                for (local_id, f) in &items.functions {
                    let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(
                        &self.db,
                        *source_file,
                        *local_id,
                    );
                    let sig = baml_compiler2_hir::signature::function_signature(&self.db, func_loc);

                    let params: Vec<String> = sig
                        .params
                        .iter()
                        .map(|(pname, ptype)| {
                            format!("{}: {}", pname, hir2_type_expr_to_string(ptype))
                        })
                        .collect();
                    let ret = sig
                        .return_type
                        .as_ref()
                        .map(|t| hir2_type_expr_to_string(t))
                        .unwrap_or_else(|| "?".into());
                    let sig_str = format!("({}) -> {ret}", params.join(", "));

                    // Find the scope for this function and get inferred types
                    let scope_idx =
                        index.scopes.iter().enumerate().find(|(_, s)| {
                            matches!(s.kind, ScopeKind::Function) && s.range == f.span
                        });

                    let mut detail: Vec<DetailLine> = vec![
                        plain(format!("function {}", f.name)),
                        plain(format!("  signature: {sig_str}")),
                    ];

                    let mut has_errors = false;

                    if let Some((si, _)) = scope_idx {
                        let scope_id = index.scope_ids[si];
                        let inference =
                            baml_compiler2_tir::inference::infer_scope_types(&self.db, scope_id);

                        // Show inferred expression types (top-level summary)
                        let mut expr_count = 0usize;
                        for (expr_id, owner_scope) in &index.expr_scopes {
                            if owner_scope.index() as usize == si {
                                if let Some(_ty) = inference.expression_type(*expr_id) {
                                    expr_count += 1;
                                }
                            }
                        }
                        detail.push(plain(""));
                        detail.push(plain(format!("Expressions: {expr_count} typed")));

                        // Type-checked body rendering
                        let func_body = baml_compiler2_hir::body::function_body(&self.db, func_loc);
                        if let baml_compiler2_hir::body::FunctionBody::Expr(ref body) = *func_body {
                            detail.push(plain(""));
                            detail.push(plain("Body:"));
                            if let Some(root) = body.root_expr {
                                Self::render_expr_to_lines(root, body, &inference, 2, &mut detail);
                            }
                        }

                        // Diagnostics
                        let rendered = baml_compiler2_tir::inference::render_scope_diagnostics(
                            &self.db, scope_id,
                        );
                        if !rendered.is_empty() {
                            has_errors = true;
                            file_error_count += rendered.len();
                            detail.push(plain(""));
                            detail.push(plain(format!("Errors ({}):", rendered.len())));
                            for rd in &rendered {
                                detail.push(vec![DetailSpan::Error(format!("  !! {rd}"))]);
                            }
                        }
                    }

                    col_items.push(Tir2ItemEntry {
                        name: f.name.to_string(),
                        kind: "fn".to_string(),
                        signature: sig_str,
                        detail_lines: detail,
                        has_errors,
                    });
                }

                // ── Classes: show resolved field types ──────────────────
                for (_, c) in &items.classes {
                    let mut detail: Vec<DetailLine> = vec![plain(format!("class {}", c.name))];
                    let has_errors = false;

                    let resolved_fields = contrib.types.iter().find_map(|(name, cdef)| {
                        if *name == c.name {
                            if let baml_compiler2_hir::contributions::Definition::Class(class_loc) =
                                cdef.definition
                            {
                                Some(baml_compiler2_tir::inference::resolve_class_fields(
                                    &self.db, class_loc,
                                ))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    });

                    if let Some(resolved) = &resolved_fields {
                        if !resolved.fields.is_empty() {
                            detail.push(plain(format!(
                                "  {} fields (resolved):",
                                resolved.fields.len()
                            )));
                            for (fname, fty) in &resolved.fields {
                                detail.push(vec![
                                    DetailSpan::Code(format!("    {fname}")),
                                    DetailSpan::TypeAnnotation(format!(": {fty}")),
                                ]);
                            }
                        }
                    }

                    let field_count = resolved_fields
                        .as_ref()
                        .map(|r| r.fields.len())
                        .unwrap_or(c.fields.len());

                    col_items.push(Tir2ItemEntry {
                        name: c.name.to_string(),
                        kind: "class".to_string(),
                        signature: format!("{{{field_count} fields}}"),
                        detail_lines: detail,
                        has_errors,
                    });
                }

                // ── Enums ───────────────────────────────────────────────
                for (_, e) in &items.enums {
                    let mut detail: Vec<DetailLine> = vec![plain(format!("enum {}", e.name))];
                    if !e.variants.is_empty() {
                        detail.push(plain(format!("  {} variants:", e.variants.len())));
                        for v in &e.variants {
                            detail.push(plain(format!("    {}", v.name)));
                        }
                    }

                    col_items.push(Tir2ItemEntry {
                        name: e.name.to_string(),
                        kind: "enum".to_string(),
                        signature: format!("{{{} variants}}", e.variants.len()),
                        detail_lines: detail,
                        has_errors: false,
                    });
                }

                // ── Type aliases: show resolved type ────────────────────
                for (_, ta) in &items.type_aliases {
                    let mut detail: Vec<DetailLine> = vec![plain(format!("type {}", ta.name))];

                    let resolved_ty = contrib.types.iter().find_map(|(name, cdef)| {
                        if *name == ta.name {
                            if let baml_compiler2_hir::contributions::Definition::TypeAlias(
                                alias_loc,
                            ) = cdef.definition
                            {
                                Some(baml_compiler2_tir::inference::resolve_type_alias(
                                    &self.db, alias_loc,
                                ))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    });

                    let sig = if let Some(resolved) = &resolved_ty {
                        let s = format!("= {}", resolved.ty);
                        detail.push(vec![
                            DetailSpan::Code("  ".into()),
                            DetailSpan::TypeAnnotation(s.clone()),
                        ]);
                        s
                    } else {
                        let raw = ta
                            .type_expr
                            .as_ref()
                            .map(|te| hir2_type_expr_to_string(&te.expr))
                            .unwrap_or_else(|| "?".to_string());
                        detail.push(plain(format!("  = {raw} (unresolved)")));
                        format!("= {raw}")
                    };

                    col_items.push(Tir2ItemEntry {
                        name: ta.name.to_string(),
                        kind: "type".to_string(),
                        signature: sig,
                        detail_lines: detail,
                        has_errors: false,
                    });
                }

                // Bubble item errors into file detail
                let items_with_errors: Vec<&Tir2ItemEntry> =
                    col_items.iter().filter(|item| item.has_errors).collect();
                if !items_with_errors.is_empty() {
                    file_detail.push(format!(
                        "Errors ({} items with errors):",
                        items_with_errors.len()
                    ));
                    for item in &items_with_errors {
                        for line in &item.detail_lines {
                            let text = detail_line_text(line);
                            let trimmed = text.trim_start();
                            if trimmed.starts_with("!!") || trimmed.starts_with("Errors (") {
                                file_detail.push(format!("  [{}] {}", item.name, text.trim()));
                            }
                        }
                    }
                }

                col_pkg.files.push(Tir2FileEntry {
                    name: file_name,
                    summary: file_summary,
                    items: col_items,
                    detail_lines: file_detail,
                    error_count: file_error_count,
                });
            }

            // Bubble file errors into package summary
            let files_with_errors: Vec<&Tir2FileEntry> =
                col_pkg.files.iter().filter(|f| f.error_count > 0).collect();
            if !files_with_errors.is_empty() {
                let total_errors: usize = files_with_errors.iter().map(|f| f.error_count).sum();
                col_pkg.namespace_summary.push(String::new());
                col_pkg.namespace_summary.push(format!(
                    "Errors ({total_errors} across {} files):",
                    files_with_errors.len()
                ));
                for file in &files_with_errors {
                    col_pkg
                        .namespace_summary
                        .push(format!("  {} ({} errors)", file.name, file.error_count));
                }
            }

            column_data.packages.push(col_pkg);
        }

        // Clamp column selections
        let state = &mut self.tir2_column_state;
        if !column_data.packages.is_empty() {
            state.selected[0] = state.selected[0].min(column_data.packages.len() - 1);
            let pkg = &column_data.packages[state.selected[0]];
            if !pkg.files.is_empty() {
                state.selected[1] = state.selected[1].min(pkg.files.len() - 1);
                let file = &pkg.files[state.selected[1]];
                if !file.items.is_empty() {
                    state.selected[2] = state.selected[2].min(file.items.len() - 1);
                } else {
                    state.selected[2] = 0;
                }
            } else {
                state.selected[1] = 0;
                state.selected[2] = 0;
            }
        } else {
            state.selected = [0; 3];
        }

        self.tir2_column_data = column_data;
    }

    /// Render an expression tree into detail lines for the TIR2 column browser.
    fn render_expr_to_lines(
        expr_id: baml_compiler2_ast::ExprId,
        body: &baml_compiler2_ast::ExprBody,
        inference: &baml_compiler2_tir::inference::ScopeInference,
        indent: usize,
        lines: &mut Vec<DetailLine>,
    ) {
        use baml_compiler2_ast::{Expr, Stmt};

        let pad = " ".repeat(indent);
        let ty_str = inference
            .expression_type(expr_id)
            .map(|t| t.to_string())
            .unwrap_or_else(|| "unknown".into());
        let expr = &body.exprs[expr_id];

        match expr {
            Expr::Block { stmts, tail_expr } => {
                let mut line = vec![DetailSpan::Code(format!("{pad}{{"))];
                line.push(DetailSpan::TypeAnnotation(format!(": {ty_str}")));
                lines.push(line);

                for stmt_id in stmts {
                    let stmt = &body.stmts[*stmt_id];
                    match stmt {
                        Stmt::Let {
                            pattern,
                            initializer,
                            ..
                        } => {
                            let pname = pat_desc(*pattern, body);
                            if let Some(init) = initializer {
                                let is_compound = matches!(
                                    &body.exprs[*init],
                                    Expr::Block { .. } | Expr::If { .. } | Expr::Match { .. }
                                );
                                if is_compound {
                                    let init_ty = inference
                                        .expression_type(*init)
                                        .map(|t| t.to_string())
                                        .unwrap_or_else(|| "unknown".into());
                                    let binding_ty =
                                        inference.binding_type(*pattern).map(|t| t.to_string());
                                    let mut line =
                                        vec![DetailSpan::Code(format!("{pad}  let {pname} ="))];
                                    line.push(DetailSpan::TypeAnnotation(format!(": {init_ty}")));
                                    if let Some(bt) = &binding_ty {
                                        if *bt != init_ty {
                                            line.push(DetailSpan::TypeAnnotation(format!(
                                                " -> {bt}"
                                            )));
                                        }
                                    }
                                    lines.push(line);
                                    Self::render_expr_to_lines(
                                        *init,
                                        body,
                                        inference,
                                        indent + 2,
                                        lines,
                                    );
                                } else {
                                    let binding_ty =
                                        inference.binding_type(*pattern).map(|t| t.to_string());
                                    let init_ty =
                                        inference.expression_type(*init).map(|t| t.to_string());
                                    let mut line =
                                        vec![DetailSpan::Code(format!("{pad}  let {pname} = "))];
                                    line.extend(expr_desc_spans(*init, body, inference));
                                    if let (Some(bt), Some(it)) = (&binding_ty, &init_ty) {
                                        if bt != it {
                                            line.push(DetailSpan::TypeAnnotation(format!(
                                                " -> {bt}"
                                            )));
                                        }
                                    }
                                    lines.push(line);
                                }
                            } else {
                                lines.push(plain(format!("{pad}  let {pname}")));
                            }
                        }
                        Stmt::Return(Some(e)) => {
                            if matches!(
                                &body.exprs[*e],
                                Expr::Block { .. } | Expr::If { .. } | Expr::Match { .. }
                            ) {
                                lines.push(plain(format!("{pad}  return")));
                                Self::render_expr_to_lines(*e, body, inference, indent + 2, lines);
                            } else {
                                let mut line = vec![DetailSpan::Code(format!("{pad}  return "))];
                                line.extend(expr_desc_spans(*e, body, inference));
                                lines.push(line);
                            }
                        }
                        Stmt::Return(None) => {
                            lines.push(plain(format!("{pad}  return")));
                        }
                        Stmt::Throw { value } => {
                            if matches!(
                                &body.exprs[*value],
                                Expr::Block { .. } | Expr::If { .. } | Expr::Match { .. }
                            ) {
                                lines.push(plain(format!("{pad}  throw")));
                                Self::render_expr_to_lines(
                                    *value,
                                    body,
                                    inference,
                                    indent + 2,
                                    lines,
                                );
                            } else {
                                let mut line = vec![DetailSpan::Code(format!("{pad}  throw "))];
                                line.extend(expr_desc_spans(*value, body, inference));
                                lines.push(line);
                            }
                        }
                        Stmt::Expr(e) => {
                            if matches!(
                                &body.exprs[*e],
                                Expr::Block { .. } | Expr::If { .. } | Expr::Match { .. }
                            ) {
                                Self::render_expr_to_lines(*e, body, inference, indent + 2, lines);
                            } else {
                                let mut line = vec![DetailSpan::Code(format!("{pad}  "))];
                                line.extend(expr_desc_spans(*e, body, inference));
                                lines.push(line);
                            }
                        }
                        Stmt::While {
                            condition,
                            body: wb,
                            ..
                        } => {
                            let mut line = vec![DetailSpan::Code(format!("{pad}  while ("))];
                            line.extend(expr_desc_spans(*condition, body, inference));
                            line.push(DetailSpan::Code(")".into()));
                            lines.push(line);
                            Self::render_expr_to_lines(*wb, body, inference, indent + 4, lines);
                        }
                        Stmt::Assign { target, value } => {
                            let mut line = vec![DetailSpan::Code(format!("{pad}  "))];
                            line.extend(expr_desc_spans(*target, body, inference));
                            line.push(DetailSpan::Code(" = ".into()));
                            line.extend(expr_desc_spans(*value, body, inference));
                            lines.push(line);
                        }
                        Stmt::AssignOp { target, op, value } => {
                            let mut line = vec![DetailSpan::Code(format!("{pad}  "))];
                            line.extend(expr_desc_spans(*target, body, inference));
                            line.push(DetailSpan::Code(format!(" {} ", assignop_sym(op))));
                            line.extend(expr_desc_spans(*value, body, inference));
                            lines.push(line);
                        }
                        Stmt::Assert { condition } => {
                            let mut line = vec![DetailSpan::Code(format!("{pad}  assert "))];
                            line.extend(expr_desc_spans(*condition, body, inference));
                            lines.push(line);
                        }
                        Stmt::Break => {
                            lines.push(plain(format!("{pad}  break")));
                        }
                        Stmt::Continue => {
                            lines.push(plain(format!("{pad}  continue")));
                        }
                        Stmt::HeaderComment { name, level } => {
                            let marker = "#".repeat(*level);
                            lines.push(plain(format!("{pad}  {marker} {name}")));
                        }
                        Stmt::Missing => {
                            lines.push(plain(format!("{pad}  <missing stmt>")));
                        }
                    }
                }
                if let Some(tail) = tail_expr {
                    Self::render_expr_to_lines(*tail, body, inference, indent + 2, lines);
                }
                lines.push(plain(format!("{pad}}}")));
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let mut line = vec![DetailSpan::Code(format!("{pad}if ("))];
                line.extend(expr_desc_spans(*condition, body, inference));
                line.push(DetailSpan::Code(")".into()));
                line.push(DetailSpan::TypeAnnotation(format!(": {ty_str}")));
                lines.push(line);
                Self::render_expr_to_lines(*then_branch, body, inference, indent + 2, lines);
                if let Some(eb) = else_branch {
                    lines.push(plain(format!("{pad}else")));
                    Self::render_expr_to_lines(*eb, body, inference, indent + 2, lines);
                }
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                let mut line = vec![DetailSpan::Code(format!("{pad}match ("))];
                line.extend(expr_desc_spans(*scrutinee, body, inference));
                line.push(DetailSpan::Code(")".into()));
                line.push(DetailSpan::TypeAnnotation(format!(": {ty_str}")));
                lines.push(line);
                for arm_id in arms {
                    let arm = &body.match_arms[*arm_id];
                    let p = pat_desc(arm.pattern, body);
                    lines.push(plain(format!("{pad}  {p} =>")));
                    Self::render_expr_to_lines(arm.body, body, inference, indent + 4, lines);
                }
            }
            _ => {
                let mut line = vec![DetailSpan::Code(pad)];
                line.extend(expr_desc_spans(expr_id, body, inference));
                lines.push(line);
            }
        }
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
        let _recursive_aliases = baml_compiler_tir::find_recursive_aliases(&type_aliases_map);
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
        let recursive_aliases = baml_compiler_tir::find_recursive_aliases(&type_aliases_map);
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

                    match lower_from_hir(
                        &body,
                        &inference_result,
                        &resolution_ctx,
                        &type_aliases_map,
                        &recursive_aliases,
                    ) {
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

    fn run_control_flow(&mut self) {
        use baml_compiler2_hir::body::FunctionBody;
        use baml_compiler2_visualization::control_flow::{
            build_control_flow_graph_from_ast, flatten_control_flow_graph,
        };

        let mut output = String::new();
        let mut output_annotated = Vec::new();

        // No typing context needed — we build CFG directly from the compiler2 AST,
        // bypassing type inference and VIR lowering entirely.

        let mut sorted_files: Vec<_> = self.source_files.iter().collect();
        sorted_files.sort_by_key(|(path, _)| path.as_path());

        for (path, source_file) in sorted_files {
            let file_path = path.display().to_string();
            let file_recomputed = self.modified_files.contains(path);

            let index = baml_compiler2_hir::file_semantic_index(&self.db, *source_file);
            let item_tree = &index.item_tree;

            // Sort functions by name for deterministic output.
            let mut functions: Vec<_> = item_tree.functions.iter().collect();
            functions.sort_by_key(|(_, f)| f.name.as_str());

            let mut file_has_output = false;

            for (local_id, func_data) in functions {
                let func_name = func_data.name.to_string();
                let func_loc =
                    baml_compiler2_hir::loc::FunctionLoc::new(&self.db, *source_file, *local_id);
                let body = baml_compiler2_hir::body::function_body(&self.db, func_loc);

                let status = if file_recomputed {
                    LineStatus::Recomputed
                } else {
                    LineStatus::Cached
                };

                // Build control flow graph directly from the compiler2 AST.
                // This survives parse and type errors thanks to Missing sentinels.
                let graph = match body.as_ref() {
                    FunctionBody::Expr(expr_body) => {
                        build_control_flow_graph_from_ast(&func_name, expr_body)
                    }
                    _ => continue,
                };

                if !file_has_output {
                    writeln!(output, "File: {file_path}").ok();
                    output_annotated.push((format!("File: {file_path}"), LineStatus::Unknown));
                    file_has_output = true;
                }

                // Raw graph
                let header = format!("--- {} (raw) ---", func_name);
                writeln!(output, "{}", header).ok();
                output_annotated.push((header, status));

                let graph_str = format!("{}", graph);
                for line in graph_str.lines() {
                    writeln!(output, "{}", line).ok();
                    output_annotated.push((line.to_string(), status));
                }

                // Flattened graph
                let flattened = flatten_control_flow_graph(&graph);
                let header = format!("--- {} (flattened) ---", func_name);
                writeln!(output, "{}", header).ok();
                output_annotated.push((header, status));

                let flat_str = format!("{}", flattened);
                for line in flat_str.lines() {
                    writeln!(output, "{}", line).ok();
                    output_annotated.push((line.to_string(), status));
                }

                writeln!(output).ok();
                output_annotated.push((String::new(), LineStatus::Unknown));
            }

            if file_has_output {
                writeln!(output).ok();
                output_annotated.push((String::new(), LineStatus::Unknown));
            }
        }

        self.phase_outputs
            .insert(CompilerPhase::ControlFlow, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::ControlFlow, output_annotated);
    }

    fn run_mir(&mut self) {
        use baml_compiler_hir::CompilerGenerated;

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
        let type_aliases_map = type_aliases(&self.db, self.project_root)
            .aliases(&self.db)
            .clone();
        let recursive_aliases = baml_compiler_tir::find_recursive_aliases(&type_aliases_map);

        // Build classes map (class name -> field name -> field index) for MIR lowering
        // Also build class type tags for TypeTag switch optimization
        let mut classes: HashMap<String, HashMap<String, usize>> = HashMap::new();
        let mut class_type_tags: HashMap<String, i64> = HashMap::new();
        let mut class_type_tag_counter = 0i64;
        // Build enums map (enum name -> variant name -> variant index) for MIR lowering
        let mut enums: HashMap<String, HashMap<String, usize>> = HashMap::new();
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
                    // Compute type tag for this class (CLASS_BASE + counter)
                    let type_tag = baml_type::typetag::CLASS_BASE + class_type_tag_counter;
                    class_type_tag_counter += 1;
                    class_type_tags.insert(class_name.clone(), type_tag);
                    classes.insert(class_name, field_indices);
                }
                if let ItemId::Enum(enum_loc) = item {
                    let enum_def = &item_tree[enum_loc.id(&self.db)];
                    let enum_name = enum_def.name.to_string();

                    let mut variant_indices = HashMap::new();
                    for (idx, variant) in enum_def.variants.iter().enumerate() {
                        variant_indices.insert(variant.name.to_string(), idx);
                    }
                    enums.insert(enum_name, variant_indices);
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

            let item_tree = baml_compiler_hir::file_item_tree(&self.db, *source_file);

            for item in items {
                if let ItemId::Function(func_id) = item {
                    let func = &item_tree[func_id.id(&self.db)];

                    // Skip compiler-generated functions (render_prompt, build_request, etc.)
                    if let Some(ref cg) = func.compiler_generated {
                        match cg {
                            CompilerGenerated::ClientResolve { .. }
                            | CompilerGenerated::LlmRenderPrompt { .. }
                            | CompilerGenerated::LlmBuildRequest { .. }
                            | CompilerGenerated::LlmCall { .. } => continue,
                        }
                    }

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
                        &type_aliases_map,
                        &recursive_aliases,
                    ) {
                        Ok(vir) => {
                            let mir = baml_compiler_mir::lower(
                                &signature,
                                &vir,
                                &self.db,
                                &classes,
                                &enums,
                                &class_type_tags,
                                &resolution_ctx,
                                &type_aliases_map,
                                &recursive_aliases,
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
        // Include both user files and builtin files so codegen can compile
        // builtin functions (e.g., baml.llm.render_prompt) that compiler-generated
        // functions call.
        let mut files: Vec<_> = self.source_files.values().copied().collect();
        files.extend(&self.builtin_files);

        let mut output = String::new();
        let mut output_annotated = Vec::new();

        let program = match baml_compiler_emit::compile_files(
            &self.db,
            &files,
            baml_compiler_emit::OptLevel::One,
            &baml_compiler_emit::CompileOptions {
                emit_test_cases: false,
            },
        ) {
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

                let bytecode_table = bex_vm::debug::display_program(
                    &[(func_name.to_string(), func)],
                    bex_vm::debug::BytecodeFormat::Textual,
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

        // Compile the program (include builtins so codegen can resolve builtin functions)
        let mut files: Vec<_> = self.source_files.values().copied().collect();
        files.extend(&self.builtin_files);
        let program = match baml_compiler_emit::compile_files(
            &self.db,
            &files,
            baml_compiler_emit::OptLevel::One,
            &baml_compiler_emit::CompileOptions {
                emit_test_cases: false,
            },
        ) {
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
        let program = match baml_compiler_emit::compile_files(
            &self.db,
            &files,
            baml_compiler_emit::OptLevel::One,
            &baml_compiler_emit::CompileOptions {
                emit_test_cases: false,
            },
        ) {
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
            Ok(VmExecState::SpanNotify(_)) => {
                // Span notifications are ignored in the VM Runner — just continue.
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

    fn run_formatter(&mut self) {
        let mut output = String::new();
        let mut output_annotated = Vec::new();

        // Sort files alphabetically by path
        let mut sorted_files: Vec<_> = self.source_files.iter().collect();
        sorted_files.sort_by_key(|(path, _)| path.as_path());

        for (path, source_file) in sorted_files {
            let file_path = path.display().to_string();
            let file_recomputed = self.modified_files.contains(path);

            writeln!(output, "File: {file_path}").ok();
            output_annotated.push((format!("File: {file_path}"), LineStatus::Unknown));

            // Format the source code using baml_fmt
            let format_options = baml_fmt::FormatOptions::default();
            match baml_fmt::format_salsa(&self.db, *source_file, &format_options) {
                Ok(formatted) => {
                    writeln!(output, "{}", formatted).ok();
                    let status = if file_recomputed {
                        LineStatus::Recomputed
                    } else {
                        LineStatus::Cached
                    };
                    for line in formatted.lines() {
                        output_annotated.push((line.to_string(), status));
                    }
                }
                Err(err) => {
                    let error_msg = match err {
                        baml_fmt::FormatterError::ParseErrors(e) => {
                            format!("Error parsing: {:?}", e)
                        }
                        baml_fmt::FormatterError::StrongAstError(e) => {
                            format!(
                                "Error formatting: {}",
                                e.print_with_file_context(path, source_file.text(&self.db))
                            )
                        }
                    };
                    writeln!(output, "{}", error_msg).ok();
                    output_annotated.push((error_msg, LineStatus::Recomputed));
                }
            }

            writeln!(output).ok();
            output_annotated.push((String::new(), LineStatus::Unknown));
        }

        self.phase_outputs.insert(CompilerPhase::Formatter, output);
        self.phase_outputs_annotated
            .insert(CompilerPhase::Formatter, output_annotated);
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

    pub(crate) fn hir2_column_data(&self) -> &Hir2ColumnData {
        &self.hir2_column_data
    }

    pub(crate) fn hir2_column_state(&self) -> &Hir2ColumnState {
        &self.hir2_column_state
    }

    pub(crate) fn hir2_column_state_mut(&mut self) -> &mut Hir2ColumnState {
        &mut self.hir2_column_state
    }

    pub(crate) fn tir2_column_data(&self) -> &Tir2ColumnData {
        &self.tir2_column_data
    }

    pub(crate) fn tir2_column_state(&self) -> &Tir2ColumnState {
        &self.tir2_column_state
    }

    pub(crate) fn tir2_column_state_mut(&mut self) -> &mut Tir2ColumnState {
        &mut self.tir2_column_state
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
    // Look for block expression or other expression types
    if let Some(block) = body
        .syntax()
        .children()
        .find_map(baml_compiler_syntax::ast::BlockExpr::cast)
    {
        write_indent(output, indent);
        writeln!(output, "EXPR_BLOCK").ok();
        format_block_expr(&block, output, indent + 1);
    } else if let Some(expr_node) = body.syntax().children().next() {
        format_expr_node(&expr_node, output, indent);
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

    for element in block.elements() {
        match element {
            BlockElement::Stmt(node) => {
                if let Some(let_stmt) = LetStmt::cast(node.clone()) {
                    format_let_stmt(&let_stmt, output, indent);
                } else if let Some(return_stmt) = ReturnStmt::cast(node.clone()) {
                    format_return_stmt(&return_stmt, output, indent);
                } else if let Some(throw_stmt) = ThrowStmt::cast(node.clone()) {
                    format_throw_stmt(&throw_stmt, output, indent);
                } else {
                    write_indent(output, indent);
                    writeln!(output, "STMT {}", node.text().to_string().trim()).ok();
                }
            }
            BlockElement::ExprNode(node) => {
                format_expr_node(&node, output, indent);
            }
            BlockElement::ExprToken(token) => {
                write_indent(output, indent);
                writeln!(output, "EXPR {}", token.text()).ok();
            }
            BlockElement::HeaderComment(_) => {}
        }
    }
}

fn format_let_stmt(stmt: &baml_compiler_syntax::ast::LetStmt, output: &mut String, indent: usize) {
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
    if let Some(initializer) = stmt.initializer() {
        write_indent(output, indent + 1);
        writeln!(output, "VALUE").ok();
        format_expr_node(&initializer, output, indent + 2);
    } else if let Some(initializer_token) = stmt.initializer_token() {
        write_indent(output, indent + 1);
        writeln!(output, "VALUE").ok();
        write_indent(output, indent + 2);
        writeln!(output, "EXPR {}", initializer_token.text()).ok();
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
        .find(|n| n.kind() != baml_compiler_syntax::SyntaxKind::BLOCK_EXPR)
    {
        format_expr_node(&cond, output, indent + 2);
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

fn format_expr_node(expr_node: &SyntaxNode, output: &mut String, indent: usize) {
    use baml_compiler_syntax::ast::*;

    if let Some(catch_expr) = CatchExpr::cast(expr_node.clone()) {
        format_catch_expr(&catch_expr, output, indent);
        return;
    }

    if let Some(throw_expr) = ThrowExpr::cast(expr_node.clone()) {
        format_throw_expr(&throw_expr, output, indent);
        return;
    }

    if let Some(if_expr) = IfExpr::cast(expr_node.clone()) {
        format_if_expr(&if_expr, output, indent);
        return;
    }

    if let Some(block_expr) = BlockExpr::cast(expr_node.clone()) {
        write_indent(output, indent);
        writeln!(output, "EXPR_BLOCK").ok();
        format_block_expr(&block_expr, output, indent + 1);
        return;
    }

    if let Some(expr) = Expr::cast(expr_node.clone()) {
        format_expr(&expr, output, indent);
        return;
    }

    let text = expr_node.text().to_string();
    if text.len() < 40 && !text.contains('\n') {
        write_indent(output, indent);
        writeln!(output, "EXPR {}", text.trim()).ok();
    } else {
        write_indent(output, indent);
        writeln!(output, "EXPR").ok();
        write_indent(output, indent + 1);
        writeln!(output, "{}", text.trim()).ok();
    }
}

fn format_return_stmt(
    stmt: &baml_compiler_syntax::ast::ReturnStmt,
    output: &mut String,
    indent: usize,
) {
    write_indent(output, indent);
    writeln!(output, "STMT_RETURN").ok();

    if let Some(value) = stmt.value() {
        write_indent(output, indent + 1);
        writeln!(output, "VALUE").ok();
        format_expr_node(&value, output, indent + 2);
        return;
    }

    let text = stmt.syntax().text().to_string();
    if let Some((_, value_text)) = text.split_once("return") {
        let value = value_text.trim().trim_end_matches(';').trim();
        if !value.is_empty() {
            write_indent(output, indent + 1);
            writeln!(output, "VALUE").ok();
            write_indent(output, indent + 2);
            writeln!(output, "EXPR {}", value).ok();
        }
    }
}

fn format_throw_stmt(
    stmt: &baml_compiler_syntax::ast::ThrowStmt,
    output: &mut String,
    indent: usize,
) {
    write_indent(output, indent);
    writeln!(output, "STMT_THROW").ok();

    if let Some(expr) = stmt.expr() {
        format_throw_expr(&expr, output, indent + 1);
    }
}

fn format_throw_expr(
    expr: &baml_compiler_syntax::ast::ThrowExpr,
    output: &mut String,
    indent: usize,
) {
    write_indent(output, indent);
    writeln!(output, "EXPR_THROW").ok();

    if let Some(value) = expr.value() {
        write_indent(output, indent + 1);
        writeln!(output, "VALUE").ok();
        format_expr_node(&value, output, indent + 2);
        return;
    }

    let text = expr.syntax().text().to_string();
    if let Some((_, value_text)) = text.split_once("throw") {
        let value = value_text.trim();
        if !value.is_empty() {
            write_indent(output, indent + 1);
            writeln!(output, "VALUE").ok();
            write_indent(output, indent + 2);
            writeln!(output, "EXPR {}", value).ok();
        }
    }
}

fn format_catch_expr(
    catch_expr: &baml_compiler_syntax::ast::CatchExpr,
    output: &mut String,
    indent: usize,
) {
    write_indent(output, indent);
    writeln!(output, "EXPR_CATCH").ok();

    if let Some(base) = catch_expr.base() {
        write_indent(output, indent + 1);
        writeln!(output, "BASE").ok();
        format_expr_node(&base, output, indent + 2);
    }

    for clause in catch_expr.clauses() {
        format_catch_clause(&clause, output, indent + 1);
    }
}

fn format_catch_clause(
    clause: &baml_compiler_syntax::ast::CatchClause,
    output: &mut String,
    indent: usize,
) {
    write_indent(output, indent);
    writeln!(output, "CATCH_CLAUSE").ok();

    if let Some(keyword) = clause.keyword() {
        write_indent(output, indent + 1);
        writeln!(output, "KIND {}", keyword.text()).ok();
    }

    if let Some(binding) = clause.binding() {
        write_indent(output, indent + 1);
        writeln!(
            output,
            "BINDING {}",
            binding.syntax().text().to_string().trim()
        )
        .ok();
    }

    let arms: Vec<_> = clause.arms().collect();
    if !arms.is_empty() {
        write_indent(output, indent + 1);
        writeln!(output, "ARMS").ok();
        for arm in arms {
            format_catch_arm(&arm, output, indent + 2);
        }
    }
}

fn format_catch_arm(arm: &baml_compiler_syntax::ast::CatchArm, output: &mut String, indent: usize) {
    write_indent(output, indent);
    writeln!(output, "CATCH_ARM").ok();

    if let Some(pattern) = arm.pattern() {
        write_indent(output, indent + 1);
        writeln!(
            output,
            "PATTERN {}",
            pattern.syntax().text().to_string().trim()
        )
        .ok();
    }

    write_indent(output, indent + 1);
    writeln!(output, "BODY").ok();
    if let Some(body) = arm.body() {
        format_expr_node(&body, output, indent + 2);
    } else if let Some(body_text) = extract_arm_body_text(arm.syntax()) {
        write_indent(output, indent + 2);
        writeln!(output, "EXPR {}", body_text).ok();
    }
}

fn extract_arm_body_text(arm_syntax: &SyntaxNode) -> Option<String> {
    let text = arm_syntax.text().to_string();
    let (_, rhs) = text.split_once("=>")?;
    let body = rhs.trim().trim_end_matches(',').trim();
    if body.is_empty() {
        None
    } else {
        Some(body.to_string())
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
                            .fields
                            .iter()
                            .zip(inst.fields.iter())
                            .map(|(f, val)| format!("{}: {}", f.name, format_vm_value(val, vm)))
                            .collect();
                        format!("{}{{ {} }}", class.name, fields.join(", "))
                    } else {
                        "<instance>".to_string()
                    }
                }
                Object::Variant(var) => {
                    let enum_obj = vm.get_object(var.enm);
                    if let Object::Enum(enm) = enum_obj {
                        if let Some(v) = enm.variants.get(var.index) {
                            format!("{}::{}", enm.name, v.name)
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
                Object::Collector(_) => "<collector>".to_string(),
                Object::Type(ty) => format!("<type: {ty}>"),
                #[cfg(feature = "heap_debug")]
                Object::Sentinel(_) => "<sentinel>".to_string(),
            }
        }
    }
}
