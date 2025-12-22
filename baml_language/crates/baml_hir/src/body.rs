//! Function bodies - either LLM prompts or expression IR.
//!
//! The CST already distinguishes `LLM_FUNCTION_BODY` from `EXPR_FUNCTION_BODY`,
//! so we just need to lower each type appropriately.

use std::collections::HashMap;
use std::sync::Arc;

use baml_base::{FileId, Span};
use la_arena::{Arena, Idx};
use rowan::ast::AstNode;

use crate::Name;

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

    /// Spans for patterns
    pub pattern_spans: HashMap<PatId, Span>,

    /// Spans for match arms: maps match expression ID to its arm spans.
    /// Each entry is (arm_span, pattern_span) for each arm in order.
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
        initializer: Option<ExprId>,
    },

    /// While loop: `while (condition) { body }`
    While { condition: ExprId, body: ExprId },

    /// For loop (iterator-style): `for (let i in items) { body }`
    ForIn {
        pattern: PatId,
        iterator: ExprId,
        body: ExprId,
    },

    /// For loop (C-style): `for (let i = 0; i < 10; i += 1) { body }`
    ForCStyle {
        initializer: Option<StmtId>,
        condition: Option<ExprId>,
        update: Option<StmtId>,
        body: ExprId,
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
        // Check which body type we have
        if let Some(llm_body) = func_node.llm_body() {
            Arc::new(FunctionBody::Llm(Self::lower_llm_body(&llm_body)))
        } else if let Some(expr_body) = func_node.expr_body() {
            Arc::new(FunctionBody::Expr(Self::lower_expr_body(
                &expr_body, file_id,
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
    ) -> ExprBody {
        let mut ctx = LoweringContext::new(file_id);

        // The EXPR_FUNCTION_BODY contains a BLOCK_EXPR as its child
        // which contains all the statements and expressions
        let root_expr = expr_body
            .syntax()
            .children()
            .find_map(baml_syntax::ast::BlockExpr::cast)
            .map(|block| ctx.lower_block_expr(&block));

        ExprBody {
            exprs: ctx.exprs,
            stmts: ctx.stmts,
            patterns: ctx.patterns,
            root_expr,
            expr_spans: ctx.expr_spans,
            pattern_spans: ctx.pattern_spans,
            match_arm_spans: ctx.match_arm_spans,
        }
    }
}

struct LoweringContext {
    exprs: Arena<Expr>,
    stmts: Arena<Stmt>,
    patterns: Arena<Pattern>,
    /// File ID for creating spans
    file_id: FileId,
    /// Span tracking for expressions
    expr_spans: HashMap<ExprId, Span>,
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
            expr_spans: HashMap::new(),
            pattern_spans: HashMap::new(),
            match_arm_spans: HashMap::new(),
        }
    }

    /// Create a span from a syntax node's text range.
    fn span_from_node(&self, node: &baml_syntax::SyntaxNode) -> Span {
        Span::new(self.file_id, node.text_range())
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
                        SyntaxKind::BREAK_STMT => self.stmts.alloc(Stmt::Break),
                        SyntaxKind::CONTINUE_STMT => self.stmts.alloc(Stmt::Continue),
                        _ => self.stmts.alloc(Stmt::Missing),
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
                        stmts.push(self.stmts.alloc(Stmt::Expr(expr_id)));
                    }
                }
                BlockElement::ExprToken(token) => {
                    // Handle bare tokens as potential tail expressions
                    let expr_id = match token.kind() {
                        SyntaxKind::WORD => {
                            let text = token.text();
                            match text {
                                "true" => self.exprs.alloc(Expr::Literal(Literal::Bool(true))),
                                "false" => self.exprs.alloc(Expr::Literal(Literal::Bool(false))),
                                "null" => self.exprs.alloc(Expr::Literal(Literal::Null)),
                                _ => self.exprs.alloc(Expr::Path(vec![Name::new(text)])),
                            }
                        }
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            self.exprs.alloc(Expr::Literal(Literal::Int(value)))
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            let text = token.text().to_string();
                            self.exprs.alloc(Expr::Literal(Literal::Float(text)))
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
                            self.exprs.alloc(Expr::Literal(Literal::String(content)))
                        }
                        _ => self.exprs.alloc(Expr::Missing),
                    };

                    // Check if this is a tail expression
                    // Last element without semicolon becomes tail expression
                    if is_last && !element.has_trailing_semicolon() {
                        tail_expr = Some(expr_id);
                    } else {
                        stmts.push(self.stmts.alloc(Stmt::Expr(expr_id)));
                    }
                }
            }
        }

        self.exprs.alloc(Expr::Block { stmts, tail_expr })
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
                    self.exprs.alloc(Expr::Missing)
                }
            }
            SyntaxKind::PATH_EXPR => self.lower_path_expr(node),
            SyntaxKind::FIELD_ACCESS_EXPR => self.lower_field_access_expr(node),
            SyntaxKind::INDEX_EXPR => self.lower_index_expr(node),
            SyntaxKind::PAREN_EXPR => {
                // Unwrap parentheses - just lower the inner expression
                // First try to find a child node
                if let Some(inner) = node.children().next() {
                    self.lower_expr(&inner)
                } else {
                    // No child nodes - try to find a literal token (true/false/null/int)
                    self.try_lower_literal_token(node)
                        .unwrap_or_else(|| self.exprs.alloc(Expr::Missing))
                }
            }
            SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                self.lower_string_literal(node)
            }
            SyntaxKind::ARRAY_LITERAL => self.lower_array_literal(node),
            SyntaxKind::OBJECT_LITERAL => self.lower_object_literal(node),
            _ => {
                // Check if this is a literal token
                if let Some(literal) = self.try_lower_literal_token(node) {
                    literal
                } else {
                    self.exprs.alloc(Expr::Missing)
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
                            let expr_id = self.exprs.alloc(Expr::Literal(Literal::Int(value)));
                            if lhs.is_none() {
                                lhs = Some(expr_id);
                            } else {
                                rhs = Some(expr_id);
                            }
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            let expr_id = self
                                .exprs
                                .alloc(Expr::Literal(Literal::Float(token.text().to_string())));
                            if lhs.is_none() {
                                lhs = Some(expr_id);
                            } else {
                                rhs = Some(expr_id);
                            }
                        }
                        SyntaxKind::WORD => {
                            let text = token.text();
                            let expr_id = match text {
                                "true" => self.exprs.alloc(Expr::Literal(Literal::Bool(true))),
                                "false" => self.exprs.alloc(Expr::Literal(Literal::Bool(false))),
                                "null" => self.exprs.alloc(Expr::Literal(Literal::Null)),
                                _ => self.exprs.alloc(Expr::Path(vec![Name::new(text)])),
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

        let lhs = lhs.unwrap_or_else(|| self.exprs.alloc(Expr::Missing));
        let rhs = rhs.unwrap_or_else(|| self.exprs.alloc(Expr::Missing));
        let op = op.unwrap_or(BinaryOp::Add);

        self.exprs.alloc(Expr::Binary { op, lhs, rhs })
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
                    // Handle literals/identifiers as expressions (skip operators)
                    match token.kind() {
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            let expr_id = self.exprs.alloc(Expr::Literal(Literal::Int(value)));
                            if lhs.is_none() {
                                lhs = Some(expr_id);
                            } else {
                                rhs = Some(expr_id);
                            }
                        }
                        SyntaxKind::WORD => {
                            let text = token.text();
                            let expr_id = match text {
                                "true" => self.exprs.alloc(Expr::Literal(Literal::Bool(true))),
                                "false" => self.exprs.alloc(Expr::Literal(Literal::Bool(false))),
                                "null" => self.exprs.alloc(Expr::Literal(Literal::Null)),
                                _ => self.exprs.alloc(Expr::Path(vec![Name::new(text)])),
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
        let target = lhs.unwrap_or_else(|| self.exprs.alloc(Expr::Missing));
        let value = rhs.unwrap_or_else(|| self.exprs.alloc(Expr::Missing));

        let stmt = match assign_op {
            None => Stmt::Assign { target, value },
            Some(op) => Stmt::AssignOp { target, op, value },
        };

        Some(self.stmts.alloc(stmt))
    }

    fn lower_unary_expr(&mut self, node: &baml_syntax::SyntaxNode) -> ExprId {
        use baml_syntax::SyntaxKind;

        // Find the operator
        let op = node
            .children_with_tokens()
            .filter_map(baml_syntax::NodeOrToken::into_token)
            .find_map(|token| match token.kind() {
                SyntaxKind::NOT => Some(UnaryOp::Not),
                SyntaxKind::MINUS => Some(UnaryOp::Neg),
                _ => None,
            })
            .unwrap_or(UnaryOp::Not); // Default

        // Find the expression
        let expr = node
            .children()
            .next()
            .map(|n| self.lower_expr(&n))
            .unwrap_or_else(|| self.exprs.alloc(Expr::Missing));

        self.exprs.alloc(Expr::Unary { op, expr })
    }

    fn lower_if_expr(&mut self, node: &baml_syntax::SyntaxNode) -> ExprId {
        // IF_EXPR structure: condition (EXPR), then_branch (BLOCK_EXPR), optional else_branch
        let children: Vec<_> = node.children().collect();

        let condition = children
            .first()
            .map(|n| self.lower_expr(n))
            .unwrap_or_else(|| self.exprs.alloc(Expr::Missing));

        let then_branch = children
            .get(1)
            .map(|n| self.lower_expr(n))
            .unwrap_or_else(|| self.exprs.alloc(Expr::Missing));

        // Check for else branch - it might be another IF_EXPR (else if) or BLOCK_EXPR (else)
        let else_branch = if children.len() > 2 {
            Some(self.lower_expr(&children[2]))
        } else {
            None
        };

        self.exprs.alloc(Expr::If {
            condition,
            then_branch,
            else_branch,
        })
    }

    /// Lower a match expression from CST to HIR.
    ///
    /// MATCH_EXPR structure (from parser):
    /// - Scrutinee expression (could be a PAREN_EXPR wrapping the actual expr, or a literal token)
    /// - One or more MATCH_ARM nodes
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
    /// MATCH_ARM structure (from parser):
    /// - MATCH_PATTERN node
    /// - Optional MATCH_GUARD node (contains 'if' keyword + expression)
    /// - FAT_ARROW token ('=>')
    /// - Body expression (BLOCK_EXPR or other expression, or literal token)
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
    /// MATCH_PATTERN structure (from parser):
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
                self.exprs.alloc(Expr::Path(vec![Name::new(&name)]))
            } else {
                self.exprs.alloc(Expr::Missing)
            }
        };

        // Find CALL_ARGS node and extract arguments
        let args = node
            .children()
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
                                let expr = match token.kind() {
                                    SyntaxKind::INTEGER_LITERAL => {
                                        let text = token.text();
                                        let value = text.parse::<i64>().unwrap_or(0);
                                        Some(self.exprs.alloc(Expr::Literal(Literal::Int(value))))
                                    }
                                    SyntaxKind::FLOAT_LITERAL => {
                                        let text = token.text().to_string();
                                        Some(self.exprs.alloc(Expr::Literal(Literal::Float(text))))
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
                                            self.exprs
                                                .alloc(Expr::Literal(Literal::String(content))),
                                        )
                                    }
                                    SyntaxKind::WORD => {
                                        // Variable reference or keyword (true/false/null)
                                        let text = token.text();
                                        match text {
                                            "true" => Some(
                                                self.exprs
                                                    .alloc(Expr::Literal(Literal::Bool(true))),
                                            ),
                                            "false" => Some(
                                                self.exprs
                                                    .alloc(Expr::Literal(Literal::Bool(false))),
                                            ),
                                            "null" => {
                                                Some(self.exprs.alloc(Expr::Literal(Literal::Null)))
                                            }
                                            _ => Some(
                                                self.exprs.alloc(Expr::Path(vec![Name::new(text)])),
                                            ),
                                        }
                                    }
                                    _ => None,
                                };
                                if let Some(e) = expr {
                                    args.push(e);
                                }
                            }
                            baml_syntax::NodeOrToken::Node(node) => {
                                // Also handle expression nodes in this pass
                                if matches!(
                                    node.kind(),
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
                                ) {
                                    args.push(self.lower_expr(&node));
                                }
                            }
                        }
                    }
                }

                args
            })
            .unwrap_or_default();

        self.exprs.alloc(Expr::Call { callee, args })
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
            return self.exprs.alloc(Expr::Missing);
        };

        let base = field_access
            .base()
            .map(|n| self.lower_expr(&n))
            .unwrap_or_else(|| self.exprs.alloc(Expr::Missing));

        let field = field_access
            .field()
            .map(|token| Name::new(token.text()))
            .unwrap_or_else(|| Name::new(""));

        self.exprs.alloc(Expr::FieldAccess { base, field })
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
                            let expr_id = self.exprs.alloc(Expr::Literal(Literal::Int(value)));
                            if !inside_brackets {
                                base = Some(expr_id);
                            } else {
                                index = Some(expr_id);
                            }
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            let expr_id = self
                                .exprs
                                .alloc(Expr::Literal(Literal::Float(token.text().to_string())));
                            if !inside_brackets {
                                base = Some(expr_id);
                            } else {
                                index = Some(expr_id);
                            }
                        }
                        SyntaxKind::WORD => {
                            let text = token.text();
                            let expr_id = match text {
                                "true" => self.exprs.alloc(Expr::Literal(Literal::Bool(true))),
                                "false" => self.exprs.alloc(Expr::Literal(Literal::Bool(false))),
                                "null" => self.exprs.alloc(Expr::Literal(Literal::Null)),
                                _ => self.exprs.alloc(Expr::Path(vec![Name::new(text)])),
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

        let base = base.unwrap_or_else(|| self.exprs.alloc(Expr::Missing));
        let index = index.unwrap_or_else(|| self.exprs.alloc(Expr::Missing));

        self.exprs.alloc(Expr::Index { base, index })
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
            return self.exprs.alloc(Expr::Missing);
        };

        let segments: Vec<Name> = path_expr
            .segments()
            .map(|token| Name::new(token.text()))
            .collect();

        if segments.is_empty() {
            return self.exprs.alloc(Expr::Missing);
        }

        self.exprs.alloc(Expr::Path(segments))
    }

    fn lower_string_literal(&mut self, node: &baml_syntax::SyntaxNode) -> ExprId {
        // node.text() may include surrounding whitespace, so trim it first
        let raw_text = node.text().to_string();
        let text = raw_text.trim();

        // Strip quotes
        let content = if text.starts_with("#\"") && text.ends_with("\"#") {
            &text[2..text.len() - 2]
        } else if text.starts_with('"') && text.ends_with('"') {
            &text[1..text.len() - 1]
        } else {
            text
        };

        self.exprs
            .alloc(Expr::Literal(Literal::String(content.to_string())))
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
                    match token.kind() {
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            elements.push(self.exprs.alloc(Expr::Literal(Literal::Int(value))));
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            elements
                                .push(self.exprs.alloc(Expr::Literal(Literal::Float(
                                    token.text().to_string(),
                                ))));
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
                            elements.push(
                                self.exprs
                                    .alloc(Expr::Literal(Literal::String(content.to_string()))),
                            );
                        }
                        SyntaxKind::WORD => {
                            let text = token.text();
                            let expr = match text {
                                "true" => self.exprs.alloc(Expr::Literal(Literal::Bool(true))),
                                "false" => self.exprs.alloc(Expr::Literal(Literal::Bool(false))),
                                "null" => self.exprs.alloc(Expr::Literal(Literal::Null)),
                                _ => self.exprs.alloc(Expr::Path(vec![Name::new(text)])),
                            };
                            elements.push(expr);
                        }
                        _ => {}
                    }
                }
            }
        }

        self.exprs.alloc(Expr::Array { elements })
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
        let fields = node
            .children()
            .filter(|n| n.kind() == SyntaxKind::OBJECT_FIELD)
            .filter_map(|field_node| {
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
                                match token.kind() {
                                    SyntaxKind::INTEGER_LITERAL => {
                                        let value = token.text().parse::<i64>().unwrap_or(0);
                                        Some(self.exprs.alloc(Expr::Literal(Literal::Int(value))))
                                    }
                                    SyntaxKind::FLOAT_LITERAL => Some(self.exprs.alloc(
                                        Expr::Literal(Literal::Float(token.text().to_string())),
                                    )),
                                    SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                                        let text = token.text();
                                        let content =
                                            if text.starts_with("#\"") && text.ends_with("\"#") {
                                                &text[2..text.len() - 2]
                                            } else if text.starts_with('"') && text.ends_with('"') {
                                                &text[1..text.len() - 1]
                                            } else {
                                                text
                                            };
                                        Some(self.exprs.alloc(Expr::Literal(Literal::String(
                                            content.to_string(),
                                        ))))
                                    }
                                    SyntaxKind::WORD => {
                                        // Variable reference or boolean/null literal
                                        let text = token.text();
                                        let expr = match text {
                                            "true" => {
                                                self.exprs.alloc(Expr::Literal(Literal::Bool(true)))
                                            }
                                            "false" => self
                                                .exprs
                                                .alloc(Expr::Literal(Literal::Bool(false))),
                                            "null" => {
                                                self.exprs.alloc(Expr::Literal(Literal::Null))
                                            }
                                            _ => {
                                                self.exprs.alloc(Expr::Path(vec![Name::new(text)]))
                                            }
                                        };
                                        Some(expr)
                                    }
                                    _ => None,
                                }
                            })
                    })
                    .unwrap_or_else(|| self.exprs.alloc(Expr::Missing));

                Some((field_name, value))
            })
            .collect();

        self.exprs.alloc(Expr::Object { type_name, fields })
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
            .unwrap_or_else(|| self.exprs.alloc(Expr::Missing))
    }

    /// Try to lower a token to a literal expression.
    fn try_lower_token(&mut self, token: &baml_syntax::SyntaxToken) -> Option<ExprId> {
        use baml_syntax::SyntaxKind;

        match token.kind() {
            SyntaxKind::WORD => {
                // Check if this is a boolean or null literal
                let text = token.text();
                match text {
                    "true" => Some(self.exprs.alloc(Expr::Literal(Literal::Bool(true)))),
                    "false" => Some(self.exprs.alloc(Expr::Literal(Literal::Bool(false)))),
                    "null" => Some(self.exprs.alloc(Expr::Literal(Literal::Null))),
                    _ => None,
                }
            }
            SyntaxKind::INTEGER_LITERAL => {
                let text = token.text();
                let value = text.parse::<i64>().unwrap_or(0);
                Some(self.exprs.alloc(Expr::Literal(Literal::Int(value))))
            }
            SyntaxKind::FLOAT_LITERAL => {
                let text = token.text();
                Some(
                    self.exprs
                        .alloc(Expr::Literal(Literal::Float(text.to_string()))),
                )
            }
            _ => None,
        }
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
                let name = Name::new(token.text());
                self.patterns.alloc(Pattern::Binding(name))
            })
            .unwrap_or_else(|| {
                self.patterns
                    .alloc(Pattern::Binding(Name::new("missing_let")))
            });

        // Extract type annotation if present
        let type_annotation = let_stmt
            .as_ref()
            .and_then(baml_syntax::LetStmt::ty)
            .map(|type_expr| crate::type_ref::TypeRef::from_ast(&type_expr));

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
                    .map(|token| match token.kind() {
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            self.exprs.alloc(Expr::Literal(Literal::Int(value)))
                        }
                        SyntaxKind::FLOAT_LITERAL => self
                            .exprs
                            .alloc(Expr::Literal(Literal::Float(token.text().to_string()))),
                        SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                            let text = token.text();
                            let content = if text.starts_with("#\"") && text.ends_with("\"#") {
                                &text[2..text.len() - 2]
                            } else if text.starts_with('"') && text.ends_with('"') {
                                &text[1..text.len() - 1]
                            } else {
                                text
                            };
                            self.exprs
                                .alloc(Expr::Literal(Literal::String(content.to_string())))
                        }
                        _ => self.exprs.alloc(Expr::Missing),
                    })
            });

        self.stmts.alloc(Stmt::Let {
            pattern,
            type_annotation,
            initializer,
        })
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
                .find_map(|token| match token.kind() {
                    SyntaxKind::INTEGER_LITERAL => {
                        let value = token.text().parse::<i64>().unwrap_or(0);
                        Some(self.exprs.alloc(Expr::Literal(Literal::Int(value))))
                    }
                    SyntaxKind::FLOAT_LITERAL => Some(
                        self.exprs
                            .alloc(Expr::Literal(Literal::Float(token.text().to_string()))),
                    ),
                    SyntaxKind::WORD => {
                        let text = token.text();
                        let expr_id = match text {
                            "true" => self.exprs.alloc(Expr::Literal(Literal::Bool(true))),
                            "false" => self.exprs.alloc(Expr::Literal(Literal::Bool(false))),
                            "null" => self.exprs.alloc(Expr::Literal(Literal::Null)),
                            _ => self.exprs.alloc(Expr::Path(vec![Name::new(text)])),
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
                        Some(
                            self.exprs
                                .alloc(Expr::Literal(Literal::String(content.to_string()))),
                        )
                    }
                    _ => None,
                })
        };

        self.stmts.alloc(Stmt::Return(return_value))
    }

    fn lower_while_stmt(&mut self, node: &baml_syntax::SyntaxNode) -> StmtId {
        // Use the WhileStmt AST wrapper for cleaner access
        let while_stmt = baml_syntax::ast::WhileStmt::cast(node.clone());

        let condition = while_stmt
            .as_ref()
            .and_then(baml_syntax::WhileStmt::condition)
            .map(|n| self.lower_expr(&n))
            .unwrap_or_else(|| self.exprs.alloc(Expr::Missing));

        let body = while_stmt
            .and_then(|w| w.body())
            .map(|block| self.lower_block_expr(&block))
            .unwrap_or_else(|| self.exprs.alloc(Expr::Missing));

        self.stmts.alloc(Stmt::While { condition, body })
    }

    fn lower_for_stmt(&mut self, node: &baml_syntax::SyntaxNode) -> StmtId {
        // Use the ForExpr AST wrapper for cleaner access
        let for_expr = baml_syntax::ast::ForExpr::cast(node.clone());

        let Some(for_expr) = for_expr else {
            return self.stmts.alloc(Stmt::Missing);
        };

        // Get the body (common to both styles)
        let body = for_expr
            .body()
            .map(|block| self.lower_block_expr(&block))
            .unwrap_or_else(|| self.exprs.alloc(Expr::Missing));

        if for_expr.is_iterator_style() {
            // Iterator-style: for (let i in items) { ... }
            let pattern = for_expr
                .let_stmt()
                .and_then(|let_stmt| let_stmt.name())
                .map(|name| {
                    let name = crate::Name::new(name.text());
                    self.patterns.alloc(Pattern::Binding(name))
                })
                .or_else(|| {
                    // Fallback to simple loop variable without let
                    for_expr.loop_var().map(|name| {
                        let name = crate::Name::new(name.text());
                        self.patterns.alloc(Pattern::Binding(name))
                    })
                })
                .unwrap_or_else(|| self.patterns.alloc(Pattern::Binding(crate::Name::new("_"))));

            let iterator = for_expr
                .iterator()
                .map(|n| self.lower_expr(&n))
                .unwrap_or_else(|| self.exprs.alloc(Expr::Missing));

            self.stmts.alloc(Stmt::ForIn {
                pattern,
                iterator,
                body,
            })
        } else {
            // C-style: for (let i = 0; i < 10; i += 1) { ... }
            let initializer = for_expr
                .let_stmt()
                .map(|let_stmt| self.lower_let_stmt(let_stmt.syntax()));

            // Get condition as expression node, or fall back to bare token
            let condition = for_expr
                .condition()
                .map(|n| self.lower_expr(&n))
                .or_else(|| {
                    for_expr.condition_token().map(|token| {
                        // Lower bare token to expression
                        self.lower_bare_token(&token)
                    })
                });

            // The update may be an assignment (i += 1) or a plain expression (f()).
            // Try to lower as assignment first, otherwise wrap as Stmt::Expr.
            let update = for_expr.update().map(|n| {
                if let Some(assign_stmt) = self.try_lower_assignment(&n) {
                    assign_stmt
                } else {
                    let expr = self.lower_expr(&n);
                    self.stmts.alloc(Stmt::Expr(expr))
                }
            });

            self.stmts.alloc(Stmt::ForCStyle {
                initializer,
                condition,
                update,
                body,
            })
        }
    }
}
