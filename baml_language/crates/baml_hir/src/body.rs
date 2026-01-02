//! Function bodies - either LLM prompts or expression IR.
//!
//! The CST already distinguishes `LLM_FUNCTION_BODY` from `EXPR_FUNCTION_BODY`,
//! so we just need to lower each type appropriately.

use std::{collections::HashMap, sync::Arc};

use baml_base::{FileId, Span};
use baml_syntax::TypeExpr;
use la_arena::{Arena, Idx};
use rowan::{TextRange, ast::AstNode};

use crate::{Name, type_ref::TypeRef};

/// The body of a function - determined by CST node type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionBody {
    /// LLM function: has `LLM_FUNCTION_BODY` in CST
    Llm(LlmBody),

    /// Expression function: has `EXPR_FUNCTION_BODY` in CST
    Expr(ExprBody),

    /// Function has no body (error recovery)
    Missing,
}

/// Body of an LLM function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmBody {
    /// The client to use (e.g., "GPT4")
    pub client: Option<Name>,

    /// The prompt template
    pub prompt: Option<PromptTemplate>,
}

/// A prompt template with interpolations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptTemplate {
    /// The raw prompt string (may contain {{ }} interpolations)
    pub text: String,

    /// Parsed interpolation expressions
    pub interpolations: Vec<Interpolation>,
}

/// A {{ var }} interpolation in a prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interpolation {
    /// Variable name referenced
    pub var_name: Name,

    /// Source offset in the prompt string
    pub offset: u32,
}

/// Body of an expression function (turing-complete).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprBody {
    /// Expression arena
    pub exprs: Arena<Expr>,

    /// Statement arena
    pub stmts: Arena<Stmt>,

    /// Pattern arena (for let bindings, match arms, etc.)
    pub patterns: Arena<Pattern>,

    /// Root expression of the function body (usually a `BLOCK_EXPR`)
    pub root_expr: Option<ExprId>,

    // ========================================================================
    // Span tracking (for accurate error messages)
    // ========================================================================
    /// Spans for expressions
    pub expr_spans: HashMap<ExprId, Span>,

    /// Spans for statements
    pub stmt_spans: HashMap<StmtId, Span>,

    /// Spans for patterns
    pub pattern_spans: HashMap<PatId, Span>,

    /// Spans for match arms: maps match expression ID to its arm spans.
    /// Each entry is (`arm_span`, `pattern_span`) for each arm in order.
    pub match_arm_spans: HashMap<ExprId, Vec<MatchArmSpans>>,
}

/// Span information for a single match arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchArmSpans {
    /// Span of the entire arm (pattern + guard + body)
    pub arm_span: Span,
    /// Span of just the pattern
    pub pattern_span: Span,
}

impl ExprBody {
    /// Get the span of an expression, if available.
    pub fn get_expr_span(&self, expr_id: ExprId) -> Option<Span> {
        self.expr_spans.get(&expr_id).copied()
    }

    /// Get the span of a statement, if available.
    pub fn get_stmt_span(&self, stmt_id: StmtId) -> Option<Span> {
        self.stmt_spans.get(&stmt_id).copied()
    }

    /// Get the span of a pattern, if available.
    pub fn get_pattern_span(&self, pat_id: PatId) -> Option<Span> {
        self.pattern_spans.get(&pat_id).copied()
    }

    /// Get the arm spans for a match expression, if available.
    pub fn get_match_arm_spans(&self, match_expr_id: ExprId) -> Option<&[MatchArmSpans]> {
        self.match_arm_spans.get(&match_expr_id).map(Vec::as_slice)
    }
}

// IDs for arena indices
pub type ExprId = Idx<Expr>;
pub type StmtId = Idx<Stmt>;
pub type PatId = Idx<Pattern>;

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
        arms: Vec<MatchArm>,
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

    /// Object constructor: `Point { x: 1, y: 2 }`
    Object {
        type_name: Option<Name>,
        fields: Vec<(Name, ExprId)>,
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
    Let {
        pattern: PatId,
        type_annotation: Option<crate::type_ref::TypeRef>,
        type_span: Option<TextRange>,
        initializer: Option<ExprId>,
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

    /// Missing/error statement
    Missing,
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
    pub fn lower(func_node: &baml_syntax::ast::FunctionDef, file_id: FileId) -> Arc<FunctionBody> {
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
            Arc::new(FunctionBody::Llm(Self::lower_llm_body(&llm_body)))
        } else if let Some(expr_body) = func_node.expr_body() {
            Arc::new(FunctionBody::Expr(Self::lower_expr_body(
                &expr_body,
                file_id,
                &param_names,
            )))
        } else {
            Arc::new(FunctionBody::Missing)
        }
    }

    fn lower_llm_body(llm_body: &baml_syntax::ast::LlmFunctionBody) -> LlmBody {
        let mut client = None;
        let mut prompt = None;

        // Extract client from CLIENT_FIELD
        for child in llm_body.syntax().children() {
            if child.kind() == baml_syntax::SyntaxKind::CLIENT_FIELD {
                // CLIENT_FIELD has: KW_CLIENT "client" WORD "GPT4"
                if let Some(client_name) = child
                    .children_with_tokens()
                    .filter_map(baml_syntax::NodeOrToken::into_token)
                    .filter(|t| t.kind() == baml_syntax::SyntaxKind::WORD)
                    .nth(0)
                {
                    client = Some(Name::new(client_name.text()));
                }
            } else if child.kind() == baml_syntax::SyntaxKind::PROMPT_FIELD {
                // PROMPT_FIELD has: WORD "prompt" RAW_STRING_LITERAL (node, not token!)
                // The RAW_STRING_LITERAL node contains the full text including delimiters
                if let Some(prompt_node) = child
                    .children()
                    .find(|n| n.kind() == baml_syntax::SyntaxKind::RAW_STRING_LITERAL)
                {
                    let text = prompt_node.text().to_string();
                    prompt = Some(Self::parse_prompt(&text));
                }
            }
        }

        LlmBody { client, prompt }
    }

    fn parse_prompt(prompt_text: &str) -> PromptTemplate {
        // Strip #"..."# or "..." delimiters
        let prompt_text = prompt_text.trim();
        let content = if prompt_text.starts_with("#\"") && prompt_text.ends_with("\"#") {
            &prompt_text[2..prompt_text.len() - 2]
        } else if prompt_text.starts_with('"') && prompt_text.ends_with('"') {
            &prompt_text[1..prompt_text.len() - 1]
        } else {
            prompt_text
        };

        // Parse {{ var }} interpolations
        let interpolations = Self::parse_interpolations(content);

        PromptTemplate {
            text: content.to_string(),
            interpolations,
        }
    }

    fn parse_interpolations(prompt: &str) -> Vec<Interpolation> {
        let mut interpolations = Vec::new();
        let mut offset = 0;

        while let Some(start) = prompt[offset..].find("{{") {
            let abs_start = offset + start;
            if let Some(end) = prompt[abs_start..].find("}}") {
                let abs_end = abs_start + end;
                let var_text = prompt[abs_start + 2..abs_end].trim();

                #[allow(clippy::cast_possible_truncation)]
                interpolations.push(Interpolation {
                    var_name: Name::new(var_text),
                    offset: abs_start as u32, // Prompt strings are unlikely to exceed 4GB
                });

                offset = abs_end + 2;
            } else {
                break;
            }
        }

        interpolations
    }

    fn lower_expr_body(
        expr_body: &baml_syntax::ast::ExprFunctionBody,
        file_id: FileId,
        param_names: &[String],
    ) -> ExprBody {
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
            .find_map(baml_syntax::ast::BlockExpr::cast)
            .map(|block| ctx.lower_block_expr(&block));

        ctx.finish(root_expr)
    }
}

struct LoweringContext {
    exprs: Arena<Expr>,
    stmts: Arena<Stmt>,
    patterns: Arena<Pattern>,
    /// File ID for creating spans
    file_id: FileId,
    /// All names used in this function, for generating unique synthetic variable names.
    names_in_scope: std::collections::HashSet<String>,

    // Span tracking
    /// Span tracking for expressions
    expr_spans: HashMap<ExprId, Span>,
    /// Span tracking for statements
    stmt_spans: HashMap<StmtId, Span>,
    /// Span tracking for patterns
    pattern_spans: HashMap<PatId, Span>,
    /// Span tracking for match arms (maps match expr ID to arm spans)
    match_arm_spans: HashMap<ExprId, Vec<MatchArmSpans>>,
}

/// Helper enum for building pattern elements during lowering.
/// Used to track partial state while scanning tokens in a pattern.
enum PatternElement {
    /// Simple identifier (could become binding or enum start)
    Ident(Name),
    /// Seen `EnumName.` - waiting for variant name
    EnumStart(Name),
    /// Seen `name:` - waiting for type expression
    TypedBindingStart(Name),
}

impl LoweringContext {
    fn new(file_id: FileId) -> Self {
        Self {
            exprs: Arena::new(),
            stmts: Arena::new(),
            patterns: Arena::new(),
            file_id,
            names_in_scope: std::collections::HashSet::new(),
            expr_spans: HashMap::new(),
            stmt_spans: HashMap::new(),
            pattern_spans: HashMap::new(),
            match_arm_spans: HashMap::new(),
        }
    }

    /// Create a span from a syntax node's text range.
    fn span_from_node(&self, node: &baml_syntax::SyntaxNode) -> Span {
        Span::new(self.file_id, node.text_range())
    }

    /// Create a span from a text range.
    fn span_from_range(&self, range: TextRange) -> Span {
        Span::new(self.file_id, range)
    }

    fn alloc_expr(&mut self, expr: Expr, range: TextRange) -> ExprId {
        let id = self.exprs.alloc(expr);
        self.expr_spans.insert(id, self.span_from_range(range));
        id
    }

    fn alloc_stmt(&mut self, stmt: Stmt, range: TextRange) -> StmtId {
        let id = self.stmts.alloc(stmt);
        self.stmt_spans.insert(id, self.span_from_range(range));
        id
    }

    fn alloc_pattern(&mut self, pattern: Pattern, range: TextRange) -> PatId {
        let id = self.patterns.alloc(pattern);
        self.pattern_spans.insert(id, self.span_from_range(range));
        id
    }

    fn finish(self, root_expr: Option<ExprId>) -> ExprBody {
        ExprBody {
            exprs: self.exprs,
            stmts: self.stmts,
            patterns: self.patterns,
            root_expr,
            expr_spans: self.expr_spans,
            stmt_spans: self.stmt_spans,
            pattern_spans: self.pattern_spans,
            match_arm_spans: self.match_arm_spans,
        }
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

    fn lower_block_expr(&mut self, block: &baml_syntax::ast::BlockExpr) -> ExprId {
        use baml_syntax::{SyntaxKind, ast::BlockElement};

        let mut stmts = Vec::new();
        let mut tail_expr = None;

        let elements: Vec<_> = block.elements().collect();

        for (idx, element) in elements.iter().enumerate() {
            let is_last = idx == elements.len() - 1;
            match element {
                BlockElement::Stmt(node) => {
                    let stmt_id = match node.kind() {
                        SyntaxKind::LET_STMT => self.lower_let_stmt(node),
                        SyntaxKind::RETURN_STMT => self.lower_return_stmt(node),
                        SyntaxKind::WHILE_STMT => self.lower_while_stmt(node),
                        SyntaxKind::FOR_EXPR => self.lower_for_stmt(node),
                        SyntaxKind::BREAK_STMT => self.alloc_stmt(Stmt::Break, node.text_range()),
                        SyntaxKind::CONTINUE_STMT => {
                            self.alloc_stmt(Stmt::Continue, node.text_range())
                        }
                        _ => self.alloc_stmt(Stmt::Missing, node.text_range()),
                    };
                    stmts.push(stmt_id);
                }
                BlockElement::ExprNode(node) => {
                    // First, try to lower as an assignment statement
                    if let Some(stmt_id) = self.try_lower_assignment(node) {
                        stmts.push(stmt_id);
                        continue;
                    }

                    // Not an assignment - lower as regular expression
                    let expr_id = self.lower_expr(node);

                    // Check if this expression is followed by a semicolon
                    let has_semicolon = element.has_trailing_semicolon();

                    // Last expression without semicolon becomes tail expression
                    if is_last && !has_semicolon {
                        tail_expr = Some(expr_id);
                    } else {
                        // Expression statement (with semicolon or not last)
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
                    // Last element without semicolon becomes tail expression
                    // TODO: in the case of optional semicolons in the future,
                    // simply knowing whether an expr has a trailing semicolon will not be enough
                    if is_last && !element.has_trailing_semicolon() {
                        tail_expr = Some(expr_id);
                    } else {
                        stmts.push(self.alloc_stmt(Stmt::Expr(expr_id), span));
                    }
                }
            }
        }

        self.alloc_expr(
            Expr::Block { stmts, tail_expr },
            block.syntax().text_range(),
        )
    }

    fn lower_expr(&mut self, node: &baml_syntax::SyntaxNode) -> ExprId {
        use baml_syntax::SyntaxKind;

        match node.kind() {
            SyntaxKind::BINARY_EXPR => self.lower_binary_expr(node),
            SyntaxKind::UNARY_EXPR => self.lower_unary_expr(node),
            SyntaxKind::CALL_EXPR => self.lower_call_expr(node),
            SyntaxKind::IF_EXPR => self.lower_if_expr(node),
            SyntaxKind::MATCH_EXPR => self.lower_match_expr(node),
            SyntaxKind::BLOCK_EXPR => {
                if let Some(block) = baml_syntax::ast::BlockExpr::cast(node.clone()) {
                    self.lower_block_expr(&block)
                } else {
                    self.alloc_expr(Expr::Missing, node.text_range())
                }
            }
            SyntaxKind::PATH_EXPR => self.lower_path_expr(node),
            SyntaxKind::FIELD_ACCESS_EXPR => self.lower_field_access_expr(node),
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

    fn lower_binary_expr(&mut self, node: &baml_syntax::SyntaxNode) -> ExprId {
        use baml_syntax::SyntaxKind;

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
    fn try_lower_assignment(&mut self, node: &baml_syntax::SyntaxNode) -> Option<StmtId> {
        use baml_syntax::SyntaxKind;

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

    fn lower_unary_expr(&mut self, node: &baml_syntax::SyntaxNode) -> ExprId {
        use baml_syntax::SyntaxKind;

        // Unary expressions can have: child nodes (other exprs) OR direct tokens (literals/identifiers)
        // We need to handle both cases, similar to lower_binary_expr.
        let mut op = None;
        let mut operand = None;

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

        let op = op.unwrap_or(UnaryOp::Not);
        let expr = operand.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        self.alloc_expr(Expr::Unary { op, expr }, node.text_range())
    }

    fn lower_if_expr(&mut self, node: &baml_syntax::SyntaxNode) -> ExprId {
        // IF_EXPR structure: condition (EXPR), then_branch (BLOCK_EXPR), optional else_branch
        let children: Vec<_> = node.children().collect();

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
    fn lower_match_expr(&mut self, node: &baml_syntax::SyntaxNode) -> ExprId {
        use baml_syntax::SyntaxKind;

        let match_span = self.span_from_node(node);
        let mut scrutinee = None;
        let mut arms = Vec::new();
        let mut arm_spans = Vec::new();

        // Use children_with_tokens to handle both node and token children
        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => {
                    match child.kind() {
                        SyntaxKind::MATCH_ARM => {
                            let (arm, spans) = self.lower_match_arm(&child);
                            arms.push(arm);
                            arm_spans.push(spans);
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
                                scrutinee =
                                    Some(self.exprs.alloc(Expr::Literal(Literal::Int(value))));
                            }
                            SyntaxKind::FLOAT_LITERAL => {
                                let text = token.text().to_string();
                                scrutinee =
                                    Some(self.exprs.alloc(Expr::Literal(Literal::Float(text))));
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
                                scrutinee =
                                    Some(self.exprs.alloc(Expr::Literal(Literal::String(content))));
                            }
                            SyntaxKind::WORD => {
                                let text = token.text();
                                let expr = match text {
                                    "true" => self.exprs.alloc(Expr::Literal(Literal::Bool(true))),
                                    "false" => {
                                        self.exprs.alloc(Expr::Literal(Literal::Bool(false)))
                                    }
                                    "null" => self.exprs.alloc(Expr::Literal(Literal::Null)),
                                    _ => self.exprs.alloc(Expr::Path(vec![Name::new(text)])),
                                };
                                scrutinee = Some(expr);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        let scrutinee = scrutinee.unwrap_or_else(|| self.exprs.alloc(Expr::Missing));

        let expr_id = self.exprs.alloc(Expr::Match { scrutinee, arms });

        // Store span information for this match expression
        self.expr_spans.insert(expr_id, match_span);
        self.match_arm_spans.insert(expr_id, arm_spans);

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
    fn lower_match_arm(&mut self, node: &baml_syntax::SyntaxNode) -> (MatchArm, MatchArmSpans) {
        use baml_syntax::SyntaxKind;

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
                            guard = child.children().next().map(|n| self.lower_expr(&n));
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
    fn lower_match_pattern(&mut self, node: &baml_syntax::SyntaxNode) -> PatId {
        use baml_syntax::SyntaxKind;

        // Collect pattern elements separated by PIPE
        let mut elements: Vec<PatId> = Vec::new();
        let mut current_element: Option<PatternElement> = None;

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
                            if let Some(PatternElement::EnumStart(enum_name)) =
                                current_element.take()
                            {
                                // Complete the enum variant: EnumName.Variant
                                let variant = Name::new(&text);
                                elements.push(
                                    self.patterns
                                        .alloc(Pattern::EnumVariant { enum_name, variant }),
                                );
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
                                    current_element = Some(PatternElement::Ident(Name::new(&text)));
                                }
                            }
                        }
                        SyntaxKind::DOT => {
                            // Transition: Ident.Variant (enum variant pattern)
                            if let Some(PatternElement::Ident(enum_name)) = current_element.take() {
                                current_element = Some(PatternElement::EnumStart(enum_name));
                            }
                        }
                        SyntaxKind::COLON => {
                            // Transition: ident: Type (typed binding pattern)
                            if let Some(PatternElement::Ident(name)) = current_element.take() {
                                current_element = Some(PatternElement::TypedBindingStart(name));
                            }
                        }
                        SyntaxKind::INTEGER_LITERAL => {
                            if let Some(el) = current_element.take() {
                                elements.push(self.finalize_pattern_element(el));
                            }
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            elements
                                .push(self.patterns.alloc(Pattern::Literal(Literal::Int(value))));
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            if let Some(el) = current_element.take() {
                                elements.push(self.finalize_pattern_element(el));
                            }
                            let text = token.text().to_string();
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
                        SyntaxKind::TYPE_EXPR => {
                            // Complete typed binding: ident: Type
                            if let Some(PatternElement::TypedBindingStart(name)) =
                                current_element.take()
                            {
                                if let Some(type_expr) =
                                    baml_syntax::ast::TypeExpr::cast(child_node)
                                {
                                    let ty = crate::type_ref::TypeRef::from_ast(&type_expr);
                                    elements.push(
                                        self.patterns.alloc(Pattern::TypedBinding { name, ty }),
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
            PatternElement::Ident(name) => self.patterns.alloc(Pattern::Binding(name)),
            PatternElement::EnumStart(enum_name) => {
                // Incomplete enum variant (missing variant name) - treat as binding
                self.patterns.alloc(Pattern::Binding(enum_name))
            }
            PatternElement::TypedBindingStart(name) => {
                // Incomplete typed binding (missing type) - treat as simple binding
                self.patterns.alloc(Pattern::Binding(name))
            }
        }
    }

    fn lower_call_expr(&mut self, node: &baml_syntax::SyntaxNode) -> ExprId {
        use baml_syntax::SyntaxKind;

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
            self.lower_expr(&n)
        } else {
            // No callee node - check for a WORD token (simple function name)
            let word_token = node
                .children_with_tokens()
                .filter_map(baml_syntax::NodeOrToken::into_token)
                .find(|t| t.kind() == SyntaxKind::WORD);

            if let Some(token) = word_token {
                let name = token.text().to_string();
                self.alloc_expr(Expr::Path(vec![Name::new(&name)]), token.text_range())
            } else {
                self.alloc_expr(Expr::Missing, node.text_range())
            }
        };

        // Find CALL_ARGS node and extract arguments
        let args =
            node.children()
                .find(|n| n.kind() == SyntaxKind::CALL_ARGS)
                .map(|args_node| {
                    let mut args = Vec::new();

                    // First, collect expression nodes
                    for child in args_node.children() {
                        if matches!(
                            child.kind(),
                            SyntaxKind::EXPR
                                | SyntaxKind::BINARY_EXPR
                                | SyntaxKind::UNARY_EXPR
                                | SyntaxKind::CALL_EXPR
                                | SyntaxKind::PATH_EXPR
                                | SyntaxKind::FIELD_ACCESS_EXPR
                                | SyntaxKind::INDEX_EXPR
                                | SyntaxKind::IF_EXPR
                                | SyntaxKind::BLOCK_EXPR
                                | SyntaxKind::PAREN_EXPR
                                | SyntaxKind::ARRAY_LITERAL
                                | SyntaxKind::STRING_LITERAL
                                | SyntaxKind::OBJECT_LITERAL
                                | SyntaxKind::MAP_LITERAL
                        ) {
                            args.push(self.lower_expr(&child));
                        }
                    }

                    // If no expression nodes found, check for literal tokens
                    // (parser may emit literals as tokens directly in CALL_ARGS)
                    if args.is_empty() {
                        for element in args_node.children_with_tokens() {
                            match element {
                                baml_syntax::NodeOrToken::Token(token) => {
                                    let span = token.text_range();
                                    let expr = match token.kind() {
                                        SyntaxKind::INTEGER_LITERAL => {
                                            let text = token.text();
                                            let value = text.parse::<i64>().unwrap_or(0);
                                            Some(self.alloc_expr(
                                                Expr::Literal(Literal::Int(value)),
                                                span,
                                            ))
                                        }
                                        SyntaxKind::FLOAT_LITERAL => {
                                            let text = token.text().to_string();
                                            Some(self.alloc_expr(
                                                Expr::Literal(Literal::Float(text)),
                                                span,
                                            ))
                                        }
                                        SyntaxKind::STRING_LITERAL
                                        | SyntaxKind::RAW_STRING_LITERAL => {
                                            let text = token.text().to_string();
                                            // Strip quotes
                                            let content = if text.starts_with("#\"")
                                                && text.ends_with("\"#")
                                            {
                                                text[2..text.len() - 2].to_string()
                                            } else if text.starts_with('"') && text.ends_with('"') {
                                                text[1..text.len() - 1].to_string()
                                            } else {
                                                text
                                            };
                                            Some(self.alloc_expr(
                                                Expr::Literal(Literal::String(content)),
                                                span,
                                            ))
                                        }
                                        SyntaxKind::WORD => {
                                            // Variable reference or keyword (true/false/null)
                                            let text = token.text();
                                            match text {
                                                "true" => Some(self.alloc_expr(
                                                    Expr::Literal(Literal::Bool(true)),
                                                    span,
                                                )),
                                                "false" => Some(self.alloc_expr(
                                                    Expr::Literal(Literal::Bool(false)),
                                                    span,
                                                )),
                                                "null" => Some(self.alloc_expr(
                                                    Expr::Literal(Literal::Null),
                                                    span,
                                                )),
                                                _ => Some(self.alloc_expr(
                                                    Expr::Path(vec![Name::new(text)]),
                                                    span,
                                                )),
                                            }
                                        }
                                        _ => None,
                                    };
                                    if let Some(e) = expr {
                                        args.push(e);
                                    }
                                }
                                baml_syntax::NodeOrToken::Node(child_node) => {
                                    // Also handle expression nodes in this pass
                                    if matches!(
                                        child_node.kind(),
                                        SyntaxKind::EXPR
                                            | SyntaxKind::BINARY_EXPR
                                            | SyntaxKind::UNARY_EXPR
                                            | SyntaxKind::CALL_EXPR
                                            | SyntaxKind::PATH_EXPR
                                            | SyntaxKind::FIELD_ACCESS_EXPR
                                            | SyntaxKind::INDEX_EXPR
                                            | SyntaxKind::IF_EXPR
                                            | SyntaxKind::BLOCK_EXPR
                                            | SyntaxKind::PAREN_EXPR
                                            | SyntaxKind::ARRAY_LITERAL
                                    ) {
                                        args.push(self.lower_expr(&child_node));
                                    }
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
    fn lower_field_access_expr(&mut self, node: &baml_syntax::SyntaxNode) -> ExprId {
        use baml_syntax::ast::FieldAccessExpr;
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

    fn lower_index_expr(&mut self, node: &baml_syntax::SyntaxNode) -> ExprId {
        use baml_syntax::SyntaxKind;

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

    fn lower_path_expr(&mut self, node: &baml_syntax::SyntaxNode) -> ExprId {
        use baml_syntax::ast::PathExpr;
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

    fn lower_string_literal(&mut self, node: &baml_syntax::SyntaxNode) -> ExprId {
        use baml_syntax::SyntaxKind;

        // Find the actual STRING_LITERAL or RAW_STRING_LITERAL token inside the node.
        // This avoids including trivia/whitespace that might be part of the node's text span.
        let text = node
            .children_with_tokens()
            .filter_map(baml_syntax::NodeOrToken::into_token)
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

        // Strip quotes
        let content = if text.starts_with("#\"") && text.ends_with("\"#") {
            &text[2..text.len() - 2]
        } else if text.starts_with('"') && text.ends_with('"') {
            &text[1..text.len() - 1]
        } else {
            &text
        };

        self.alloc_expr(
            Expr::Literal(Literal::String(content.to_string())),
            node.text_range(),
        )
    }

    fn lower_array_literal(&mut self, node: &baml_syntax::SyntaxNode) -> ExprId {
        use baml_syntax::SyntaxKind;

        // Collect elements from both child nodes and direct tokens
        let mut elements = Vec::new();

        // First, collect expression nodes
        for child in node.children() {
            if !matches!(child.kind(), SyntaxKind::L_BRACKET | SyntaxKind::R_BRACKET) {
                elements.push(self.lower_expr(&child));
            }
        }

        // If no child nodes found, check for direct literal tokens
        if elements.is_empty() {
            for elem in node.children_with_tokens() {
                if let rowan::NodeOrToken::Token(token) = elem {
                    let span = token.text_range();
                    match token.kind() {
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            elements
                                .push(self.alloc_expr(Expr::Literal(Literal::Int(value)), span));
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            elements.push(self.alloc_expr(
                                Expr::Literal(Literal::Float(token.text().to_string())),
                                span,
                            ));
                        }
                        SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                            let text = token.text();
                            let content = if text.starts_with("#\"") && text.ends_with("\"#") {
                                &text[2..text.len() - 2]
                            } else if text.starts_with('"') && text.ends_with('"') {
                                &text[1..text.len() - 1]
                            } else {
                                text
                            };
                            elements.push(self.alloc_expr(
                                Expr::Literal(Literal::String(content.to_string())),
                                span,
                            ));
                        }
                        SyntaxKind::WORD => {
                            let text = token.text();
                            let expr = match text {
                                "true" => self.alloc_expr(Expr::Literal(Literal::Bool(true)), span),
                                "false" => {
                                    self.alloc_expr(Expr::Literal(Literal::Bool(false)), span)
                                }
                                "null" => self.alloc_expr(Expr::Literal(Literal::Null), span),
                                _ => self.alloc_expr(Expr::Path(vec![Name::new(text)]), span),
                            };
                            elements.push(expr);
                        }
                        _ => {}
                    }
                }
            }
        }

        self.alloc_expr(Expr::Array { elements }, node.text_range())
    }

    fn lower_object_literal(&mut self, node: &baml_syntax::SyntaxNode) -> ExprId {
        use baml_syntax::SyntaxKind;

        // Extract type name if present (before the brace)
        let type_name = node
            .children_with_tokens()
            .filter_map(baml_syntax::NodeOrToken::into_token)
            .find(|token| token.kind() == SyntaxKind::WORD)
            .map(|token| Name::new(token.text()));

        // Extract fields from OBJECT_FIELD children
        let fields =
            node.children()
                .filter(|n| n.kind() == SyntaxKind::OBJECT_FIELD)
                .filter_map(|field_node| {
                    let field_span = field_node.text_range();
                    // OBJECT_FIELD has: WORD (field name), COLON, value (EXPR or literal token)
                    let field_name = field_node
                        .children_with_tokens()
                        .filter_map(baml_syntax::NodeOrToken::into_token)
                        .find(|token| token.kind() == SyntaxKind::WORD)
                        .map(|token| Name::new(token.text()))?;

                    // Try to get value as a child node first
                    let value = field_node
                        .children()
                        .next()
                        .map(|n| self.lower_expr(&n))
                        .or_else(|| {
                            // Try to get value as a direct token (literal or identifier)
                            // Skip the field name WORD and look for the value token after COLON
                            let mut seen_colon = false;
                            field_node
                                .children_with_tokens()
                                .filter_map(baml_syntax::NodeOrToken::into_token)
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
                                        SyntaxKind::STRING_LITERAL
                                        | SyntaxKind::RAW_STRING_LITERAL => {
                                            let text = token.text();
                                            let content = if text.starts_with("#\"")
                                                && text.ends_with("\"#")
                                            {
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
                                        SyntaxKind::WORD => {
                                            // Variable reference or boolean/null literal
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

                    Some((field_name, value))
                })
                .collect();

        self.alloc_expr(Expr::Object { type_name, fields }, node.text_range())
    }

    fn lower_map_literal(&mut self, node: &baml_syntax::SyntaxNode) -> ExprId {
        use baml_syntax::SyntaxKind;

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
                                .filter_map(baml_syntax::NodeOrToken::into_token)
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

                    let key_span = self.expr_spans.get(&key).copied();

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
                            )
                        })
                        .map(|n| self.lower_expr(&n))
                        .or_else(|| {
                            // Try to get value as a direct token (literal or identifier)
                            // Skip tokens before the colon
                            let mut seen_colon = false;
                            field_node
                                .children_with_tokens()
                                .filter_map(baml_syntax::NodeOrToken::into_token)
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

    fn try_lower_literal_token(&mut self, node: &baml_syntax::SyntaxNode) -> Option<ExprId> {
        // Check if this node contains a literal token
        node.children_with_tokens()
            .filter_map(baml_syntax::NodeOrToken::into_token)
            .find_map(|token| self.try_lower_token(&token))
    }

    /// Lower a bare token (WORD, `INTEGER_LITERAL`, `FLOAT_LITERAL`) to an expression.
    fn lower_bare_token(&mut self, token: &baml_syntax::SyntaxToken) -> ExprId {
        self.try_lower_token(token)
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, token.text_range()))
    }

    /// Try to lower a token to a literal expression.
    fn try_lower_token(&mut self, token: &baml_syntax::SyntaxToken) -> Option<ExprId> {
        use baml_syntax::SyntaxKind;

        let span = token.text_range();
        match token.kind() {
            SyntaxKind::WORD => {
                // Check if this is a boolean or null literal
                let text = token.text();
                match text {
                    "true" => Some(self.alloc_expr(Expr::Literal(Literal::Bool(true)), span)),
                    "false" => Some(self.alloc_expr(Expr::Literal(Literal::Bool(false)), span)),
                    "null" => Some(self.alloc_expr(Expr::Literal(Literal::Null), span)),
                    _ => None,
                }
            }
            SyntaxKind::INTEGER_LITERAL => {
                let text = token.text();
                let value = text.parse::<i64>().unwrap_or(0);
                Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span))
            }
            SyntaxKind::FLOAT_LITERAL => {
                let text = token.text();
                Some(self.alloc_expr(Expr::Literal(Literal::Float(text.to_string())), span))
            }
            _ => None,
        }
    }

    /// Try to lower token content inside a parenthesized expression.
    ///
    /// This handles the case where `PAREN_EXPR` contains only tokens (no child nodes),
    /// such as `(b)` where `b` is a variable reference, or `(42)` where 42 is a literal.
    fn try_lower_paren_token_content(&mut self, node: &baml_syntax::SyntaxNode) -> Option<ExprId> {
        use baml_syntax::SyntaxKind;

        // Look for tokens inside the parentheses (skip L_PAREN and R_PAREN)
        for elem in node.children_with_tokens() {
            if let Some(token) = elem.into_token() {
                let span = token.text_range();
                match token.kind() {
                    SyntaxKind::WORD => {
                        let text = token.text();
                        // Check if this is a literal (true/false/null)
                        return match text {
                            "true" => {
                                Some(self.alloc_expr(Expr::Literal(Literal::Bool(true)), span))
                            }
                            "false" => {
                                Some(self.alloc_expr(Expr::Literal(Literal::Bool(false)), span))
                            }
                            "null" => Some(self.alloc_expr(Expr::Literal(Literal::Null), span)),
                            // Otherwise it's a variable reference
                            _ => Some(self.alloc_expr(Expr::Path(vec![Name::new(text)]), span)),
                        };
                    }
                    SyntaxKind::INTEGER_LITERAL => {
                        let text = token.text();
                        let value = text.parse::<i64>().unwrap_or(0);
                        return Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span));
                    }
                    SyntaxKind::FLOAT_LITERAL => {
                        let text = token.text();
                        return Some(
                            self.alloc_expr(Expr::Literal(Literal::Float(text.to_string())), span),
                        );
                    }
                    // Skip parentheses and whitespace
                    SyntaxKind::L_PAREN | SyntaxKind::R_PAREN | SyntaxKind::WHITESPACE => {}
                    _ => {}
                }
            }
        }
        None
    }

    fn lower_let_stmt(&mut self, node: &baml_syntax::SyntaxNode) -> StmtId {
        use baml_syntax::SyntaxKind;

        // Use the LetStmt AST wrapper for cleaner access
        let let_stmt = baml_syntax::ast::LetStmt::cast(node.clone());

        // Extract pattern (variable name)
        let pattern = let_stmt
            .as_ref()
            .and_then(baml_syntax::LetStmt::name)
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

        let type_node = let_stmt.as_ref().and_then(baml_syntax::LetStmt::ty);

        // Extract type annotation if present
        let type_annotation = type_node.as_ref().map(TypeRef::from_ast);

        let type_span = type_node.map(|t: TypeExpr| t.syntax().text_range());

        // Extract initializer expression - first try as a node, then as a token
        let initializer = let_stmt
            .as_ref()
            .and_then(baml_syntax::LetStmt::initializer)
            .map(|n| self.lower_expr(&n))
            .or_else(|| {
                // Try to get initializer as a direct token (for simple literals)
                let_stmt
                    .as_ref()
                    .and_then(baml_syntax::LetStmt::initializer_token)
                    .map(|token| {
                        let span = token.text_range();
                        match token.kind() {
                            SyntaxKind::INTEGER_LITERAL => {
                                let value = token.text().parse::<i64>().unwrap_or(0);
                                self.alloc_expr(Expr::Literal(Literal::Int(value)), span)
                            }
                            SyntaxKind::FLOAT_LITERAL => self.alloc_expr(
                                Expr::Literal(Literal::Float(token.text().to_string())),
                                span,
                            ),
                            SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                                let text = token.text();
                                let content = if text.starts_with("#\"") && text.ends_with("\"#") {
                                    &text[2..text.len() - 2]
                                } else if text.starts_with('"') && text.ends_with('"') {
                                    &text[1..text.len() - 1]
                                } else {
                                    text
                                };
                                self.alloc_expr(
                                    Expr::Literal(Literal::String(content.to_string())),
                                    span,
                                )
                            }
                            SyntaxKind::WORD => {
                                // Handle boolean and null literals (parsed as WORD tokens)
                                match token.text() {
                                    "true" => {
                                        self.alloc_expr(Expr::Literal(Literal::Bool(true)), span)
                                    }
                                    "false" => {
                                        self.alloc_expr(Expr::Literal(Literal::Bool(false)), span)
                                    }
                                    "null" => self.alloc_expr(Expr::Literal(Literal::Null), span),
                                    _ => self.alloc_expr(Expr::Missing, span),
                                }
                            }
                            _ => self.alloc_expr(Expr::Missing, span),
                        }
                    })
            });

        self.alloc_stmt(
            Stmt::Let {
                pattern,
                type_annotation,
                type_span,
                initializer,
            },
            node.text_range(),
        )
    }

    fn lower_return_stmt(&mut self, node: &baml_syntax::SyntaxNode) -> StmtId {
        use baml_syntax::SyntaxKind;

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
                .filter_map(baml_syntax::NodeOrToken::into_token)
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

    fn lower_while_stmt(&mut self, node: &baml_syntax::SyntaxNode) -> StmtId {
        // Use the WhileStmt AST wrapper for cleaner access
        let while_stmt = baml_syntax::ast::WhileStmt::cast(node.clone());

        let condition = while_stmt
            .as_ref()
            .and_then(baml_syntax::WhileStmt::condition)
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

    fn lower_for_stmt(&mut self, node: &baml_syntax::SyntaxNode) -> StmtId {
        // Use the ForExpr AST wrapper for cleaner access
        let for_expr = baml_syntax::ast::ForExpr::cast(node.clone());

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
    fn desugar_c_style_for(&mut self, for_expr: &baml_syntax::ast::ForExpr) -> StmtId {
        // 1. Lower the initializer (if present)
        let initializer = for_expr
            .let_stmt()
            .map(|let_stmt| self.lower_let_stmt(let_stmt.syntax()));

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
    fn lower_update_stmt(&mut self, update_node: &baml_syntax::SyntaxNode) -> StmtId {
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
        update_ast: &baml_syntax::SyntaxNode,
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
        update_ast: &baml_syntax::SyntaxNode,
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
    fn desugar_for_in(&mut self, for_expr: &baml_syntax::ast::ForExpr) -> StmtId {
        // Generate unique names for synthetic variables FIRST
        // This ensures outer loops claim _iter, _len, _i before inner loops
        let arr_name = self.gensym("iter");
        let len_name = self.gensym("len");
        let idx_name = self.gensym("i");

        // Now lower the body - inner for-loops will get _iter1, _len1, _i1, etc.
        let user_body = for_expr
            .body()
            .map(|block| self.lower_block_expr(&block))
            .unwrap_or_else(|| self.exprs.alloc(Expr::Missing));

        // 1. let _arr_N = <iterator>
        // First try to get iterator as a child node (for complex expressions like arrays, calls, etc.)
        // If not found, look for a bare WORD token (simple identifier like `xs`)
        let iterator_expr = for_expr
            .iterator()
            .map(|n| self.lower_expr(&n))
            .or_else(|| {
                // Look for a bare WORD token after 'in' keyword
                // The iterator could be a simple identifier that wasn't wrapped in a node
                use baml_syntax::SyntaxKind;
                let mut seen_in = false;
                for element in for_expr.syntax().children_with_tokens() {
                    match element {
                        baml_syntax::NodeOrToken::Token(token) => {
                            if token.kind() == SyntaxKind::KW_IN {
                                seen_in = true;
                            } else if seen_in && token.kind() == SyntaxKind::WORD {
                                // Found the iterator identifier
                                return Some(
                                    self.exprs.alloc(Expr::Path(vec![Name::new(token.text())])),
                                );
                            }
                        }
                        baml_syntax::NodeOrToken::Node(_) => {}
                    }
                }
                None
            })
            .unwrap_or_else(|| self.exprs.alloc(Expr::Missing));

        let arr_pat = self.patterns.alloc(Pattern::Binding(arr_name.clone()));
        let arr_let = self.stmts.alloc(Stmt::Let {
            pattern: arr_pat,
            type_annotation: None,
            type_span: None,
            initializer: Some(iterator_expr),
        });

        // 2. let _len_N = _arr_N.length()
        // This is a method call: FieldAccess followed by Call with no arguments.
        // The typechecker will resolve `length` as a method on arrays.
        let arr_ref = self.exprs.alloc(Expr::Path(vec![arr_name.clone()]));
        let length_method = self.exprs.alloc(Expr::FieldAccess {
            base: arr_ref,
            field: Name::new("length"),
        });
        let length_call = self.exprs.alloc(Expr::Call {
            callee: length_method,
            args: vec![],
        });
        let len_pat = self.patterns.alloc(Pattern::Binding(len_name.clone()));
        let len_let = self.stmts.alloc(Stmt::Let {
            pattern: len_pat,
            type_annotation: None,
            type_span: None,
            initializer: Some(length_call),
        });

        // 3. let _i_N = 0
        let zero = self.exprs.alloc(Expr::Literal(Literal::Int(0)));
        let idx_pat = self.patterns.alloc(Pattern::Binding(idx_name.clone()));
        let idx_let = self.stmts.alloc(Stmt::Let {
            pattern: idx_pat,
            type_annotation: None,
            type_span: None,
            initializer: Some(zero),
        });

        // 4. Condition: _i_N < _len_N
        let idx_ref = self.exprs.alloc(Expr::Path(vec![idx_name.clone()]));
        let len_ref = self.exprs.alloc(Expr::Path(vec![len_name]));
        let condition = self.exprs.alloc(Expr::Binary {
            op: BinaryOp::Lt,
            lhs: idx_ref,
            rhs: len_ref,
        });

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

        let arr_ref2 = self.exprs.alloc(Expr::Path(vec![arr_name]));
        let idx_ref2 = self.exprs.alloc(Expr::Path(vec![idx_name.clone()]));
        let element_access = self.exprs.alloc(Expr::Index {
            base: arr_ref2,
            index: idx_ref2,
        });
        let elem_let = self.stmts.alloc(Stmt::Let {
            pattern: user_pattern,
            type_annotation: None,
            type_span: None,
            initializer: Some(element_access),
        });

        // 6. Increment: _i_N += 1
        let idx_target = self.exprs.alloc(Expr::Path(vec![idx_name]));
        let one = self.exprs.alloc(Expr::Literal(Literal::Int(1)));
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
            self.exprs.alloc(Expr::Block {
                stmts: body_stmts,
                tail_expr: None,
            })
        } else {
            let body_stmt = self.stmts.alloc(Stmt::Expr(user_body));
            self.exprs.alloc(Expr::Block {
                stmts: vec![elem_let, idx_assign, body_stmt],
                tail_expr: None,
            })
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
        let outer_block = self.exprs.alloc(Expr::Block {
            stmts: vec![arr_let, len_let, idx_let, while_stmt],
            tail_expr: None,
        });

        self.stmts.alloc(Stmt::Expr(outer_block))
    }
}
