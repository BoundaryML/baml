//! Function bodies - either LLM prompts or expression IR.
//!
//! The CST already distinguishes `LLM_FUNCTION_BODY` from `EXPR_FUNCTION_BODY`,
//! so we just need to lower each type appropriately.

use std::sync::Arc;

use baml_base::{FileId, Span};
use baml_compiler_diagnostics::HirDiagnostic;
use baml_compiler_syntax::TypeExpr;
use la_arena::{Arena, Idx};
use rowan::{TextRange, TextSize, ast::AstNode};

use crate::{Name, source_map::HirSourceMap, type_ref::TypeRef};

/// Create a `PrimitiveClient` body for clients with no options block.
///
/// Returns: `baml.llm.build_primitive_client(name, provider, default_role, allowed_roles, {})`
pub fn empty_primitive_client_body(
    _file_id: FileId,
    client_name: &str,
    provider: &str,
    default_role: &str,
    allowed_roles: &[String],
) -> (ExprBody, HirSourceMap) {
    use crate::Name;
    let mut exprs: Arena<Expr> = Arena::new();
    let source_map = HirSourceMap::new();

    // Create empty options map
    let options_map_expr = exprs.alloc(Expr::Map { entries: vec![] });

    // Create string literal arguments
    let name_expr = exprs.alloc(Expr::Literal(Literal::String(client_name.to_string())));
    let provider_expr = exprs.alloc(Expr::Literal(Literal::String(provider.to_string())));
    let default_role_expr = exprs.alloc(Expr::Literal(Literal::String(default_role.to_string())));

    // Create allowed_roles array
    let role_elements: Vec<_> = allowed_roles
        .iter()
        .map(|role| exprs.alloc(Expr::Literal(Literal::String(role.clone()))))
        .collect();
    let allowed_roles_expr = exprs.alloc(Expr::Array {
        elements: role_elements,
    });

    // Create the function call path: baml.llm.build_primitive_client
    let callee_expr = exprs.alloc(Expr::Path(vec![
        Name::new("baml"),
        Name::new("llm"),
        Name::new("build_primitive_client"),
    ]));

    // Create the call expression
    let call_expr = exprs.alloc(Expr::Call {
        callee: callee_expr,
        args: vec![
            name_expr,
            provider_expr,
            default_role_expr,
            allowed_roles_expr,
            options_map_expr,
        ],
    });

    let body = ExprBody {
        exprs,
        stmts: Arena::new(),
        patterns: Arena::new(),
        match_arms: Arena::new(),
        types: Arena::new(),
        root_expr: Some(call_expr),
        diagnostics: Vec::new(),
    };

    (body, source_map)
}

/// Create a synthetic body for LLM functions that calls `baml.llm.call_llm_function`.
///
/// For an LLM function like:
/// ```baml
/// function Greet(name: string) -> string {
///     client TestClient
///     prompt #"Hello, {{ name }}!"#
/// }
/// ```
///
/// Generates a body equivalent to:
/// ```baml
/// baml.llm.call_llm_function("Greet", {"name": name})
/// ```
pub fn lower_llm_to_call_llm_function(
    function_name: &str,
    param_names: &[Name],
) -> (ExprBody, HirSourceMap) {
    use crate::Name;
    let mut exprs: Arena<Expr> = Arena::new();
    let source_map = HirSourceMap::new();

    // Create the args map: {"param1": param1, "param2": param2, ...}
    let entries: Vec<(ExprId, ExprId)> = param_names
        .iter()
        .map(|name| {
            let key = exprs.alloc(Expr::Literal(Literal::String(name.to_string())));
            let value = exprs.alloc(Expr::Path(vec![name.clone()]));
            (key, value)
        })
        .collect();
    let args_map = exprs.alloc(Expr::Map { entries });

    // Create function name literal
    let fn_name_expr = exprs.alloc(Expr::Literal(Literal::String(function_name.to_string())));

    // Create the function call path: baml.llm.call_llm_function
    let callee_expr = exprs.alloc(Expr::Path(vec![
        Name::new("baml"),
        Name::new("llm"),
        Name::new("call_llm_function"),
    ]));

    // Create the call expression
    let call_expr = exprs.alloc(Expr::Call {
        callee: callee_expr,
        args: vec![fn_name_expr, args_map],
    });

    let body = ExprBody {
        exprs,
        stmts: Arena::new(),
        patterns: Arena::new(),
        match_arms: Arena::new(),
        types: Arena::new(),
        root_expr: Some(call_expr),
        diagnostics: Vec::new(),
    };

    (body, source_map)
}

pub fn strip_string_delimiters(text: &str) -> &str {
    let text = text.trim();
    if text.starts_with("#\"") && text.ends_with("\"#") {
        &text[2..text.len() - 2]
    } else if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
        &text[1..text.len() - 1]
    } else {
        text
    }
}

/// The body of a function - determined by CST node type.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum FunctionBody {
    /// LLM function: has `LLM_FUNCTION_BODY` in CST.
    Llm(LlmBody),

    /// Expression function: has `EXPR_FUNCTION_BODY` in CST.
    /// Contains both the position-independent body and the source map for spans.
    Expr(ExprBody, HirSourceMap),

    /// Function has no body (error recovery)
    Missing,
}

/// Body of an LLM function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmBody {
    /// The client to use (e.g., "GPT4")
    pub client: Name,

    /// The prompt template
    pub prompt: PromptTemplate,
}

/// A prompt template with interpolations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptTemplate {
    /// The raw prompt string (may contain {{ }} interpolations)
    pub text: String,

    /// Parsed interpolation expressions
    pub interpolations: Vec<Interpolation>,
}

impl PromptTemplate {
    #[allow(clippy::cast_possible_truncation)]
    /// Parse a prompt template from a raw string literal.
    ///
    /// Note: This does not store file offsets. To get the template's file offset
    /// for diagnostic rendering, use `get_file_offset()` or look it up from the CST.
    pub fn from_raw_string(raw_string: &baml_compiler_syntax::ast::RawStringLiteral) -> Self {
        use baml_compiler_syntax::ast::{JinjaExpression, JinjaStatement, PromptText};

        let mut text = String::new();
        let mut interpolations = Vec::new();
        let mut current_offset = 0u32;

        // Iterate through the children of the raw string in order
        for child in raw_string.syntax().children() {
            match child.kind() {
                baml_compiler_syntax::SyntaxKind::PROMPT_TEXT => {
                    // Plain text - add directly to output
                    if let Some(prompt_text) = PromptText::cast(child.clone()) {
                        let content = prompt_text.text();
                        text.push_str(&content);
                        current_offset += content.len() as u32;
                    }
                }
                baml_compiler_syntax::SyntaxKind::TEMPLATE_INTERPOLATION => {
                    // Jinja expression {{ ... }}
                    if let Some(jinja_expr) = JinjaExpression::cast(child.clone()) {
                        let inner = jinja_expr.inner_text();
                        let full_text = jinja_expr.full_text();

                        // Store the expression text for later validation by minijinja
                        interpolations.push(Interpolation {
                            expr_text: inner.clone(),
                            offset: current_offset,
                            length: full_text.len() as u32,
                        });

                        // Keep the {{ }} in the text for now (will be replaced at runtime)
                        let placeholder = full_text;
                        text.push_str(&placeholder);
                        current_offset += placeholder.len() as u32;
                    }
                }
                baml_compiler_syntax::SyntaxKind::TEMPLATE_CONTROL => {
                    // Jinja statement {% ... %} - keep in text as-is for minijinja to evaluate
                    if let Some(jinja_stmt) = JinjaStatement::cast(child.clone()) {
                        let content = jinja_stmt.full_text();
                        text.push_str(&content);
                        current_offset += content.len() as u32;
                    }
                }
                baml_compiler_syntax::SyntaxKind::TEMPLATE_COMMENT => {
                    // Keep Jinja comments in text so byte offsets stay aligned with file positions.
                    // Minijinja handles {# ... #} natively and ignores them during evaluation.
                    if let Some(jinja_comment) =
                        baml_compiler_syntax::ast::JinjaComment::cast(child.clone())
                    {
                        let content = jinja_comment.full_text();
                        text.push_str(&content);
                        current_offset += content.len() as u32;
                    }
                }
                _ => {
                    // Other tokens (delimiters, etc.) - skip
                }
            }
        }

        PromptTemplate {
            text,
            interpolations,
        }
    }

    /// Get the file offset where the template text starts from a raw string literal.
    ///
    /// This is used at diagnostic rendering time to convert relative Jinja error
    /// offsets to absolute file positions.
    pub fn get_file_offset(raw_string: &baml_compiler_syntax::ast::RawStringLiteral) -> u32 {
        // The raw string syntax is: #"..."# where the content starts after #"
        let raw_string_start = raw_string.syntax().text_range().start();
        // Skip the #" prefix (2 characters)
        u32::from(raw_string_start) + 2
    }
}

/// A {{ expr }} interpolation in a prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interpolation {
    /// The raw expression text inside the {{ }} delimiters.
    /// This will be parsed by minijinja during Jinja validation.
    pub expr_text: String,

    /// Source offset in the prompt string (points to the opening `{{`)
    pub offset: u32,

    /// Length of the full interpolation including delimiters (e.g., `{{ foo }}`)
    pub length: u32,
}

/// Body of an expression function (turing-complete).
///
/// This structure is position-independent - it contains no span information.
/// Spans are stored separately in `HirSourceMap` to enable incremental compilation
/// (whitespace changes don't invalidate type checking).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprBody {
    /// Expression arena
    pub exprs: Arena<Expr>,

    /// Statement arena
    pub stmts: Arena<Stmt>,

    /// Pattern arena (for let bindings, match arms, etc.)
    pub patterns: Arena<Pattern>,

    /// Match arm arena
    pub match_arms: Arena<MatchArm>,

    /// Type annotation arena (for let bindings, etc.)
    pub types: Arena<crate::type_ref::TypeRef>,

    /// Root expression of the function body (usually a `BLOCK_EXPR`)
    pub root_expr: Option<ExprId>,

    /// HIR diagnostics collected during lowering (e.g., missing semicolons).
    pub diagnostics: Vec<HirDiagnostic>,
}

/// Span information for a single match arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchArmSpans {
    /// Span of the entire arm (pattern + guard + body)
    pub arm_span: Span,
    /// Span of just the pattern
    pub pattern_span: Span,
}

// IDs for arena indices
pub type ExprId = Idx<Expr>;
pub type StmtId = Idx<Stmt>;
pub type PatId = Idx<Pattern>;
pub type MatchArmId = Idx<MatchArm>;
/// ID for any syntactic occurrence of a type (annotations, generic arguments, etc.)
pub type TypeId = Idx<crate::type_ref::TypeRef>;

/// A spread element in an object constructor: `...expr`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpreadField {
    /// The expression being spread
    pub expr: ExprId,
    /// Position index where this spread appears among all elements
    /// Used to determine override order (later positions override earlier)
    pub position: usize,
}

/// Expressions in BAML function bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// Literal values
    Literal(Literal),

    /// Path expression with one or more segments.
    /// Single segment: `x`, `GPT4`
    /// Multi-segment: `user.name`, `baml.image.from_url`, `Status.Active`
    /// Resolution to determine if this is a local variable, field access,
    /// enum variant, or module item happens in THIR.
    Path(Vec<Name>),

    /// If expression
    If {
        condition: ExprId,
        then_branch: ExprId,
        else_branch: Option<ExprId>,
    },

    /// Match expression: `match (scrutinee) { arm1, arm2, ... }`
    Match {
        scrutinee: ExprId,
        arms: Vec<MatchArmId>,
    },

    /// Binary operation
    Binary {
        op: BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
    },

    /// Unary operation
    Unary { op: UnaryOp, expr: ExprId },

    /// Function call: `call_f1()`, `transform(user)`
    Call { callee: ExprId, args: Vec<ExprId> },

    /// Object constructor: `Point { x: 1, y: 2, ...spread }`
    Object {
        type_name: Option<Name>,
        fields: Vec<(Name, ExprId)>,
        /// Spread elements with their positions for override semantics
        spreads: Vec<SpreadField>,
    },

    /// Array constructor: `[1, 2, 3]`
    Array { elements: Vec<ExprId> },

    /// Map literal: `{ "key": value, ... }` or `{ key value, ... }`
    Map { entries: Vec<(ExprId, ExprId)> },

    /// Block expression: `{ stmt1; stmt2; expr }`
    Block {
        stmts: Vec<StmtId>,
        tail_expr: Option<ExprId>,
    },

    /// Field access on a complex expression: `f().field`, `arr[0].field`, `(a + b).x`
    ///
    /// Used when the base is a computed value (call result, index result, etc.),
    /// NOT a simple identifier chain.
    ///
    /// For simple identifier chains like `user.name.length`, use `Path` instead.
    /// The distinction is:
    /// - `Path(vec!["user", "name"])` - might be variable + field, enum variant, or module path
    /// - `FieldAccess { base, field }` - definitely a field access on a computed value
    FieldAccess { base: ExprId, field: Name },

    /// Index access: `array[0]`, `map[key]`
    Index { base: ExprId, index: ExprId },

    /// Missing/error expression
    Missing,
}

/// Statements in BAML function bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    /// Expression statement: `call_f1();`
    Expr(ExprId),

    /// Let binding: `let x = call_f3();`
    /// If `is_watched` is true, this is a `watch let` that tracks variable changes.
    Let {
        pattern: PatId,
        type_annotation: Option<TypeId>,
        initializer: Option<ExprId>,
        is_watched: bool,
    },

    /// While loop: `while (condition) { body }`
    ///
    /// The `origin` field tracks whether this loop was written directly
    /// by the user or desugared from a for loop.
    ///
    /// The optional `after` statement runs after each iteration (including on `continue`).
    /// This is used to desugar C-style for loops: `for (init; cond; update)`.
    While {
        condition: ExprId,
        body: ExprId,
        /// Optional statement to run after each iteration (for C-style for loops' update).
        after: Option<StmtId>,
        origin: LoopOrigin,
    },

    /// Return statement: `return "minor";`
    Return(Option<ExprId>),

    /// Break statement: `break;`
    Break,

    /// Continue statement: `continue;`
    Continue,

    /// Simple assignment: `a = expr;`
    Assign { target: ExprId, value: ExprId },

    /// Compound assignment: `a += expr;`
    AssignOp {
        target: ExprId,
        op: AssignOp,
        value: ExprId,
    },

    /// Assert statement: `assert condition;`
    Assert { condition: ExprId },

    /// Missing/error statement
    Missing,

    /// Header comment notification: `//# name`
    /// Emits a block notification when executed.
    HeaderComment {
        /// The name of the block annotation
        name: Name,
        /// The header level (number of # symbols)
        level: usize,
    },
}

/// Indicates where a loop construct originated from.
///
/// This is used to provide better error messages for desugared constructs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoopOrigin {
    /// User wrote a `while` loop directly
    #[default]
    While,
    /// Desugared from a `for-in` loop
    ForLoop,
}

/// Compound assignment operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Add,    // +=
    Sub,    // -=
    Mul,    // *=
    Div,    // /=
    Mod,    // %=
    BitAnd, // &=
    BitOr,  // |=
    BitXor, // ^=
    Shl,    // <<=
    Shr,    // >>=
}

/// Patterns for let bindings and match arms.
///
/// Following BEP-002, patterns can be:
/// - Simple bindings: `x`, `_` (wildcard is semantically dropped later)
/// - Typed bindings: `s: Success`
/// - Literals: `null`, `true`, `42`, `"hello"`
/// - Enum variants: `Status.Active`
/// - Unions: `200 | 201` or `Status.Active | Status.Pending`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    /// Simple binding pattern: `x`, `user`, `_`
    /// Note: `_` is parsed as a regular identifier; semantic analysis
    /// treats it as a wildcard/discard.
    Binding(Name),

    /// Typed binding pattern: `s: Success`, `n: int`
    TypedBinding {
        name: Name,
        ty: crate::type_ref::TypeRef,
    },

    /// Literal pattern: `null`, `true`, `false`, `42`, `3.14`, `"hello"`
    Literal(Literal),

    /// Enum variant pattern: `Status.Active`
    EnumVariant { enum_name: Name, variant: Name },

    /// Union pattern: `200 | 201 | 204` or `Status.Active | Status.Pending`
    /// Only literals and enum variants can be unioned (not arbitrary bindings).
    Union(Vec<PatId>),
}

/// A single arm in a match expression.
///
/// Grammar: `pattern guard? '=>' arm_body`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    /// The pattern to match against
    pub pattern: PatId,

    /// Optional guard: `if condition`
    /// Note: Guards do NOT contribute to exhaustiveness checking.
    pub guard: Option<ExprId>,

    /// The body expression (result if this arm matches)
    pub body: ExprId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Literal {
    String(String),
    Int(i64),
    Float(String),
    Bool(bool),
    Null,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,

    // Comparison
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,

    // Logical
    And,
    Or,

    // Bitwise
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,

    // Type checking
    Instanceof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
}

impl FunctionBody {
    /// Lower a function body from CST to HIR.
    ///
    /// The CST already tells us if it's LLM or Expr via node type!
    ///
    /// # Arguments
    /// - `func_node`: The function definition AST node
    /// - `file_id`: The file ID for span tracking
    pub fn lower(
        func_node: &baml_compiler_syntax::ast::FunctionDef,
        file_id: FileId,
    ) -> Arc<FunctionBody> {
        // Collect parameter names to add to scope so gensym avoids them
        let param_names: Vec<String> = func_node
            .param_list()
            .map(|pl| {
                pl.params()
                    .filter_map(|p| p.name().map(|n| n.text().to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // Check which body type we have
        if let Some(llm_body) = func_node.llm_body() {
            Arc::new(Self::lower_llm_body(&llm_body))
        } else if let Some(expr_body) = func_node.expr_body() {
            let (body, source_map) = Self::lower_expr_body(&expr_body, file_id, &param_names);
            Arc::new(FunctionBody::Expr(body, source_map))
        } else {
            Arc::new(FunctionBody::Missing)
        }
    }

    fn lower_llm_body(llm_body: &baml_compiler_syntax::ast::LlmFunctionBody) -> FunctionBody {
        // Extract client name using AST accessor
        // Use value() to handle both identifier (`client Foo`) and string (`client "openai/gpt-4o"`) forms
        let client = llm_body
            .client_field()
            .and_then(|cf| cf.value())
            .map(|name| Name::new(&name));

        // Extract prompt using AST accessor
        let prompt = llm_body
            .prompt_field()
            .and_then(|pf| pf.raw_string())
            .map(|raw_str| Self::parse_prompt(&raw_str));

        if let (Some(client), Some(prompt)) = (client, prompt) {
            FunctionBody::Llm(LlmBody { client, prompt })
        } else {
            // TODO: Better would be to error here, with a new FunctionBody::Invalid
            // that has errors in it.
            FunctionBody::Missing
        }
    }

    fn parse_prompt(raw_string: &baml_compiler_syntax::ast::RawStringLiteral) -> PromptTemplate {
        PromptTemplate::from_raw_string(raw_string)
    }

    fn lower_expr_body(
        expr_body: &baml_compiler_syntax::ast::ExprFunctionBody,
        file_id: FileId,
        param_names: &[String],
    ) -> (ExprBody, HirSourceMap) {
        let mut ctx = LoweringContext::new(file_id);

        // Add function parameters to scope so gensym avoids them
        for name in param_names {
            ctx.add_name_to_scope(name);
        }

        // The EXPR_FUNCTION_BODY contains a BLOCK_EXPR as its child
        // which contains all the statements and expressions
        let root_expr = expr_body
            .syntax()
            .children()
            .find_map(baml_compiler_syntax::ast::BlockExpr::cast)
            .map(|block| ctx.lower_block_expr(&block));

        ctx.finish(root_expr)
    }

    /// Lower a client options block to a `PrimitiveClient` expression.
    ///
    /// Used for client `.resolve` functions. Takes the options config block
    /// and client metadata, and creates a `FunctionBody` that returns:
    /// ```baml
    /// baml.llm.build_primitive_client(name, provider, default_role, allowed_roles, options_map)
    /// ```
    pub fn lower_client_options_to_primitive_client(
        config_block: &baml_compiler_syntax::ast::ConfigBlock,
        file_id: FileId,
        client_name: &str,
        provider: &str,
        default_role: &str,
        allowed_roles: &[String],
    ) -> (ExprBody, HirSourceMap) {
        use crate::Name;
        let mut ctx = LoweringContext::new(file_id);
        let range = config_block.syntax().text_range();

        // Build the options map expression
        let options_map_expr = ctx.lower_config_block_to_map_expr(config_block);

        // Create string literal arguments
        let name_expr = ctx.alloc_expr(
            Expr::Literal(Literal::String(client_name.to_string())),
            range,
        );
        let provider_expr =
            ctx.alloc_expr(Expr::Literal(Literal::String(provider.to_string())), range);
        let default_role_expr = ctx.alloc_expr(
            Expr::Literal(Literal::String(default_role.to_string())),
            range,
        );

        // Create allowed_roles array
        let role_elements: Vec<ExprId> = allowed_roles
            .iter()
            .map(|role| ctx.alloc_expr(Expr::Literal(Literal::String(role.clone())), range))
            .collect();
        let allowed_roles_expr = ctx.alloc_expr(
            Expr::Array {
                elements: role_elements,
            },
            range,
        );

        // Create the function call path: baml.llm.build_primitive_client
        let callee_expr = ctx.alloc_expr(
            Expr::Path(vec![
                Name::new("baml"),
                Name::new("llm"),
                Name::new("build_primitive_client"),
            ]),
            range,
        );

        // Create the call expression
        let call_expr = ctx.alloc_expr(
            Expr::Call {
                callee: callee_expr,
                args: vec![
                    name_expr,
                    provider_expr,
                    default_role_expr,
                    allowed_roles_expr,
                    options_map_expr,
                ],
            },
            range,
        );

        ctx.finish(Some(call_expr))
    }
}

struct LoweringContext {
    exprs: Arena<Expr>,
    stmts: Arena<Stmt>,
    patterns: Arena<Pattern>,
    match_arms: Arena<MatchArm>,
    types: Arena<crate::type_ref::TypeRef>,
    /// File ID for creating spans
    file_id: FileId,
    /// All names used in this function, for generating unique synthetic variable names.
    names_in_scope: std::collections::HashSet<String>,

    /// Source map for tracking spans (separate from `ExprBody` for incrementality)
    source_map: HirSourceMap,

    /// HIR diagnostics collected during lowering.
    diagnostics: Vec<HirDiagnostic>,
}

/// Helper enum for building pattern elements during lowering.
/// Used to track partial state while scanning tokens in a pattern.
enum PatternElement {
    /// Simple identifier (could become binding or enum start)
    /// Stores (name, `start_position`) for span tracking
    Ident(Name, TextSize),
    /// Seen `EnumName.` - waiting for variant name
    /// Stores (`enum_name`, `start_position`) for span tracking
    EnumStart(Name, TextSize),
    /// Seen `name:` - waiting for type expression
    /// Stores (name, `start_position`) for span tracking
    TypedBindingStart(Name, TextSize),
}

impl LoweringContext {
    fn new(file_id: FileId) -> Self {
        Self {
            exprs: Arena::new(),
            stmts: Arena::new(),
            patterns: Arena::new(),
            match_arms: Arena::new(),
            types: Arena::new(),
            file_id,
            names_in_scope: std::collections::HashSet::new(),
            source_map: HirSourceMap::new(),
            diagnostics: Vec::new(),
        }
    }

    /// Push a diagnostic to be reported.
    fn push_diagnostic(&mut self, diagnostic: HirDiagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Create a span from a syntax node's text range.
    fn span_from_node(&self, node: &baml_compiler_syntax::SyntaxNode) -> Span {
        Span::new(self.file_id, node.text_range())
    }

    /// Find the start position of the first non-trivia token in a syntax node.
    /// Falls back to the node's start position if no non-trivia token is found.
    fn first_significant_start(node: &baml_compiler_syntax::SyntaxNode) -> TextSize {
        for element in node.descendants_with_tokens() {
            if let Some(token) = element.as_token() {
                if !token.kind().is_trivia() {
                    return token.text_range().start();
                }
            }
        }
        node.text_range().start()
    }

    /// Create a span from a syntax node, but skip any leading trivia (whitespace, newlines, comments).
    /// This is useful for error spans that should point to the actual code, not preceding whitespace.
    fn span_from_node_skip_trivia(&self, node: &baml_compiler_syntax::SyntaxNode) -> Span {
        let range = TextRange::new(Self::first_significant_start(node), node.text_range().end());
        Span::new(self.file_id, range)
    }

    /// Create a span from a text range.
    fn span_from_range(&self, range: TextRange) -> Span {
        Span::new(self.file_id, range)
    }

    /// Get the text range of a syntax node, skipping any leading trivia (whitespace, newlines, comments).
    /// This is useful for synthetic expressions that should point to the actual code, not preceding whitespace.
    fn text_range_skip_trivia(node: &baml_compiler_syntax::SyntaxNode) -> TextRange {
        TextRange::new(Self::first_significant_start(node), node.text_range().end())
    }

    fn alloc_expr(&mut self, expr: Expr, range: TextRange) -> ExprId {
        let id = self.exprs.alloc(expr);
        self.source_map.insert_expr(id, self.span_from_range(range));
        id
    }

    fn alloc_stmt(&mut self, stmt: Stmt, range: TextRange) -> StmtId {
        let id = self.stmts.alloc(stmt);
        self.source_map.insert_stmt(id, self.span_from_range(range));
        id
    }

    fn alloc_pattern(&mut self, pattern: Pattern, range: TextRange) -> PatId {
        let id = self.patterns.alloc(pattern);
        self.source_map
            .insert_pattern(id, self.span_from_range(range));
        id
    }

    fn alloc_match_arm(&mut self, arm: MatchArm, spans: MatchArmSpans) -> MatchArmId {
        let id = self.match_arms.alloc(arm);
        self.source_map.insert_match_arm(id, spans);
        id
    }

    fn alloc_type(&mut self, type_ref: crate::type_ref::TypeRef, range: TextRange) -> TypeId {
        let id = self.types.alloc(type_ref);
        self.source_map.insert_type(id, self.span_from_range(range));
        id
    }

    fn finish(self, root_expr: Option<ExprId>) -> (ExprBody, HirSourceMap) {
        let body = ExprBody {
            exprs: self.exprs,
            stmts: self.stmts,
            patterns: self.patterns,
            match_arms: self.match_arms,
            types: self.types,
            root_expr,
            diagnostics: self.diagnostics,
        };
        (body, self.source_map)
    }

    /// Generate a unique variable name for desugaring.
    ///
    /// Tries readable names first, then adds numeric suffixes if needed:
    /// - First tries `_iter`, then `_iter1`, `_iter2`, ...
    /// - First tries `_len`, then `_len1`, `_len2`, ...
    /// - First tries `_i`, then `_i1`, `_i2`, ...
    fn gensym(&mut self, prefix: &str) -> Name {
        let base = format!("_{prefix}");

        // First try without a number
        if !self.names_in_scope.contains(&base) {
            self.names_in_scope.insert(base.clone());
            return Name::new(&base);
        }

        // Then try with incrementing numbers
        let mut counter = 1;
        loop {
            let name = format!("{base}{counter}");
            if !self.names_in_scope.contains(&name) {
                self.names_in_scope.insert(name.clone());
                return Name::new(&name);
            }
            counter += 1;
        }
    }

    /// Add a user-defined name to the set of known names.
    fn add_name_to_scope(&mut self, name: &str) {
        self.names_in_scope.insert(name.to_string());
    }

    fn lower_block_expr(&mut self, block: &baml_compiler_syntax::ast::BlockExpr) -> ExprId {
        use baml_compiler_syntax::{SyntaxKind, ast::BlockElement};

        let mut stmts = Vec::new();
        let mut tail_expr = None;

        let elements: Vec<_> = block.elements().collect();

        for (idx, element) in elements.iter().enumerate() {
            let is_last = idx == elements.len() - 1;
            match element {
                BlockElement::Stmt(node) => {
                    let stmt_id = match node.kind() {
                        SyntaxKind::LET_STMT => self.lower_let_stmt(node, false),
                        SyntaxKind::WATCH_LET => self.lower_let_stmt(node, true),
                        SyntaxKind::RETURN_STMT => self.lower_return_stmt(node),
                        SyntaxKind::WHILE_STMT => self.lower_while_stmt(node),
                        SyntaxKind::FOR_EXPR => self.lower_for_stmt(node),
                        SyntaxKind::BREAK_STMT => self.alloc_stmt(Stmt::Break, node.text_range()),
                        SyntaxKind::CONTINUE_STMT => {
                            self.alloc_stmt(Stmt::Continue, node.text_range())
                        }
                        SyntaxKind::ASSERT_STMT => self.lower_assert_stmt(node),
                        _ => self.alloc_stmt(Stmt::Missing, node.text_range()),
                    };

                    // Check for missing semicolon on let statements only.
                    // - let/watch_let: always need semicolons
                    // - All other constructs (if, while, for, assert, etc.): semicolons optional
                    let needs_semicolon =
                        matches!(node.kind(), SyntaxKind::LET_STMT | SyntaxKind::WATCH_LET);

                    if needs_semicolon && !element.has_trailing_semicolon() {
                        self.push_diagnostic(HirDiagnostic::MissingSemicolon {
                            span: self.span_from_node_skip_trivia(node),
                        });
                    }

                    stmts.push(stmt_id);
                }
                BlockElement::ExprNode(node) => {
                    // First, try to lower as an assignment statement
                    if let Some(stmt_id) = self.try_lower_assignment(node) {
                        // Semicolons are optional for assignments
                        stmts.push(stmt_id);
                        continue;
                    }

                    // Not an assignment - lower as regular expression
                    let expr_id = self.lower_expr(node);

                    // Check if this expression is followed by a semicolon
                    let has_semicolon = element.has_trailing_semicolon();

                    // Last expression without semicolon becomes tail expression (return value)
                    if is_last && !has_semicolon {
                        tail_expr = Some(expr_id);
                    } else {
                        // Expression statement (with semicolon or not last)
                        // Semicolons are optional for expression statements
                        stmts.push(self.alloc_stmt(Stmt::Expr(expr_id), node.text_range()));
                    }
                }
                BlockElement::ExprToken(token) => {
                    // Handle bare tokens as potential tail expressions
                    let span = token.text_range();
                    let expr_id = match token.kind() {
                        SyntaxKind::WORD => {
                            let text = token.text();
                            let expr_to_alloc = match text {
                                "true" => Expr::Literal(Literal::Bool(true)),
                                "false" => Expr::Literal(Literal::Bool(false)),
                                "null" => Expr::Literal(Literal::Null),
                                _ => Expr::Path(vec![Name::new(text)]),
                            };
                            self.alloc_expr(expr_to_alloc, span)
                        }
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            self.alloc_expr(Expr::Literal(Literal::Int(value)), span)
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            let text = token.text().to_string();
                            self.alloc_expr(Expr::Literal(Literal::Float(text)), span)
                        }
                        SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                            let text = token.text().to_string();
                            let content = if text.starts_with("#\"") && text.ends_with("\"#") {
                                text[2..text.len() - 2].to_string()
                            } else if text.starts_with('"') && text.ends_with('"') {
                                text[1..text.len() - 1].to_string()
                            } else {
                                text
                            };
                            self.alloc_expr(Expr::Literal(Literal::String(content)), span)
                        }
                        _ => self.alloc_expr(Expr::Missing, span),
                    };

                    // Check if this is a tail expression
                    // Last element without semicolon becomes tail expression (return value)
                    let has_semicolon = element.has_trailing_semicolon();
                    if is_last && !has_semicolon {
                        tail_expr = Some(expr_id);
                    } else {
                        // Semicolons are optional for expression statements
                        stmts.push(self.alloc_stmt(Stmt::Expr(expr_id), span));
                    }
                }
                BlockElement::HeaderComment(node) => {
                    let stmt_id = self.lower_header_comment(node);
                    stmts.push(stmt_id);
                }
            }
        }

        self.alloc_expr(
            Expr::Block { stmts, tail_expr },
            block.syntax().text_range(),
        )
    }

    fn lower_expr(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> ExprId {
        use baml_compiler_syntax::SyntaxKind;

        match node.kind() {
            SyntaxKind::BINARY_EXPR => self.lower_binary_expr(node),
            SyntaxKind::UNARY_EXPR => self.lower_unary_expr(node),
            SyntaxKind::CALL_EXPR => self.lower_call_expr(node),
            SyntaxKind::IF_EXPR => self.lower_if_expr(node),
            SyntaxKind::MATCH_EXPR => self.lower_match_expr(node),
            SyntaxKind::BLOCK_EXPR => {
                if let Some(block) = baml_compiler_syntax::ast::BlockExpr::cast(node.clone()) {
                    self.lower_block_expr(&block)
                } else {
                    self.alloc_expr(Expr::Missing, node.text_range())
                }
            }
            SyntaxKind::PATH_EXPR => self.lower_path_expr(node),
            SyntaxKind::FIELD_ACCESS_EXPR => self.lower_field_access_expr(node),
            SyntaxKind::ENV_ACCESS_EXPR => self.lower_env_access_expr(node),
            SyntaxKind::INDEX_EXPR => self.lower_index_expr(node),
            SyntaxKind::PAREN_EXPR => {
                // Unwrap parentheses - just lower the inner expression
                // First try to find a child node (for complex expressions like `(1 + 2)`)
                if let Some(inner) = node.children().next() {
                    self.lower_expr(&inner)
                } else {
                    // No child nodes - the parentheses contain only tokens.
                    // This happens for simple expressions like `(b)` where `b` is a variable.
                    // Look for tokens and handle both literals and variable references.
                    self.try_lower_paren_token_content(node)
                        .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()))
                }
            }
            SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                self.lower_string_literal(node)
            }
            SyntaxKind::ARRAY_LITERAL => self.lower_array_literal(node),
            SyntaxKind::OBJECT_LITERAL => self.lower_object_literal(node),
            SyntaxKind::MAP_LITERAL => self.lower_map_literal(node),
            _ => {
                // Check if this is a literal token
                if let Some(literal) = self.try_lower_literal_token(node) {
                    literal
                } else {
                    self.alloc_expr(Expr::Missing, node.text_range())
                }
            }
        }
    }

    fn lower_binary_expr(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> ExprId {
        use baml_compiler_syntax::SyntaxKind;

        // Binary expressions can have: child nodes (other exprs) OR direct tokens (literals/identifiers)
        // We need to handle both cases

        let mut lhs = None;
        let mut rhs = None;
        let mut op = None;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child_node) => {
                    // This is a child expression node (e.g., another BINARY_EXPR, PAREN_EXPR)
                    let expr_id = self.lower_expr(&child_node);
                    if lhs.is_none() {
                        lhs = Some(expr_id);
                    } else {
                        rhs = Some(expr_id);
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    let span = token.text_range();
                    match token.kind() {
                        // Operators
                        SyntaxKind::PLUS => op = Some(BinaryOp::Add),
                        SyntaxKind::MINUS => op = Some(BinaryOp::Sub),
                        SyntaxKind::STAR => op = Some(BinaryOp::Mul),
                        SyntaxKind::SLASH => op = Some(BinaryOp::Div),
                        SyntaxKind::PERCENT => op = Some(BinaryOp::Mod),
                        SyntaxKind::EQUALS_EQUALS => op = Some(BinaryOp::Eq),
                        SyntaxKind::NOT_EQUALS => op = Some(BinaryOp::Ne),
                        SyntaxKind::LESS => op = Some(BinaryOp::Lt),
                        SyntaxKind::LESS_EQUALS => op = Some(BinaryOp::Le),
                        SyntaxKind::GREATER => op = Some(BinaryOp::Gt),
                        SyntaxKind::GREATER_EQUALS => op = Some(BinaryOp::Ge),
                        SyntaxKind::AND_AND => op = Some(BinaryOp::And),
                        SyntaxKind::OR_OR => op = Some(BinaryOp::Or),
                        SyntaxKind::AND => op = Some(BinaryOp::BitAnd),
                        SyntaxKind::PIPE => op = Some(BinaryOp::BitOr),
                        SyntaxKind::CARET => op = Some(BinaryOp::BitXor),
                        SyntaxKind::LESS_LESS => op = Some(BinaryOp::Shl),
                        SyntaxKind::GREATER_GREATER => op = Some(BinaryOp::Shr),
                        SyntaxKind::KW_INSTANCEOF => op = Some(BinaryOp::Instanceof),

                        // Literals and identifiers - convert to expressions
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            let expr_id = self.alloc_expr(Expr::Literal(Literal::Int(value)), span);
                            if lhs.is_none() {
                                lhs = Some(expr_id);
                            } else {
                                rhs = Some(expr_id);
                            }
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            let expr_id = self.alloc_expr(
                                Expr::Literal(Literal::Float(token.text().to_string())),
                                span,
                            );
                            if lhs.is_none() {
                                lhs = Some(expr_id);
                            } else {
                                rhs = Some(expr_id);
                            }
                        }
                        SyntaxKind::WORD => {
                            let text = token.text();
                            let expr_id = match text {
                                "true" => self.alloc_expr(Expr::Literal(Literal::Bool(true)), span),
                                "false" => {
                                    self.alloc_expr(Expr::Literal(Literal::Bool(false)), span)
                                }
                                "null" => self.alloc_expr(Expr::Literal(Literal::Null), span),
                                _ => self.alloc_expr(Expr::Path(vec![Name::new(text)]), span),
                            };
                            if lhs.is_none() {
                                lhs = Some(expr_id);
                            } else {
                                rhs = Some(expr_id);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        let lhs = lhs.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        let rhs = rhs.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        let op = op.unwrap_or(BinaryOp::Add);

        self.alloc_expr(Expr::Binary { op, lhs, rhs }, node.text_range())
    }

    /// Try to lower a `BINARY_EXPR` as an assignment statement.
    /// Returns Some(StmtId) if it's an assignment, None otherwise.
    fn try_lower_assignment(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> Option<StmtId> {
        use baml_compiler_syntax::SyntaxKind;

        if node.kind() != SyntaxKind::BINARY_EXPR {
            return None;
        }

        // FIRST: Check if there's an assignment operator before lowering anything.
        // This avoids allocating expressions for non-assignment binary expressions.
        let mut assign_op: Option<Option<AssignOp>> = None; // None=not assignment, Some(None)=simple assign, Some(Some(op))=compound

        for child in node.children_with_tokens() {
            if let rowan::NodeOrToken::Token(token) = child {
                match token.kind() {
                    SyntaxKind::EQUALS => assign_op = Some(None),
                    SyntaxKind::PLUS_EQUALS => assign_op = Some(Some(AssignOp::Add)),
                    SyntaxKind::MINUS_EQUALS => assign_op = Some(Some(AssignOp::Sub)),
                    SyntaxKind::STAR_EQUALS => assign_op = Some(Some(AssignOp::Mul)),
                    SyntaxKind::SLASH_EQUALS => assign_op = Some(Some(AssignOp::Div)),
                    SyntaxKind::PERCENT_EQUALS => assign_op = Some(Some(AssignOp::Mod)),
                    SyntaxKind::AND_EQUALS => assign_op = Some(Some(AssignOp::BitAnd)),
                    SyntaxKind::PIPE_EQUALS => assign_op = Some(Some(AssignOp::BitOr)),
                    SyntaxKind::CARET_EQUALS => assign_op = Some(Some(AssignOp::BitXor)),
                    SyntaxKind::LESS_LESS_EQUALS => assign_op = Some(Some(AssignOp::Shl)),
                    SyntaxKind::GREATER_GREATER_EQUALS => assign_op = Some(Some(AssignOp::Shr)),
                    _ => {}
                }
            }
        }

        // Early return if not an assignment - don't allocate any expressions
        let assign_op = assign_op?;

        // Now lower the operands since we know this is an assignment
        let mut lhs: Option<ExprId> = None;
        let mut rhs: Option<ExprId> = None;

        for child in node.children_with_tokens() {
            match child {
                rowan::NodeOrToken::Node(n) => {
                    let expr_id = self.lower_expr(&n);
                    if lhs.is_none() {
                        lhs = Some(expr_id);
                    } else {
                        rhs = Some(expr_id);
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    let span = token.text_range();
                    // Handle literals/identifiers as expressions (skip operators)
                    match token.kind() {
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            let expr_id = self.alloc_expr(Expr::Literal(Literal::Int(value)), span);
                            if lhs.is_none() {
                                lhs = Some(expr_id);
                            } else {
                                rhs = Some(expr_id);
                            }
                        }
                        SyntaxKind::WORD => {
                            let text = token.text();
                            let expr_id = match text {
                                "true" => self.alloc_expr(Expr::Literal(Literal::Bool(true)), span),
                                "false" => {
                                    self.alloc_expr(Expr::Literal(Literal::Bool(false)), span)
                                }
                                "null" => self.alloc_expr(Expr::Literal(Literal::Null), span),
                                _ => self.alloc_expr(Expr::Path(vec![Name::new(text)]), span),
                            };
                            if lhs.is_none() {
                                lhs = Some(expr_id);
                            } else {
                                rhs = Some(expr_id);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        let target = lhs.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        let value = rhs.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        let stmt = match assign_op {
            None => Stmt::Assign { target, value },
            Some(op) => Stmt::AssignOp { target, op, value },
        };

        Some(self.alloc_stmt(stmt, node.text_range()))
    }

    fn lower_unary_expr(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> ExprId {
        use baml_compiler_syntax::SyntaxKind;

        // Unary expressions can have: child nodes (other exprs) OR direct tokens (literals/identifiers)
        // We need to handle both cases, similar to lower_binary_expr.
        let mut op = None;
        let mut operand = None;
        // Track double operators like -- and ++ which need special handling
        let mut double_op = false;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child_node) => {
                    // This is a child expression node
                    operand = Some(self.lower_expr(&child_node));
                }
                rowan::NodeOrToken::Token(token) => {
                    let span = token.text_range();
                    match token.kind() {
                        // Operators
                        SyntaxKind::NOT => op = Some(UnaryOp::Not),
                        SyntaxKind::MINUS => op = Some(UnaryOp::Neg),
                        // Double operators: -- is double negation, ++ is double... nothing useful
                        // but we handle it for consistency
                        SyntaxKind::MINUS_MINUS => {
                            op = Some(UnaryOp::Neg);
                            double_op = true;
                        }
                        SyntaxKind::PLUS_PLUS => {
                            // ++x in this context just returns x (no-op for values)
                            // We'll treat it as identity by not setting any op
                            // Actually, let's just skip it - operand will be returned as-is
                        }

                        // Literals and identifiers - convert to expressions
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            operand =
                                Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span));
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            operand = Some(self.alloc_expr(
                                Expr::Literal(Literal::Float(token.text().to_string())),
                                span,
                            ));
                        }
                        SyntaxKind::WORD => {
                            let text = token.text();
                            let expr_id = match text {
                                "true" => self.alloc_expr(Expr::Literal(Literal::Bool(true)), span),
                                "false" => {
                                    self.alloc_expr(Expr::Literal(Literal::Bool(false)), span)
                                }
                                "null" => self.alloc_expr(Expr::Literal(Literal::Null), span),
                                _ => self.alloc_expr(Expr::Path(vec![Name::new(text)]), span),
                            };
                            operand = Some(expr_id);
                        }
                        _ => {}
                    }
                }
            }
        }

        let expr = operand.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        // If no operator was set (e.g., ++x), just return the operand
        let Some(op) = op else {
            return expr;
        };

        // Create the first unary expression
        let result = self.alloc_expr(Expr::Unary { op, expr }, node.text_range());

        // For double operators like --, wrap in another unary operation
        if double_op {
            self.alloc_expr(Expr::Unary { op, expr: result }, node.text_range())
        } else {
            result
        }
    }

    fn lower_if_expr(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> ExprId {
        use baml_compiler_syntax::SyntaxKind;

        // IF_EXPR structure: condition (EXPR), then_branch (BLOCK_EXPR), optional else_branch
        let children: Vec<_> = node.children().collect();

        // Validate that condition is wrapped in parentheses
        if let Some(cond) = children.first() {
            if cond.kind() != SyntaxKind::PAREN_EXPR {
                self.push_diagnostic(HirDiagnostic::MissingConditionParens {
                    kind: "if",
                    span: self.span_from_node(cond),
                });
            }
        }

        let condition = children
            .first()
            .map(|n| self.lower_expr(n))
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        let then_branch = children
            .get(1)
            .map(|n| self.lower_expr(n))
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        // Check for else branch - it might be another IF_EXPR (else if) or BLOCK_EXPR (else)
        let else_branch = if children.len() > 2 {
            Some(self.lower_expr(&children[2]))
        } else {
            None
        };

        self.alloc_expr(
            Expr::If {
                condition,
                then_branch,
                else_branch,
            },
            node.text_range(),
        )
    }

    /// Lower a match expression from CST to HIR.
    ///
    /// `MATCH_EXPR` structure (from parser):
    /// - Scrutinee expression (could be a `PAREN_EXPR` wrapping the actual expr, or a literal token)
    /// - One or more `MATCH_ARM` nodes
    fn lower_match_expr(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> ExprId {
        use baml_compiler_syntax::SyntaxKind;

        let match_span = self.span_from_node(node);
        let mut scrutinee = None;
        let mut arm_ids = Vec::new();

        // Use children_with_tokens to handle both node and token children
        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => {
                    match child.kind() {
                        SyntaxKind::MATCH_ARM => {
                            let (arm, spans) = self.lower_match_arm(&child);
                            let arm_id = self.alloc_match_arm(arm, spans);
                            arm_ids.push(arm_id);
                        }
                        _ => {
                            // First non-MATCH_ARM child is the scrutinee (as a node)
                            if scrutinee.is_none() {
                                scrutinee = Some(self.lower_expr(&child));
                            }
                        }
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    // Handle literal tokens as scrutinee (when scrutinee is a simple value)
                    if scrutinee.is_none() {
                        match token.kind() {
                            SyntaxKind::INTEGER_LITERAL => {
                                let value = token.text().parse::<i64>().unwrap_or(0);
                                let range = token.text_range();
                                scrutinee = Some(
                                    self.alloc_expr(Expr::Literal(Literal::Int(value)), range),
                                );
                            }
                            SyntaxKind::FLOAT_LITERAL => {
                                let text = token.text().to_string();
                                let range = token.text_range();
                                scrutinee = Some(
                                    self.alloc_expr(Expr::Literal(Literal::Float(text)), range),
                                );
                            }
                            SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                                let text = token.text().to_string();
                                let content = if text.starts_with("#\"") && text.ends_with("\"#") {
                                    text[2..text.len() - 2].to_string()
                                } else if text.starts_with('"') && text.ends_with('"') {
                                    text[1..text.len() - 1].to_string()
                                } else {
                                    text
                                };
                                let range = token.text_range();
                                scrutinee = Some(
                                    self.alloc_expr(Expr::Literal(Literal::String(content)), range),
                                );
                            }
                            SyntaxKind::WORD => {
                                let text = token.text();
                                let range = token.text_range();
                                let expr = match text {
                                    "true" => Expr::Literal(Literal::Bool(true)),
                                    "false" => Expr::Literal(Literal::Bool(false)),
                                    "null" => Expr::Literal(Literal::Null),
                                    _ => Expr::Path(vec![Name::new(text)]),
                                };
                                scrutinee = Some(self.alloc_expr(expr, range));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        let scrutinee = scrutinee.unwrap_or_else(|| {
            // If we couldn't find a scrutinee, create a missing expression with an empty range
            self.alloc_expr(Expr::Missing, TextRange::default())
        });

        let expr_id = self.exprs.alloc(Expr::Match {
            scrutinee,
            arms: arm_ids,
        });

        // Store span information for this match expression
        self.source_map.insert_expr(expr_id, match_span);

        expr_id
    }

    /// Lower a single match arm from CST to HIR.
    ///
    /// `MATCH_ARM` structure (from parser):
    /// - `MATCH_PATTERN` node
    /// - Optional `MATCH_GUARD` node (contains 'if' keyword + expression)
    /// - `FAT_ARROW` token ('=>')
    /// - Body expression (`BLOCK_EXPR` or other expression, or literal token)
    ///
    /// Returns both the lowered arm and its span information.
    fn lower_match_arm(
        &mut self,
        node: &baml_compiler_syntax::SyntaxNode,
    ) -> (MatchArm, MatchArmSpans) {
        use baml_compiler_syntax::SyntaxKind;

        let arm_span = self.span_from_node(node);
        let mut pattern = None;
        let mut pattern_span = None;
        let mut guard = None;
        let mut body = None;
        let mut seen_fat_arrow = false;

        // Use children_with_tokens to handle both node and token children
        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => {
                    match child.kind() {
                        SyntaxKind::MATCH_PATTERN => {
                            pattern_span = Some(self.span_from_node(&child));
                            pattern = Some(self.lower_match_pattern(&child));
                        }
                        SyntaxKind::MATCH_GUARD => {
                            // MATCH_GUARD contains: KW_IF, then the guard expression
                            // The guard expression can be either:
                            // 1. A child node (for complex expressions like `a && b`)
                            // 2. A token (for simple identifiers like `flag`)
                            if let Some(expr_node) = child.children().next() {
                                guard = Some(self.lower_expr(&expr_node));
                            } else {
                                // No child node - look for tokens after KW_IF
                                for tok in child.children_with_tokens() {
                                    if let rowan::NodeOrToken::Token(t) = tok {
                                        match t.kind() {
                                            SyntaxKind::KW_IF => continue, // skip the 'if' keyword
                                            SyntaxKind::WORD => {
                                                let text = t.text();
                                                let expr = match text {
                                                    "true" => self
                                                        .exprs
                                                        .alloc(Expr::Literal(Literal::Bool(true))),
                                                    "false" => self
                                                        .exprs
                                                        .alloc(Expr::Literal(Literal::Bool(false))),
                                                    "null" => self
                                                        .exprs
                                                        .alloc(Expr::Literal(Literal::Null)),
                                                    _ => self
                                                        .exprs
                                                        .alloc(Expr::Path(vec![Name::new(text)])),
                                                };
                                                guard = Some(expr);
                                                break;
                                            }
                                            SyntaxKind::INTEGER_LITERAL => {
                                                let value = t.text().parse::<i64>().unwrap_or(0);
                                                guard = Some(
                                                    self.exprs
                                                        .alloc(Expr::Literal(Literal::Int(value))),
                                                );
                                                break;
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                        // Handle string literals as nodes (parser wraps them)
                        SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL
                            if seen_fat_arrow && body.is_none() =>
                        {
                            body = Some(self.lower_string_literal(&child));
                        }
                        _ => {
                            // After the fat arrow, the expression node is the body
                            if seen_fat_arrow && body.is_none() {
                                body = Some(self.lower_expr(&child));
                            }
                        }
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    match token.kind() {
                        SyntaxKind::FAT_ARROW => {
                            seen_fat_arrow = true;
                        }
                        // Handle literal tokens as body (when body is a simple value)
                        SyntaxKind::INTEGER_LITERAL if seen_fat_arrow && body.is_none() => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            body = Some(self.exprs.alloc(Expr::Literal(Literal::Int(value))));
                        }
                        SyntaxKind::FLOAT_LITERAL if seen_fat_arrow && body.is_none() => {
                            let text = token.text().to_string();
                            body = Some(self.exprs.alloc(Expr::Literal(Literal::Float(text))));
                        }
                        SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL
                            if seen_fat_arrow && body.is_none() =>
                        {
                            let text = token.text().trim();
                            let content = if text.starts_with("#\"") && text.ends_with("\"#") {
                                &text[2..text.len() - 2]
                            } else if text.starts_with('"') && text.ends_with('"') {
                                &text[1..text.len() - 1]
                            } else {
                                text
                            };
                            body = Some(
                                self.exprs
                                    .alloc(Expr::Literal(Literal::String(content.to_string()))),
                            );
                        }
                        SyntaxKind::WORD if seen_fat_arrow && body.is_none() => {
                            let text = token.text();
                            let expr = match text {
                                "true" => self.exprs.alloc(Expr::Literal(Literal::Bool(true))),
                                "false" => self.exprs.alloc(Expr::Literal(Literal::Bool(false))),
                                "null" => self.exprs.alloc(Expr::Literal(Literal::Null)),
                                _ => self.exprs.alloc(Expr::Path(vec![Name::new(text)])),
                            };
                            body = Some(expr);
                        }
                        _ => {}
                    }
                }
            }
        }

        let arm = MatchArm {
            pattern: pattern
                .unwrap_or_else(|| self.patterns.alloc(Pattern::Binding(Name::new("_")))),
            guard,
            body: body.unwrap_or_else(|| self.exprs.alloc(Expr::Missing)),
        };

        let spans = MatchArmSpans {
            arm_span,
            pattern_span: pattern_span.unwrap_or(arm_span),
        };

        (arm, spans)
    }

    /// Lower a match pattern from CST to HIR.
    ///
    /// `MATCH_PATTERN` structure (from parser):
    /// - Pattern elements (identifiers, literals, type expressions)
    /// - Optional PIPE tokens for union patterns
    ///
    /// Pattern forms:
    /// - Binding: `x`, `_`
    /// - Typed binding: `s: Success`
    /// - Literal: `null`, `true`, `42`, `"hello"`
    /// - Enum variant: `Status.Active`
    /// - Union: `200 | 201` or `Status.Active | Status.Pending`
    fn lower_match_pattern(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> PatId {
        use baml_compiler_syntax::SyntaxKind;

        // Collect pattern elements separated by PIPE
        let mut elements: Vec<PatId> = Vec::new();
        let mut current_element: Option<PatternElement> = None;
        // Track if we've seen a minus sign that should negate the next numeric literal
        let mut pending_negation = false;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Token(token) => {
                    match token.kind() {
                        SyntaxKind::PIPE => {
                            // Finalize current element and start a new one
                            if let Some(el) = current_element.take() {
                                elements.push(self.finalize_pattern_element(el));
                            }
                        }
                        SyntaxKind::WORD => {
                            let text = token.text().to_string();

                            // First, check if we're completing an enum variant
                            if let Some(PatternElement::EnumStart(enum_name, start)) =
                                current_element.take()
                            {
                                // Complete the enum variant: EnumName.Variant
                                let variant = Name::new(&text);
                                // Compute span from enum name start to variant end
                                let range = TextRange::new(start, token.text_range().end());
                                elements.push(self.alloc_pattern(
                                    Pattern::EnumVariant { enum_name, variant },
                                    range,
                                ));
                                continue;
                            }

                            match text.as_str() {
                                "true" => {
                                    if let Some(el) = current_element.take() {
                                        elements.push(self.finalize_pattern_element(el));
                                    }
                                    elements.push(
                                        self.patterns.alloc(Pattern::Literal(Literal::Bool(true))),
                                    );
                                }
                                "false" => {
                                    if let Some(el) = current_element.take() {
                                        elements.push(self.finalize_pattern_element(el));
                                    }
                                    elements.push(
                                        self.patterns.alloc(Pattern::Literal(Literal::Bool(false))),
                                    );
                                }
                                "null" => {
                                    if let Some(el) = current_element.take() {
                                        elements.push(self.finalize_pattern_element(el));
                                    }
                                    elements
                                        .push(self.patterns.alloc(Pattern::Literal(Literal::Null)));
                                }
                                _ => {
                                    // Finalize any previous element before starting new one
                                    if let Some(el) = current_element.take() {
                                        elements.push(self.finalize_pattern_element(el));
                                    }
                                    // Regular identifier - could be binding or start of enum variant
                                    // Track the start position for span tracking
                                    current_element = Some(PatternElement::Ident(
                                        Name::new(&text),
                                        token.text_range().start(),
                                    ));
                                }
                            }
                        }
                        SyntaxKind::DOT => {
                            // Transition: Ident.Variant (enum variant pattern)
                            if let Some(PatternElement::Ident(enum_name, start)) =
                                current_element.take()
                            {
                                current_element = Some(PatternElement::EnumStart(enum_name, start));
                            }
                        }
                        SyntaxKind::COLON => {
                            // Transition: ident: Type (typed binding pattern)
                            if let Some(PatternElement::Ident(name, start)) = current_element.take()
                            {
                                current_element =
                                    Some(PatternElement::TypedBindingStart(name, start));
                            }
                        }
                        SyntaxKind::MINUS => {
                            // Track negation for the next numeric literal
                            pending_negation = true;
                        }
                        SyntaxKind::INTEGER_LITERAL => {
                            if let Some(el) = current_element.take() {
                                elements.push(self.finalize_pattern_element(el));
                            }
                            let mut value = token.text().parse::<i64>().unwrap_or(0);
                            if pending_negation {
                                value = -value;
                                pending_negation = false;
                            }
                            elements
                                .push(self.patterns.alloc(Pattern::Literal(Literal::Int(value))));
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            if let Some(el) = current_element.take() {
                                elements.push(self.finalize_pattern_element(el));
                            }
                            let text = token.text().to_string();
                            let text = if pending_negation {
                                pending_negation = false;
                                format!("-{text}")
                            } else {
                                text
                            };
                            elements
                                .push(self.patterns.alloc(Pattern::Literal(Literal::Float(text))));
                        }
                        SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                            if let Some(el) = current_element.take() {
                                elements.push(self.finalize_pattern_element(el));
                            }
                            let text = token.text().to_string();
                            let content = if text.starts_with("#\"") && text.ends_with("\"#") {
                                text[2..text.len() - 2].to_string()
                            } else if text.starts_with('"') && text.ends_with('"') {
                                text[1..text.len() - 1].to_string()
                            } else {
                                text
                            };
                            elements.push(
                                self.patterns
                                    .alloc(Pattern::Literal(Literal::String(content))),
                            );
                        }
                        _ => {}
                    }
                }
                rowan::NodeOrToken::Node(child_node) => {
                    match child_node.kind() {
                        SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                            // Handle string literals as nodes (parser wraps them in nodes)
                            if let Some(el) = current_element.take() {
                                elements.push(self.finalize_pattern_element(el));
                            }
                            // Extract the string content from the node
                            // Trim whitespace trivia first, then remove quotes
                            let text = child_node.text().to_string();
                            let trimmed = text.trim();
                            let content = if trimmed.starts_with("#\"") && trimmed.ends_with("\"#")
                            {
                                trimmed[2..trimmed.len() - 2].to_string()
                            } else if trimmed.starts_with('"') && trimmed.ends_with('"') {
                                trimmed[1..trimmed.len() - 1].to_string()
                            } else {
                                trimmed.to_string()
                            };
                            elements.push(
                                self.patterns
                                    .alloc(Pattern::Literal(Literal::String(content))),
                            );
                        }
                        SyntaxKind::TYPE_EXPR => {
                            // Complete typed binding: ident: Type
                            if let Some(PatternElement::TypedBindingStart(name, start)) =
                                current_element.take()
                            {
                                if let Some(type_expr) =
                                    baml_compiler_syntax::ast::TypeExpr::cast(child_node.clone())
                                {
                                    let ty = crate::type_ref::TypeRef::from_ast(&type_expr);
                                    // Compute span from name start to type end
                                    let range =
                                        TextRange::new(start, child_node.text_range().end());
                                    elements.push(
                                        self.alloc_pattern(
                                            Pattern::TypedBinding { name, ty },
                                            range,
                                        ),
                                    );
                                } else {
                                    // Failed to cast - treat as simple binding
                                    elements.push(self.patterns.alloc(Pattern::Binding(name)));
                                }
                            }
                        }
                        SyntaxKind::MATCH_PATTERN => {
                            // Nested pattern group (from parenthesized patterns)
                            // Flatten the nested pattern into current elements to maintain
                            // canonical form: (A | B) | C = A | B | C (union associativity)
                            if let Some(el) = current_element.take() {
                                elements.push(self.finalize_pattern_element(el));
                            }
                            let nested_pat_id = self.lower_match_pattern(&child_node);
                            // Check if nested pattern is a union and flatten it
                            let nested_elements: Option<Vec<PatId>> =
                                match &self.patterns[nested_pat_id] {
                                    Pattern::Union(sub_elements) => Some(sub_elements.clone()),
                                    _ => None,
                                };
                            if let Some(sub_elements) = nested_elements {
                                // Flatten: add all sub-elements directly
                                elements.extend(sub_elements);
                            } else {
                                // Single pattern - add as-is
                                elements.push(nested_pat_id);
                            }
                        }
                        _ => {
                            // Handle other nested patterns if needed
                        }
                    }
                }
            }
        }

        // Finalize any remaining element
        if let Some(el) = current_element.take() {
            elements.push(self.finalize_pattern_element(el));
        }

        // Return based on number of elements
        match elements.len() {
            0 => self.patterns.alloc(Pattern::Binding(Name::new("_"))),
            1 => elements.into_iter().next().unwrap(),
            _ => self.patterns.alloc(Pattern::Union(elements)),
        }
    }

    /// Finalize a partially-built pattern element.
    fn finalize_pattern_element(&mut self, element: PatternElement) -> PatId {
        match element {
            PatternElement::Ident(name, _start) => self.patterns.alloc(Pattern::Binding(name)),
            PatternElement::EnumStart(enum_name, _start) => {
                // Incomplete enum variant (missing variant name) - treat as binding
                self.patterns.alloc(Pattern::Binding(enum_name))
            }
            PatternElement::TypedBindingStart(name, _start) => {
                // Incomplete typed binding (missing type) - treat as simple binding
                self.patterns.alloc(Pattern::Binding(name))
            }
        }
    }

    fn lower_call_expr(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> ExprId {
        use baml_compiler_syntax::SyntaxKind;

        // CALL_EXPR structure: callee (PATH_EXPR, WORD token, or other expr), CALL_ARGS
        // The callee can be either:
        // 1. A WORD token directly (simple function call like `Foo(1)`)
        // 2. A PATH_EXPR node (qualified path like `mod::Foo(1)`)
        // 3. Another expression node (e.g., `(get_fn())(1)`)

        // First, try to find a callee expression node
        let callee_node = node
            .children()
            .find(|n| !matches!(n.kind(), SyntaxKind::CALL_ARGS));

        let callee = if let Some(n) = callee_node {
            if n.kind() == SyntaxKind::ENV_ACCESS_EXPR {
                // env.method(...) → Path(["env", method])
                // Don't use lower_expr which would desugar to get_or_panic
                use baml_compiler_syntax::ast::EnvAccessExpr;
                use rowan::ast::AstNode;
                if let Some(field_token) = EnvAccessExpr::cast(n.clone()).and_then(|e| e.field()) {
                    self.alloc_expr(
                        Expr::Path(vec![Name::new("env"), Name::new(field_token.text())]),
                        n.text_range(),
                    )
                } else {
                    self.alloc_expr(Expr::Missing, n.text_range())
                }
            } else {
                self.lower_expr(&n)
            }
        } else {
            // No callee node - check for a WORD token (simple function name)
            let word_token = node
                .children_with_tokens()
                .filter_map(baml_compiler_syntax::NodeOrToken::into_token)
                .find(|t| t.kind() == SyntaxKind::WORD);

            if let Some(token) = word_token {
                let name = token.text().to_string();
                self.alloc_expr(Expr::Path(vec![Name::new(&name)]), token.text_range())
            } else {
                self.alloc_expr(Expr::Missing, node.text_range())
            }
        };

        // Find CALL_ARGS node and extract arguments
        // We iterate over children_with_tokens() to handle both expression nodes
        // and bare tokens (like simple identifiers) in argument position.
        let args = node
            .children()
            .find(|n| n.kind() == SyntaxKind::CALL_ARGS)
            .map(|args_node| {
                let mut args = Vec::new();

                for element in args_node.children_with_tokens() {
                    match element {
                        baml_compiler_syntax::NodeOrToken::Node(child_node) => {
                            // Handle expression nodes
                            if matches!(
                                child_node.kind(),
                                SyntaxKind::EXPR
                                    | SyntaxKind::BINARY_EXPR
                                    | SyntaxKind::UNARY_EXPR
                                    | SyntaxKind::CALL_EXPR
                                    | SyntaxKind::PATH_EXPR
                                    | SyntaxKind::FIELD_ACCESS_EXPR
                                    | SyntaxKind::ENV_ACCESS_EXPR
                                    | SyntaxKind::INDEX_EXPR
                                    | SyntaxKind::IF_EXPR
                                    | SyntaxKind::BLOCK_EXPR
                                    | SyntaxKind::PAREN_EXPR
                                    | SyntaxKind::ARRAY_LITERAL
                                    | SyntaxKind::STRING_LITERAL
                                    | SyntaxKind::OBJECT_LITERAL
                                    | SyntaxKind::MAP_LITERAL
                            ) {
                                args.push(self.lower_expr(&child_node));
                            }
                        }
                        baml_compiler_syntax::NodeOrToken::Token(token) => {
                            // Handle bare tokens (literals, identifiers)
                            let span = token.text_range();
                            let expr = match token.kind() {
                                SyntaxKind::INTEGER_LITERAL => {
                                    let text = token.text();
                                    let value = text.parse::<i64>().unwrap_or(0);
                                    Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span))
                                }
                                SyntaxKind::FLOAT_LITERAL => {
                                    let text = token.text().to_string();
                                    Some(self.alloc_expr(Expr::Literal(Literal::Float(text)), span))
                                }
                                SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                                    let text = token.text().to_string();
                                    // Strip quotes
                                    let content =
                                        if text.starts_with("#\"") && text.ends_with("\"#") {
                                            text[2..text.len() - 2].to_string()
                                        } else if text.starts_with('"') && text.ends_with('"') {
                                            text[1..text.len() - 1].to_string()
                                        } else {
                                            text
                                        };
                                    Some(
                                        self.alloc_expr(
                                            Expr::Literal(Literal::String(content)),
                                            span,
                                        ),
                                    )
                                }
                                SyntaxKind::WORD => {
                                    // Variable reference or keyword (true/false/null)
                                    let text = token.text();
                                    match text {
                                        "true" => {
                                            Some(self.alloc_expr(
                                                Expr::Literal(Literal::Bool(true)),
                                                span,
                                            ))
                                        }
                                        "false" => {
                                            Some(self.alloc_expr(
                                                Expr::Literal(Literal::Bool(false)),
                                                span,
                                            ))
                                        }
                                        "null" => Some(
                                            self.alloc_expr(Expr::Literal(Literal::Null), span),
                                        ),
                                        _ => {
                                            Some(self.alloc_expr(
                                                Expr::Path(vec![Name::new(text)]),
                                                span,
                                            ))
                                        }
                                    }
                                }
                                _ => None,
                            };
                            if let Some(e) = expr {
                                args.push(e);
                            }
                        }
                    }
                }

                args
            })
            .unwrap_or_default();

        self.alloc_expr(Expr::Call { callee, args }, node.text_range())
    }

    /// Lower a `FIELD_ACCESS_EXPR` to `Expr::FieldAccess`.
    ///
    /// This handles field access on complex expressions where the base is NOT
    /// a simple identifier chain:
    /// - `f().field` -> `FieldAccess` { base: Call(...), field: "field" }
    /// - `arr[0].field` -> `FieldAccess` { base: Index(...), field: "field" }
    /// - `(a + b).field` -> `FieldAccess` { base: Binary(...), field: "field" }
    ///
    /// For simple identifier chains like `user.name.length`, the parser produces
    /// `PATH_EXPR` instead, which is lowered by `lower_path_expr` to
    /// `Expr::Path(vec!["user", "name", "length"])`. Resolution of whether that's
    /// a variable + field accesses, enum variant, or module path happens in THIR.
    ///
    /// The key distinction:
    /// - `Expr::Path` - all segments are identifiers, resolution deferred to THIR
    /// - `Expr::FieldAccess` - base is a computed value, always a field access
    fn lower_field_access_expr(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> ExprId {
        use baml_compiler_syntax::ast::FieldAccessExpr;
        use rowan::ast::AstNode;

        // FIELD_ACCESS_EXPR structure: base expression, DOT token, field name (WORD)
        let Some(field_access) = FieldAccessExpr::cast(node.clone()) else {
            return self.alloc_expr(Expr::Missing, node.text_range());
        };

        let base = field_access
            .base()
            .map(|n| self.lower_expr(&n))
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        let field = field_access
            .field()
            .map(|token| Name::new(token.text()))
            .unwrap_or_else(|| Name::new(""));

        self.alloc_expr(Expr::FieldAccess { base, field }, node.text_range())
    }

    /// Lower an `ENV_ACCESS_EXPR` to a desugared call.
    ///
    /// In non-call position (standalone `env.FOO`), desugars to:
    ///   `env.get_or_panic("FOO")`
    ///
    /// When used as a callee in a `CALL_EXPR` (e.g. `env.get(...)`), the
    /// `lower_call_expr` method handles it specially — converting the
    /// `ENV_ACCESS_EXPR` callee into a path into the env module.
    fn lower_env_access_expr(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> ExprId {
        use baml_compiler_syntax::ast::EnvAccessExpr;
        use rowan::ast::AstNode;

        let Some(field_token) = EnvAccessExpr::cast(node.clone()).and_then(|e| e.field()) else {
            return self.alloc_expr(Expr::Missing, node.text_range());
        };
        let field_name = field_token.text().to_string();

        // Synthesize: env.get_or_panic("FIELD_NAME")
        let callee = self.alloc_expr(
            Expr::Path(vec![Name::new("env"), Name::new("get_or_panic")]),
            node.text_range(),
        );
        let arg = self.alloc_expr(
            Expr::Literal(Literal::String(field_name)),
            node.text_range(),
        );
        self.alloc_expr(
            Expr::Call {
                callee,
                args: vec![arg],
            },
            node.text_range(),
        )
    }

    fn lower_index_expr(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> ExprId {
        use baml_compiler_syntax::SyntaxKind;

        // INDEX_EXPR structure: base (node or token), L_BRACKET, index (node or token), R_BRACKET
        // Similar to BINARY_EXPR, the base and index can be either child nodes or direct tokens

        let mut base = None;
        let mut index = None;
        let mut inside_brackets = false;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child_node) => {
                    // Child expression node
                    let expr_id = self.lower_expr(&child_node);
                    if !inside_brackets {
                        base = Some(expr_id);
                    } else {
                        index = Some(expr_id);
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    let span = token.text_range();
                    match token.kind() {
                        SyntaxKind::L_BRACKET => {
                            inside_brackets = true;
                        }
                        SyntaxKind::R_BRACKET => {
                            inside_brackets = false;
                        }
                        // Handle direct tokens (literals, identifiers)
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            let expr_id = self.alloc_expr(Expr::Literal(Literal::Int(value)), span);
                            if !inside_brackets {
                                base = Some(expr_id);
                            } else {
                                index = Some(expr_id);
                            }
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            let expr_id = self.alloc_expr(
                                Expr::Literal(Literal::Float(token.text().to_string())),
                                span,
                            );
                            if !inside_brackets {
                                base = Some(expr_id);
                            } else {
                                index = Some(expr_id);
                            }
                        }
                        SyntaxKind::WORD => {
                            let text = token.text();
                            let expr_id = match text {
                                "true" => self.alloc_expr(Expr::Literal(Literal::Bool(true)), span),
                                "false" => {
                                    self.alloc_expr(Expr::Literal(Literal::Bool(false)), span)
                                }
                                "null" => self.alloc_expr(Expr::Literal(Literal::Null), span),
                                _ => self.alloc_expr(Expr::Path(vec![Name::new(text)]), span),
                            };
                            if !inside_brackets {
                                base = Some(expr_id);
                            } else {
                                index = Some(expr_id);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        let base = base.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        let index = index.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        self.alloc_expr(Expr::Index { base, index }, node.text_range())
    }

    fn lower_path_expr(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> ExprId {
        use baml_compiler_syntax::ast::PathExpr;
        use rowan::ast::AstNode;

        // PATH_EXPR contains one or more segments separated by dots.
        // Examples:
        // - Simple identifier: `foo` -> Path(vec!["foo"])
        // - Qualified path: `mod.foo` -> Path(vec!["mod", "foo"])
        // - Field access chain: `obj.field.nested` -> Path(vec!["obj", "field", "nested"])
        //
        // Resolution to determine the meaning (local var, field access, enum variant,
        // module item) happens in THIR.

        let Some(path_expr) = PathExpr::cast(node.clone()) else {
            return self.alloc_expr(Expr::Missing, node.text_range());
        };

        let segments: Vec<Name> = path_expr
            .segments()
            .map(|token| Name::new(token.text()))
            .collect();

        if segments.is_empty() {
            return self.alloc_expr(Expr::Missing, node.text_range());
        }

        self.alloc_expr(Expr::Path(segments), node.text_range())
    }

    fn lower_string_literal(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> ExprId {
        use baml_compiler_syntax::SyntaxKind;

        // Find the actual STRING_LITERAL or RAW_STRING_LITERAL token inside the node.
        // This avoids including trivia/whitespace that might be part of the node's text span.
        let text = node
            .children_with_tokens()
            .filter_map(baml_compiler_syntax::NodeOrToken::into_token)
            .find(|t| {
                matches!(
                    t.kind(),
                    SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL
                )
            })
            .map(|t| t.text().to_string())
            .unwrap_or_else(|| {
                // Fallback: trim the node text to remove surrounding trivia
                node.text().to_string().trim().to_string()
            });

        // Strip quotes using the helper
        let content = strip_string_delimiters(&text);

        self.alloc_expr(
            Expr::Literal(Literal::String(content.to_string())),
            node.text_range(),
        )
    }

    fn lower_array_literal(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> ExprId {
        use baml_compiler_syntax::SyntaxKind;

        // Collect elements from both child nodes and direct tokens.
        // Arrays can have mixed content: some elements are nodes (like STRING_LITERAL
        // with quote children), while others are bare tokens (INTEGER_LITERAL).
        // We need to process all of them in order.
        let mut elements = Vec::new();

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => {
                    // Skip bracket nodes (shouldn't happen but be safe)
                    if !matches!(child.kind(), SyntaxKind::L_BRACKET | SyntaxKind::R_BRACKET) {
                        elements.push(self.lower_expr(&child));
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    // Try to lower value tokens (integers, floats, etc.)
                    if let Some(expr_id) = self.lower_value_token(&token) {
                        elements.push(expr_id);
                    }
                }
            }
        }

        self.alloc_expr(Expr::Array { elements }, node.text_range())
    }

    fn lower_object_literal(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> ExprId {
        use baml_compiler_syntax::SyntaxKind;

        // Extract type name if present (before the brace)
        let type_name = node
            .children_with_tokens()
            .filter_map(baml_compiler_syntax::NodeOrToken::into_token)
            .find(|token| token.kind() == SyntaxKind::WORD)
            .map(|token| Name::new(token.text()));

        // Track position for override semantics
        let mut position = 0;
        let mut fields = Vec::new();
        let mut spreads = Vec::new();

        // Process children in order to track positions correctly
        for child in node.children() {
            match child.kind() {
                SyntaxKind::OBJECT_FIELD => {
                    let field_span = child.text_range();
                    // OBJECT_FIELD has: WORD (field name), COLON, value (EXPR or literal token)
                    let field_name = child
                        .children_with_tokens()
                        .filter_map(baml_compiler_syntax::NodeOrToken::into_token)
                        .find(|token| token.kind() == SyntaxKind::WORD)
                        .map(|token| Name::new(token.text()));

                    if let Some(field_name) = field_name {
                        // Try to get value as a child node first
                        let value = child
                            .children()
                            .next()
                            .map(|n| self.lower_expr(&n))
                            .or_else(|| {
                                // Try to get value as a direct token (literal or identifier)
                                // Skip tokens until we see COLON, then lower the next value token
                                let mut seen_colon = false;
                                child
                                    .children_with_tokens()
                                    .filter_map(baml_compiler_syntax::NodeOrToken::into_token)
                                    .find_map(|token| {
                                        if token.kind() == SyntaxKind::COLON {
                                            seen_colon = true;
                                            return None;
                                        }
                                        if !seen_colon {
                                            return None;
                                        }
                                        self.lower_value_token(&token)
                                    })
                            })
                            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, field_span));

                        fields.push((field_name, value));
                    }
                    position += 1;
                }
                SyntaxKind::SPREAD_ELEMENT => {
                    // SPREAD_ELEMENT has: DOT_DOT_DOT, expr
                    // Get the expression being spread (child node after the ... token)
                    let spread_expr = child
                        .children()
                        .next()
                        .map(|n| self.lower_expr(&n))
                        .unwrap_or_else(|| self.alloc_expr(Expr::Missing, child.text_range()));

                    spreads.push(SpreadField {
                        expr: spread_expr,
                        position,
                    });
                    position += 1;
                }
                _ => {}
            }
        }

        self.alloc_expr(
            Expr::Object {
                type_name,
                fields,
                spreads,
            },
            node.text_range(),
        )
    }

    fn lower_map_literal(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> ExprId {
        use baml_compiler_syntax::SyntaxKind;

        // Extract entries from OBJECT_FIELD children (parser reuses this node type for map entries)
        let entries =
            node.children()
                .filter(|n| n.kind() == SyntaxKind::OBJECT_FIELD)
                .filter_map(|field_node| {
                    let field_span = field_node.text_range();

                    // Key - can be identifier (WORD) or string literal
                    let key = field_node
                        .children()
                        .find(|n| n.kind() == SyntaxKind::STRING_LITERAL)
                        .map(|n| self.lower_string_literal(&n))
                        .or_else(|| {
                            // Try to get key as identifier token
                            field_node
                                .children_with_tokens()
                                .filter_map(baml_compiler_syntax::NodeOrToken::into_token)
                                .find(|token| token.kind() == SyntaxKind::WORD)
                                .map(|token| {
                                    let span = token.text_range();
                                    // Identifier key becomes a string literal
                                    self.alloc_expr(
                                        Expr::Literal(Literal::String(token.text().to_string())),
                                        span,
                                    )
                                })
                        })?;

                    let key_span = self.source_map.expr_span(key);

                    // Value - get child expression after the key
                    // Skip STRING_LITERAL if it was the key (compare spans), and get the next expression
                    let value = field_node
                        .children()
                        .filter(|n| {
                            // Skip the key if it's a STRING_LITERAL by comparing spans
                            if n.kind() == SyntaxKind::STRING_LITERAL {
                                key_span != Some(self.span_from_range(n.text_range()))
                            } else {
                                true
                            }
                        })
                        .find(|n| {
                            matches!(
                                n.kind(),
                                SyntaxKind::STRING_LITERAL
                                    | SyntaxKind::INTEGER_LITERAL
                                    | SyntaxKind::FLOAT_LITERAL
                                    | SyntaxKind::PATH_EXPR
                                    | SyntaxKind::CALL_EXPR
                                    | SyntaxKind::BINARY_EXPR
                                    | SyntaxKind::UNARY_EXPR
                                    | SyntaxKind::PAREN_EXPR
                                    | SyntaxKind::IF_EXPR
                                    | SyntaxKind::BLOCK_EXPR
                                    | SyntaxKind::ARRAY_LITERAL
                                    | SyntaxKind::OBJECT_LITERAL
                                    | SyntaxKind::MAP_LITERAL
                                    | SyntaxKind::INDEX_EXPR
                                    | SyntaxKind::FIELD_ACCESS_EXPR
                                    | SyntaxKind::ENV_ACCESS_EXPR
                            )
                        })
                        .map(|n| self.lower_expr(&n))
                        .or_else(|| {
                            // Try to get value as a direct token (literal or identifier)
                            // Skip tokens before the colon
                            let mut seen_colon = false;
                            field_node
                                .children_with_tokens()
                                .filter_map(baml_compiler_syntax::NodeOrToken::into_token)
                                .find_map(|token| {
                                    if token.kind() == SyntaxKind::COLON {
                                        seen_colon = true;
                                        return None;
                                    }
                                    if !seen_colon {
                                        return None;
                                    }
                                    let span = token.text_range();
                                    match token.kind() {
                                        SyntaxKind::INTEGER_LITERAL => {
                                            let value = token.text().parse::<i64>().unwrap_or(0);
                                            Some(self.alloc_expr(
                                                Expr::Literal(Literal::Int(value)),
                                                span,
                                            ))
                                        }
                                        SyntaxKind::FLOAT_LITERAL => Some(self.alloc_expr(
                                            Expr::Literal(Literal::Float(token.text().to_string())),
                                            span,
                                        )),
                                        SyntaxKind::WORD => {
                                            let text = token.text();
                                            let expr = match text {
                                                "true" => self.alloc_expr(
                                                    Expr::Literal(Literal::Bool(true)),
                                                    span,
                                                ),
                                                "false" => self.alloc_expr(
                                                    Expr::Literal(Literal::Bool(false)),
                                                    span,
                                                ),
                                                "null" => self
                                                    .alloc_expr(Expr::Literal(Literal::Null), span),
                                                _ => self.alloc_expr(
                                                    Expr::Path(vec![Name::new(text)]),
                                                    span,
                                                ),
                                            };
                                            Some(expr)
                                        }
                                        _ => None,
                                    }
                                })
                        })
                        .unwrap_or_else(|| self.alloc_expr(Expr::Missing, field_span));

                    Some((key, value))
                })
                .collect();

        self.alloc_expr(Expr::Map { entries }, node.text_range())
    }

    fn try_lower_literal_token(
        &mut self,
        node: &baml_compiler_syntax::SyntaxNode,
    ) -> Option<ExprId> {
        // Check if this node contains a value token (literal or identifier)
        node.children_with_tokens()
            .filter_map(baml_compiler_syntax::NodeOrToken::into_token)
            .find_map(|token| self.lower_value_token(&token))
    }

    /// Lower a bare token (WORD, `INTEGER_LITERAL`, `FLOAT_LITERAL`) to an expression.
    fn lower_bare_token(&mut self, token: &baml_compiler_syntax::SyntaxToken) -> ExprId {
        self.lower_value_token(token)
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, token.text_range()))
    }

    /// Lower any value token to an expression.
    ///
    /// Handles all value tokens including:
    /// - Boolean literals: `true`, `false`
    /// - Null literal: `null`
    /// - Integer literals: `42`
    /// - Float literals: `3.14`
    /// - String literals: `"hello"`, `#"raw"#`
    /// - Variable references (WORD tokens that aren't literals)
    ///
    /// Returns `None` for non-value tokens (operators, brackets, etc.).
    fn lower_value_token(&mut self, token: &baml_compiler_syntax::SyntaxToken) -> Option<ExprId> {
        use baml_compiler_syntax::SyntaxKind;

        let span = token.text_range();
        match token.kind() {
            SyntaxKind::WORD => {
                let text = token.text();
                let expr = match text {
                    "true" => Expr::Literal(Literal::Bool(true)),
                    "false" => Expr::Literal(Literal::Bool(false)),
                    "null" => Expr::Literal(Literal::Null),
                    _ => Expr::Path(vec![Name::new(text)]),
                };
                Some(self.alloc_expr(expr, span))
            }
            SyntaxKind::INTEGER_LITERAL => {
                let value = token.text().parse::<i64>().unwrap_or(0);
                Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span))
            }
            SyntaxKind::FLOAT_LITERAL => Some(self.alloc_expr(
                Expr::Literal(Literal::Float(token.text().to_string())),
                span,
            )),
            SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                let content = strip_string_delimiters(token.text());
                Some(self.alloc_expr(Expr::Literal(Literal::String(content.to_string())), span))
            }
            _ => None,
        }
    }

    /// Try to lower token content inside a parenthesized expression.
    ///
    /// This handles the case where `PAREN_EXPR` contains only tokens (no child nodes),
    /// such as `(b)` where `b` is a variable reference, or `(42)` where 42 is a literal.
    fn try_lower_paren_token_content(
        &mut self,
        node: &baml_compiler_syntax::SyntaxNode,
    ) -> Option<ExprId> {
        use baml_compiler_syntax::SyntaxKind;

        // Look for value tokens inside the parentheses (skip L_PAREN and R_PAREN)
        for elem in node.children_with_tokens() {
            if let Some(token) = elem.into_token() {
                // Skip structural tokens
                if matches!(
                    token.kind(),
                    SyntaxKind::L_PAREN | SyntaxKind::R_PAREN | SyntaxKind::WHITESPACE
                ) {
                    continue;
                }
                // Try to lower as a value token
                if let Some(expr_id) = self.lower_value_token(&token) {
                    return Some(expr_id);
                }
            }
        }
        None
    }

    fn lower_let_stmt(
        &mut self,
        node: &baml_compiler_syntax::SyntaxNode,
        is_watched: bool,
    ) -> StmtId {
        // Use the LetStmt AST wrapper for cleaner access
        let let_stmt = baml_compiler_syntax::ast::LetStmt::cast(node.clone());

        // Extract pattern (variable name)
        let pattern = let_stmt
            .as_ref()
            .and_then(baml_compiler_syntax::LetStmt::name)
            .map(|token| {
                let name_str = token.text();
                self.add_name_to_scope(name_str);
                let name = Name::new(name_str);
                self.patterns.alloc(Pattern::Binding(name))
            })
            .unwrap_or_else(|| {
                self.alloc_pattern(
                    Pattern::Binding(Name::new("missing_let")),
                    node.text_range(),
                )
            });

        let type_node = let_stmt
            .as_ref()
            .and_then(baml_compiler_syntax::LetStmt::ty);

        // Extract type annotation if present, allocating it in the arena
        let type_annotation = type_node.map(|t: TypeExpr| {
            let type_ref = TypeRef::from_ast(&t);
            self.alloc_type(type_ref, t.text_range())
        });

        // Extract initializer expression - first try as a node, then as a token
        let initializer = let_stmt
            .as_ref()
            .and_then(baml_compiler_syntax::LetStmt::initializer)
            .map(|n| self.lower_expr(&n))
            .or_else(|| {
                // Try to get initializer as a direct token (for simple literals/vars)
                let_stmt
                    .as_ref()
                    .and_then(baml_compiler_syntax::LetStmt::initializer_token)
                    .and_then(|token| self.lower_value_token(&token))
            });

        self.alloc_stmt(
            Stmt::Let {
                pattern,
                type_annotation,
                initializer,
                is_watched,
            },
            node.text_range(),
        )
    }

    fn lower_return_stmt(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> StmtId {
        use baml_compiler_syntax::SyntaxKind;

        // RETURN_STMT structure: return keyword, optional expression (which might be a node or a direct token)
        let return_value = if let Some(child_node) = node.children().find(|n| {
            matches!(
                n.kind(),
                SyntaxKind::EXPR
                    | SyntaxKind::BINARY_EXPR
                    | SyntaxKind::UNARY_EXPR
                    | SyntaxKind::CALL_EXPR
                    | SyntaxKind::PATH_EXPR
                    | SyntaxKind::FIELD_ACCESS_EXPR
                    | SyntaxKind::ENV_ACCESS_EXPR
                    | SyntaxKind::INDEX_EXPR
                    | SyntaxKind::IF_EXPR
                    | SyntaxKind::BLOCK_EXPR
                    | SyntaxKind::PAREN_EXPR
                    | SyntaxKind::STRING_LITERAL
                    | SyntaxKind::RAW_STRING_LITERAL
            )
        }) {
            Some(self.lower_expr(&child_node))
        } else {
            // Check for direct tokens (literals, identifiers)
            node.children_with_tokens()
                .filter_map(baml_compiler_syntax::NodeOrToken::into_token)
                .find_map(|token| {
                    let span = token.text_range();
                    match token.kind() {
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span))
                        }
                        SyntaxKind::FLOAT_LITERAL => Some(self.alloc_expr(
                            Expr::Literal(Literal::Float(token.text().to_string())),
                            span,
                        )),
                        SyntaxKind::WORD => {
                            let text = token.text();
                            let expr_id = match text {
                                "true" => self.alloc_expr(Expr::Literal(Literal::Bool(true)), span),
                                "false" => {
                                    self.alloc_expr(Expr::Literal(Literal::Bool(false)), span)
                                }
                                "null" => self.alloc_expr(Expr::Literal(Literal::Null), span),
                                _ => self.alloc_expr(Expr::Path(vec![Name::new(text)]), span),
                            };
                            Some(expr_id)
                        }
                        SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                            let text = token.text();
                            // Strip quotes
                            let content = if text.starts_with("#\"") && text.ends_with("\"#") {
                                &text[2..text.len() - 2]
                            } else if text.starts_with('"') && text.ends_with('"') {
                                &text[1..text.len() - 1]
                            } else {
                                text
                            };
                            Some(self.alloc_expr(
                                Expr::Literal(Literal::String(content.to_string())),
                                span,
                            ))
                        }
                        _ => None,
                    }
                })
        };

        self.alloc_stmt(Stmt::Return(return_value), node.text_range())
    }

    fn lower_assert_stmt(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> StmtId {
        use baml_compiler_syntax::SyntaxKind;

        // ASSERT_STMT structure: assert keyword, expression
        let condition = node
            .children()
            .find(|n| {
                matches!(
                    n.kind(),
                    SyntaxKind::EXPR
                        | SyntaxKind::BINARY_EXPR
                        | SyntaxKind::UNARY_EXPR
                        | SyntaxKind::CALL_EXPR
                        | SyntaxKind::PATH_EXPR
                        | SyntaxKind::FIELD_ACCESS_EXPR
                        | SyntaxKind::ENV_ACCESS_EXPR
                        | SyntaxKind::INDEX_EXPR
                        | SyntaxKind::IF_EXPR
                        | SyntaxKind::BLOCK_EXPR
                        | SyntaxKind::PAREN_EXPR
                )
            })
            .map(|n| self.lower_expr(&n))
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        self.alloc_stmt(Stmt::Assert { condition }, node.text_range())
    }

    fn lower_while_stmt(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> StmtId {
        use baml_compiler_syntax::SyntaxKind;

        // Use the WhileStmt AST wrapper for cleaner access
        let while_stmt = baml_compiler_syntax::ast::WhileStmt::cast(node.clone());

        // Get the raw condition node to check if it's wrapped in parentheses
        let condition_node = while_stmt
            .as_ref()
            .and_then(baml_compiler_syntax::WhileStmt::condition);

        // Validate that condition is wrapped in parentheses
        if let Some(ref cond) = condition_node {
            if cond.kind() != SyntaxKind::PAREN_EXPR {
                self.push_diagnostic(HirDiagnostic::MissingConditionParens {
                    kind: "while",
                    span: self.span_from_node(cond),
                });
            }
        }

        let condition = condition_node
            .map(|n| self.lower_expr(&n))
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        let body = while_stmt
            .and_then(|w| w.body())
            .map(|block| self.lower_block_expr(&block))
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        self.stmts.alloc(Stmt::While {
            condition,
            body,
            after: None,
            origin: LoopOrigin::While,
        })
    }

    fn lower_for_stmt(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> StmtId {
        // Use the ForExpr AST wrapper for cleaner access
        let for_expr = baml_compiler_syntax::ast::ForExpr::cast(node.clone());

        let Some(for_expr) = for_expr else {
            return self.alloc_stmt(Stmt::Missing, node.text_range());
        };

        // Note: We pass the body AST node to desugar functions, which will:
        // 1. Generate their synthetic names FIRST (claiming _iter, _len, _i)
        // 2. THEN lower the body (inner loops will get _iter1, etc.)
        // This ensures outer loops get simpler names than inner loops.
        if for_expr.is_iterator_style() {
            // Iterator-style: for (let i in items) { ... }
            // Desugar into a while loop
            self.desugar_for_in(&for_expr)
        } else {
            // C-style: for (let i = 0; i < 10; i += 1) { ... }
            // Desugar into a while loop
            self.desugar_c_style_for(&for_expr)
        }
    }

    /// Desugar a C-style `for (init; cond; update) { body }` loop into a while loop.
    ///
    /// The transformation is:
    /// ```text
    /// for (init; cond; update) { body }
    /// ```
    /// becomes:
    /// ```text
    /// {
    ///     init;
    ///     while (cond) {
    ///         body
    ///         // after: update (runs even on continue)
    ///     }
    /// }
    /// ```
    ///
    /// If there's no condition, it becomes `while (true)` (infinite loop).
    fn desugar_c_style_for(&mut self, for_expr: &baml_compiler_syntax::ast::ForExpr) -> StmtId {
        // 1. Lower the initializer (if present)
        let initializer = for_expr
            .let_stmt()
            .map(|let_stmt| self.lower_let_stmt(let_stmt.syntax(), false));

        // 2. Lower the condition, or default to `true` for infinite loop
        let condition = for_expr
            .condition()
            .map(|n| self.lower_expr(&n))
            .or_else(|| {
                for_expr
                    .condition_token()
                    .map(|token| self.lower_bare_token(&token))
            })
            .unwrap_or_else(|| self.exprs.alloc(Expr::Literal(Literal::Bool(true))));

        // 3. Get the update AST node (we'll lower it multiple times as needed)
        let update_ast = for_expr.update();

        // 4. Lower the body AFTER processing init/condition/update
        // This ensures outer loops' synthetic names are claimed before inner loops
        let user_body = for_expr
            .body()
            .map(|block| self.lower_block_expr(&block))
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, for_expr.syntax().text_range()));

        // 5. Create the while loop body
        // If there's an update, we need to:
        // a) Transform all `continue` in the body to `{ update; continue; }`
        // b) Add the update at the end of the body
        let while_body = if let Some(ref update_node) = update_ast {
            // Transform continues to include the update (re-lowers the AST each time)
            let transformed_body =
                self.transform_continues_in_expr_with_update(user_body, update_node);
            // Lower the update for the end of body
            let after_at_end = self.lower_update_stmt(update_node);
            // If the body is already a block, flatten its statements into the new block
            // to avoid unnecessary nesting like `{ { body }; update; }`
            if let Expr::Block { stmts, tail_expr } = &self.exprs[transformed_body] {
                let mut new_stmts = stmts.clone();
                // If there's a tail expression, convert it to a statement first
                if let Some(tail) = tail_expr {
                    new_stmts.push(self.stmts.alloc(Stmt::Expr(*tail)));
                }
                new_stmts.push(after_at_end);
                self.exprs.alloc(Expr::Block {
                    stmts: new_stmts,
                    tail_expr: None,
                })
            } else {
                // Body is not a block, wrap it
                let body_stmt = self.stmts.alloc(Stmt::Expr(transformed_body));
                self.exprs.alloc(Expr::Block {
                    stmts: vec![body_stmt, after_at_end],
                    tail_expr: None,
                })
            }
        } else {
            // No update - just use the body as-is (it's typically already a block)
            user_body
        };

        let while_stmt = self.stmts.alloc(Stmt::While {
            condition,
            body: while_body,
            after: None,
            origin: LoopOrigin::ForLoop,
        });

        // 5. Wrap in outer block with initializer
        let mut outer_stmts = Vec::new();
        if let Some(init) = initializer {
            outer_stmts.push(init);
        }
        outer_stmts.push(while_stmt);

        let outer_block = self.exprs.alloc(Expr::Block {
            stmts: outer_stmts,
            tail_expr: None,
        });

        self.stmts.alloc(Stmt::Expr(outer_block))
    }

    /// Lower an update expression AST node to a statement.
    fn lower_update_stmt(&mut self, update_node: &baml_compiler_syntax::SyntaxNode) -> StmtId {
        if let Some(assign_stmt) = self.try_lower_assignment(update_node) {
            assign_stmt
        } else {
            let expr = self.lower_expr(update_node);
            self.stmts.alloc(Stmt::Expr(expr))
        }
    }

    /// Transform an expression, replacing all `continue` statements with
    /// `{ update; continue; }` by re-lowering the update AST each time.
    fn transform_continues_in_expr_with_update(
        &mut self,
        expr_id: ExprId,
        update_ast: &baml_compiler_syntax::SyntaxNode,
    ) -> ExprId {
        let expr = self.exprs[expr_id].clone();
        let new_expr = match expr {
            Expr::Block { stmts, tail_expr } => {
                let new_stmts = stmts
                    .iter()
                    .map(|s| self.transform_continues_in_stmt_with_update(*s, update_ast))
                    .collect();
                let new_tail =
                    tail_expr.map(|e| self.transform_continues_in_expr_with_update(e, update_ast));
                Expr::Block {
                    stmts: new_stmts,
                    tail_expr: new_tail,
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let new_then =
                    self.transform_continues_in_expr_with_update(then_branch, update_ast);
                let new_else = else_branch
                    .map(|e| self.transform_continues_in_expr_with_update(e, update_ast));
                Expr::If {
                    condition,
                    then_branch: new_then,
                    else_branch: new_else,
                }
            }
            // Other expressions don't contain statements
            _ => return expr_id, // Return original, no transformation needed
        };
        self.exprs.alloc(new_expr)
    }

    /// Transform a statement, replacing `continue` with `{ update; continue; }`.
    fn transform_continues_in_stmt_with_update(
        &mut self,
        stmt_id: StmtId,
        update_ast: &baml_compiler_syntax::SyntaxNode,
    ) -> StmtId {
        // Clone the statement to avoid borrow checker issues when calling mutable methods
        let stmt = self.stmts[stmt_id].clone();
        match stmt {
            Stmt::Continue => {
                // Replace continue with { update; continue; }
                let update_stmt = self.lower_update_stmt(update_ast);
                let continue_stmt = self.stmts.alloc(Stmt::Continue);
                let block = self.exprs.alloc(Expr::Block {
                    stmts: vec![update_stmt, continue_stmt],
                    tail_expr: None,
                });
                self.stmts.alloc(Stmt::Expr(block))
            }
            Stmt::Expr(expr_id) => {
                let new_expr = self.transform_continues_in_expr_with_update(expr_id, update_ast);
                if new_expr == expr_id {
                    stmt_id // No change
                } else {
                    self.stmts.alloc(Stmt::Expr(new_expr))
                }
            }
            Stmt::While { .. } => {
                // Don't transform inside nested loops - their continues refer to the inner loop
                stmt_id
            }
            // Other statements don't contain continues
            _ => stmt_id,
        }
    }

    /// Desugar a `for (let x in arr) { body }` loop into a while loop.
    ///
    /// The transformation is:
    /// ```text
    /// for (let x in arr) { body }
    /// ```
    /// becomes:
    /// ```text
    /// {
    ///     let _arr_N = arr;
    ///     let _len_N = _arr_N.length();
    ///     let _i_N = 0;
    ///     while (_i_N < _len_N) {
    ///         let x = _arr_N[_i_N];
    ///         _i_N += 1;
    ///         body
    ///     }
    /// }
    /// ```
    fn desugar_for_in(&mut self, for_expr: &baml_compiler_syntax::ast::ForExpr) -> StmtId {
        // Get the for expression's range for synthetic expressions, skipping leading trivia
        // This ensures errors point to the actual `for` keyword, not preceding comments
        let for_range = Self::text_range_skip_trivia(for_expr.syntax());

        // Generate unique names for synthetic variables FIRST
        // This ensures outer loops claim _iter, _len, _i before inner loops
        let arr_name = self.gensym("iter");
        let len_name = self.gensym("len");
        let idx_name = self.gensym("i");

        // Now lower the body - inner for-loops will get _iter1, _len1, _i1, etc.
        let user_body = for_expr
            .body()
            .map(|block| self.lower_block_expr(&block))
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, for_range));

        // 1. let _arr_N = <iterator>
        // First try to get iterator as a child node (for complex expressions like arrays, calls, etc.)
        // If not found, look for a bare WORD token (simple identifier like `xs`)
        let iterator_expr = for_expr
            .iterator()
            .map(|n| self.lower_expr(&n))
            .or_else(|| {
                // Look for a bare WORD token after 'in' keyword
                // The iterator could be a simple identifier that wasn't wrapped in a node
                use baml_compiler_syntax::SyntaxKind;
                let mut seen_in = false;
                for element in for_expr.syntax().children_with_tokens() {
                    match element {
                        baml_compiler_syntax::NodeOrToken::Token(token) => {
                            if token.kind() == SyntaxKind::KW_IN {
                                seen_in = true;
                            } else if seen_in && token.kind() == SyntaxKind::WORD {
                                // Found the iterator identifier - use token's range
                                return Some(self.alloc_expr(
                                    Expr::Path(vec![Name::new(token.text())]),
                                    token.text_range(),
                                ));
                            }
                        }
                        baml_compiler_syntax::NodeOrToken::Node(_) => {}
                    }
                }
                None
            })
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, for_range));

        let arr_pat = self.patterns.alloc(Pattern::Binding(arr_name.clone()));
        let arr_let = self.stmts.alloc(Stmt::Let {
            pattern: arr_pat,
            type_annotation: None,
            initializer: Some(iterator_expr),
            is_watched: false,
        });

        // 2. let _len_N = _arr_N.length()
        // This is a method call: FieldAccess followed by Call with no arguments.
        // The typechecker will resolve `length` as a method on arrays.
        // Use for_range for synthetic expressions so errors point to the for statement.
        let arr_ref = self.alloc_expr(Expr::Path(vec![arr_name.clone()]), for_range);
        let length_method = self.alloc_expr(
            Expr::FieldAccess {
                base: arr_ref,
                field: Name::new("length"),
            },
            for_range,
        );
        let length_call = self.alloc_expr(
            Expr::Call {
                callee: length_method,
                args: vec![],
            },
            for_range,
        );
        let len_pat = self.patterns.alloc(Pattern::Binding(len_name.clone()));
        let len_let = self.stmts.alloc(Stmt::Let {
            pattern: len_pat,
            type_annotation: None,
            initializer: Some(length_call),
            is_watched: false,
        });

        // 3. let _i_N = 0
        let zero = self.alloc_expr(Expr::Literal(Literal::Int(0)), for_range);
        let idx_pat = self.patterns.alloc(Pattern::Binding(idx_name.clone()));
        let idx_let = self.stmts.alloc(Stmt::Let {
            pattern: idx_pat,
            type_annotation: None,
            initializer: Some(zero),
            is_watched: false,
        });

        // 4. Condition: _i_N < _len_N
        let idx_ref = self.alloc_expr(Expr::Path(vec![idx_name.clone()]), for_range);
        let len_ref = self.alloc_expr(Expr::Path(vec![len_name]), for_range);
        let condition = self.alloc_expr(
            Expr::Binary {
                op: BinaryOp::Lt,
                lhs: idx_ref,
                rhs: len_ref,
            },
            for_range,
        );

        // 5. Loop body: let x = _arr_N[_i_N]
        let user_pattern = for_expr
            .let_stmt()
            .and_then(|ls| ls.name())
            .map(|n| {
                self.add_name_to_scope(n.text());
                self.patterns.alloc(Pattern::Binding(Name::new(n.text())))
            })
            .or_else(|| {
                for_expr.loop_var().map(|n| {
                    self.add_name_to_scope(n.text());
                    self.patterns.alloc(Pattern::Binding(Name::new(n.text())))
                })
            })
            .unwrap_or_else(|| self.patterns.alloc(Pattern::Binding(Name::new("_"))));

        let arr_ref2 = self.alloc_expr(Expr::Path(vec![arr_name]), for_range);
        let idx_ref2 = self.alloc_expr(Expr::Path(vec![idx_name.clone()]), for_range);
        let element_access = self.alloc_expr(
            Expr::Index {
                base: arr_ref2,
                index: idx_ref2,
            },
            for_range,
        );
        let elem_let = self.stmts.alloc(Stmt::Let {
            pattern: user_pattern,
            type_annotation: None,
            initializer: Some(element_access),
            is_watched: false,
        });

        // 6. Increment: _i_N += 1
        let idx_target = self.alloc_expr(Expr::Path(vec![idx_name]), for_range);
        let one = self.alloc_expr(Expr::Literal(Literal::Int(1)), for_range);
        let idx_assign = self.stmts.alloc(Stmt::AssignOp {
            target: idx_target,
            op: AssignOp::Add,
            value: one,
        });

        // 7. Assemble while body: [elem_let, idx_assign, ...user_body_stmts]
        // Note: increment after elem_let so `continue` works correctly
        // Flatten the user body if it's already a block to avoid unnecessary nesting
        let while_body = if let Expr::Block { stmts, tail_expr } = &self.exprs[user_body] {
            let mut body_stmts = vec![elem_let, idx_assign];
            body_stmts.extend(stmts.iter().copied());
            // If there's a tail expression, convert it to a statement
            if let Some(tail) = tail_expr {
                body_stmts.push(self.stmts.alloc(Stmt::Expr(*tail)));
            }
            self.alloc_expr(
                Expr::Block {
                    stmts: body_stmts,
                    tail_expr: None,
                },
                for_range,
            )
        } else {
            let body_stmt = self.stmts.alloc(Stmt::Expr(user_body));
            self.alloc_expr(
                Expr::Block {
                    stmts: vec![elem_let, idx_assign, body_stmt],
                    tail_expr: None,
                },
                for_range,
            )
        };

        // 8. While statement with ForLoop origin
        // Note: idx_assign is in the body, so no separate after statement needed
        let while_stmt = self.stmts.alloc(Stmt::While {
            condition,
            body: while_body,
            after: None,
            origin: LoopOrigin::ForLoop,
        });

        // 9. Wrap in outer block
        let outer_block = self.alloc_expr(
            Expr::Block {
                stmts: vec![arr_let, len_let, idx_let, while_stmt],
                tail_expr: None,
            },
            for_range,
        );

        self.stmts.alloc(Stmt::Expr(outer_block))
    }

    /// Lower a header comment (`//# name`) to a `HeaderComment` statement.
    fn lower_header_comment(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> StmtId {
        use baml_compiler_syntax::SyntaxKind;

        // Count the # tokens to determine level, and collect the title text
        let mut level = 0;
        let mut title_parts = Vec::new();
        let mut in_title = false;

        for child in node.children_with_tokens() {
            if let rowan::NodeOrToken::Token(token) = child {
                match token.kind() {
                    SyntaxKind::HASH => {
                        if !in_title {
                            level += 1;
                        }
                    }
                    SyntaxKind::SLASH => {
                        // Skip the // prefix
                    }
                    SyntaxKind::WHITESPACE => {
                        // First whitespace after # marks start of title
                        if level > 0 {
                            in_title = true;
                        }
                    }
                    _ => {
                        // Any other token is part of the title
                        in_title = true;
                        title_parts.push(token.text().to_string());
                    }
                }
            }
        }

        let name = title_parts.join("").trim().to_string();
        let name = Name::new(&name);

        self.alloc_stmt(Stmt::HeaderComment { name, level }, node.text_range())
    }

    /// Lower a `CONFIG_VALUE` node to an expression.
    ///
    /// `CONFIG_VALUE` can contain various expression types or legacy unquoted strings.
    fn lower_config_value(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> ExprId {
        use baml_compiler_syntax::SyntaxKind;

        // First check for child expression nodes
        for child in node.children() {
            match child.kind() {
                SyntaxKind::STRING_LITERAL
                | SyntaxKind::RAW_STRING_LITERAL
                | SyntaxKind::BINARY_EXPR
                | SyntaxKind::PATH_EXPR
                | SyntaxKind::ENV_ACCESS_EXPR
                | SyntaxKind::CALL_EXPR
                | SyntaxKind::MAP_LITERAL => {
                    return self.lower_expr(&child);
                }
                // Config arrays have CONFIG_VALUE or CONFIG_BLOCK children, not expression children
                SyntaxKind::ARRAY_LITERAL => {
                    return self.lower_config_array(&child);
                }
                // Nested config block becomes a map
                SyntaxKind::CONFIG_BLOCK => {
                    if let Some(block) = baml_compiler_syntax::ast::ConfigBlock::cast(child.clone())
                    {
                        return self.lower_config_block_to_map_expr(&block);
                    }
                }
                _ => {}
            }
        }

        // Check for literal tokens directly under CONFIG_VALUE
        for item in node.children_with_tokens() {
            if let rowan::NodeOrToken::Token(token) = item {
                match token.kind() {
                    SyntaxKind::INTEGER_LITERAL => {
                        if let Ok(val) = token.text().parse::<i64>() {
                            return self
                                .alloc_expr(Expr::Literal(Literal::Int(val)), token.text_range());
                        }
                    }
                    SyntaxKind::FLOAT_LITERAL => {
                        return self.alloc_expr(
                            Expr::Literal(Literal::Float(token.text().to_string())),
                            token.text_range(),
                        );
                    }
                    SyntaxKind::WORD => {
                        let text = token.text();
                        if text == "true" {
                            return self.alloc_expr(
                                Expr::Literal(Literal::Bool(true)),
                                token.text_range(),
                            );
                        } else if text == "false" {
                            return self.alloc_expr(
                                Expr::Literal(Literal::Bool(false)),
                                token.text_range(),
                            );
                        }
                        // Single word - treat as string (legacy unquoted string)
                        return self.alloc_expr(
                            Expr::Literal(Literal::String(text.to_string())),
                            token.text_range(),
                        );
                    }
                    _ => {}
                }
            }
        }

        // Fall back: collect all text content as a string (legacy unquoted strings)
        let text: String = node
            .descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|t| {
                !matches!(
                    t.kind(),
                    SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE | SyntaxKind::COMMA
                )
            })
            .map(|t| t.text().to_string())
            .collect();

        if text.is_empty() {
            self.alloc_expr(Expr::Missing, node.text_range())
        } else {
            self.alloc_expr(Expr::Literal(Literal::String(text)), node.text_range())
        }
    }

    /// Lower a config array to an array expression.
    ///
    /// Config arrays have `CONFIG_VALUE` or `CONFIG_BLOCK` children (not regular expression children).
    fn lower_config_array(&mut self, node: &baml_compiler_syntax::SyntaxNode) -> ExprId {
        use baml_compiler_syntax::SyntaxKind;

        let elements: Vec<ExprId> = node
            .children()
            .filter_map(|child| match child.kind() {
                SyntaxKind::CONFIG_VALUE => Some(self.lower_config_value(&child)),
                SyntaxKind::CONFIG_BLOCK => baml_compiler_syntax::ast::ConfigBlock::cast(child)
                    .map(|block| self.lower_config_block_to_map_expr(&block)),
                _ => None,
            })
            .collect();

        self.alloc_expr(Expr::Array { elements }, node.text_range())
    }

    /// Lower a config block to a map expression.
    fn lower_config_block_to_map_expr(
        &mut self,
        block: &baml_compiler_syntax::ast::ConfigBlock,
    ) -> ExprId {
        let entries: Vec<(ExprId, ExprId)> = block
            .items()
            .filter_map(|item| {
                let key = item.key()?;
                let key_text = key.text().to_string();

                let key_expr =
                    self.alloc_expr(Expr::Literal(Literal::String(key_text)), key.text_range());

                let value_expr = if let Some(config_value_node) = item.config_value_node() {
                    self.lower_config_value(&config_value_node)
                } else if let Some(nested_block) = item.nested_block() {
                    self.lower_config_block_to_map_expr(&nested_block)
                } else {
                    self.alloc_expr(Expr::Missing, key.text_range())
                };

                Some((key_expr, value_expr))
            })
            .collect();

        self.alloc_expr(Expr::Map { entries }, block.syntax().text_range())
    }
}
