//! Function bodies - either LLM prompts or expression IR.
//!
//! The CST already distinguishes `LLM_FUNCTION_BODY` from `EXPR_FUNCTION_BODY`,
//! so we just need to lower each type appropriately.

use std::sync::Arc;

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

    /// Variable/path reference (e.g., `x`, `GPT4`)
    Path(Name),

    /// If expression
    If {
        condition: ExprId,
        then_branch: ExprId,
        else_branch: Option<ExprId>,
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

    /// Field access: `user.name`, `obj.field.nested`
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
        update: Option<ExprId>,
        body: ExprId,
    },

    /// Return statement: `return "minor";`
    Return(Option<ExprId>),

    /// Missing/error statement
    Missing,
}

/// The left-hand side of a let binding, or match arm in the future.
///
/// Today only variables can be bound, but in the future we will support
/// more complex patterns: wildcards, literals, paths, and constructors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    /// Binding pattern: `x`, `user`
    Binding(Name),
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
    pub fn lower(func_node: &baml_syntax::ast::FunctionDef) -> Arc<FunctionBody> {
        // Check which body type we have
        if let Some(llm_body) = func_node.llm_body() {
            Arc::new(FunctionBody::Llm(Self::lower_llm_body(&llm_body)))
        } else if let Some(expr_body) = func_node.expr_body() {
            Arc::new(FunctionBody::Expr(Self::lower_expr_body(&expr_body)))
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

    fn lower_expr_body(expr_body: &baml_syntax::ast::ExprFunctionBody) -> ExprBody {
        let mut ctx = LoweringContext::new();

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
        }
    }
}

struct LoweringContext {
    exprs: Arena<Expr>,
    stmts: Arena<Stmt>,
    patterns: Arena<Pattern>,
}

impl LoweringContext {
    fn new() -> Self {
        Self {
            exprs: Arena::new(),
            stmts: Arena::new(),
            patterns: Arena::new(),
        }
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
                        _ => self.stmts.alloc(Stmt::Missing),
                    };
                    stmts.push(stmt_id);
                }
                BlockElement::ExprNode(node) => {
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
                                _ => self.exprs.alloc(Expr::Path(Name::new(text))),
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
                if let Some(inner) = node.children().next() {
                    self.lower_expr(&inner)
                } else {
                    self.exprs.alloc(Expr::Missing)
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
                                _ => self.exprs.alloc(Expr::Path(Name::new(text))),
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
                self.exprs.alloc(Expr::Path(Name::new(&name)))
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
                                            _ => {
                                                Some(self.exprs.alloc(Expr::Path(Name::new(text))))
                                            }
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

    fn lower_field_access_expr(&mut self, node: &baml_syntax::SyntaxNode) -> ExprId {
        use baml_syntax::SyntaxKind;

        // FIELD_ACCESS_EXPR structure: base expression, DOT token, field name (WORD)
        // The parser wraps the left side as a child expression node
        let base = node
            .children()
            .next()
            .map(|n| self.lower_expr(&n))
            .unwrap_or_else(|| self.exprs.alloc(Expr::Missing));

        // Find the field name (WORD token after DOT)
        let field = node
            .children_with_tokens()
            .filter_map(baml_syntax::NodeOrToken::into_token)
            .filter(|token| token.kind() == SyntaxKind::WORD)
            .last() // Get the last WORD (the field name, not part of the base expression)
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
                                _ => self.exprs.alloc(Expr::Path(Name::new(text))),
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
        use baml_syntax::SyntaxKind;

        // PATH_EXPR can be a simple identifier or a qualified path
        // Collect all WORD tokens and join them
        let path_parts: Vec<String> = node
            .children_with_tokens()
            .filter_map(baml_syntax::NodeOrToken::into_token)
            .filter(|token| token.kind() == SyntaxKind::WORD)
            .map(|token| token.text().to_string())
            .collect();

        if path_parts.is_empty() {
            self.exprs.alloc(Expr::Missing)
        } else {
            let path_text = path_parts.join("::");
            self.exprs.alloc(Expr::Path(Name::new(&path_text)))
        }
    }

    fn lower_string_literal(&mut self, node: &baml_syntax::SyntaxNode) -> ExprId {
        let text = node.text().to_string();

        // Strip quotes
        let content = if text.starts_with("#\"") && text.ends_with("\"#") {
            &text[2..text.len() - 2]
        } else if text.starts_with('"') && text.ends_with('"') {
            &text[1..text.len() - 1]
        } else {
            &text
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
                                _ => self.exprs.alloc(Expr::Path(Name::new(text))),
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
        let fields =
            node.children()
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
                            // Try to get value as a direct literal token
                            field_node
                                .children_with_tokens()
                                .filter_map(baml_syntax::NodeOrToken::into_token)
                                .find_map(|token| match token.kind() {
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
                                    _ => None,
                                })
                        })
                        .unwrap_or_else(|| self.exprs.alloc(Expr::Missing));

                    Some((field_name, value))
                })
                .collect();

        self.exprs.alloc(Expr::Object { type_name, fields })
    }

    fn try_lower_literal_token(&mut self, node: &baml_syntax::SyntaxNode) -> Option<ExprId> {
        use baml_syntax::SyntaxKind;

        // Check if this node contains a literal token
        node.children_with_tokens()
            .filter_map(baml_syntax::NodeOrToken::into_token)
            .find_map(|token| match token.kind() {
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
            })
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
                            _ => self.exprs.alloc(Expr::Path(Name::new(text))),
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

            let condition = for_expr.condition().map(|n| self.lower_expr(&n));

            let update = for_expr.update().map(|n| self.lower_expr(&n));

            self.stmts.alloc(Stmt::ForCStyle {
                initializer,
                condition,
                update,
                body,
            })
        }
    }
}
