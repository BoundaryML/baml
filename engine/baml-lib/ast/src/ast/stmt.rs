use std::fmt;

use super::{Expression, ExpressionBlock, FieldType, Identifier, Span};

#[derive(Debug, Clone)]
pub struct LetStmt {
    pub identifier: Identifier,
    /// Always true after mut keyword removal
    pub is_mutable: bool,
    pub annotation: Option<FieldType>,
    pub expr: Expression,
    pub span: Span,
    pub annotations: Vec<std::sync::Arc<Header>>,
}

#[derive(Debug, Clone)]
pub struct AssignStmt {
    pub left: Expression,
    pub expr: Expression,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct AssignOpStmt {
    pub left: Expression,
    pub assign_op: AssignOp,
    pub expr: Expression,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    /// The `+=` operator (addition)
    AddAssign,
    /// The `-=` operator (subtraction)
    SubAssign,
    /// The `*=` operator (multiplication)
    MulAssign,
    /// The `/=` operator (division)
    DivAssign,
    /// The `%=` operator (modulus)
    ModAssign,
    /// The `^=` operator (bitwise xor)
    BitXorAssign,
    /// The `&=` operator (bitwise and)
    BitAndAssign,
    /// The `|=` operator (bitwise or)
    BitOrAssign,
    /// The `<<=` operator (shift left)
    ShlAssign,
    /// The `>>=` operator (shift right)
    ShrAssign,
}

#[derive(Debug, Clone)]
pub struct ForLoopStmt {
    pub identifier: Identifier,
    pub iterator: Expression,
    pub body: ExpressionBlock,
    pub span: Span,
    // Whether the source had an explicit `let` in the loop header: `for (let x in xs)`
    pub has_let: bool,
    pub annotations: Vec<std::sync::Arc<Header>>,
}

#[derive(Debug, Clone)]
pub struct ExprStmt {
    pub expr: Expression,
    pub annotations: Vec<std::sync::Arc<Header>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct CForLoopStmt {
    pub init_stmt: Option<Box<Stmt>>,
    pub condition: Option<Expression>,
    /// Third statement in `for (;;<after>)` construction.
    pub after_stmt: Option<Box<Stmt>>,
    pub body: ExpressionBlock,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct WhileStmt {
    pub condition: Expression,
    pub body: ExpressionBlock,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ReturnStmt {
    pub value: Expression,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct AssertStmt {
    pub value: Expression,
    pub span: Span,
}

// Stmt(statements) perform actions and not often return values.
#[derive(Debug, Clone)]
pub enum Stmt {
    Let(LetStmt),
    ForLoop(ForLoopStmt),
    CForLoop(CForLoopStmt),
    WhileLoop(WhileStmt),
    /// Expression without a trailing semicolon.
    Expression(ExprStmt),
    /// Expression with a trailing semicolon.
    Semicolon(Expression),
    Assign(AssignStmt),
    AssignOp(AssignOpStmt),
    Break(Span),
    Continue(Span),
    Return(ReturnStmt),
    Assert(AssertStmt),
}

impl fmt::Display for AssignOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AssignOp::AddAssign => "+=",
            AssignOp::SubAssign => "-=",
            AssignOp::MulAssign => "*=",
            AssignOp::DivAssign => "/=",
            AssignOp::ModAssign => "%=",
            AssignOp::BitAndAssign => "&=",
            AssignOp::BitOrAssign => "|=",
            AssignOp::BitXorAssign => "^=",
            AssignOp::ShlAssign => "<<=",
            AssignOp::ShrAssign => ">>=",
        })
    }
}

#[derive(Debug, Clone)]
pub struct Header {
    pub level: u8,
    pub title: String,
    pub span: Span,
}

impl fmt::Display for Stmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stmt::Let(stmt) => {
                if let Some(ann) = &stmt.annotation {
                    write!(f, "let {}: {} = {}", stmt.identifier, ann, stmt.expr)
                } else {
                    write!(f, "let {} = {}", stmt.identifier, stmt.expr)
                }
            }
            Stmt::ForLoop(stmt) => {
                if stmt.has_let {
                    write!(f, "for let {} in {}", stmt.identifier, stmt.iterator)
                } else {
                    write!(f, "for {} in {}", stmt.identifier, stmt.iterator)
                }
            }
            Stmt::CForLoop(stmt) => {
                f.write_str("for (")?;

                if let Some(init) = stmt.init_stmt.as_ref() {
                    write!(f, "{init}")?;
                }

                f.write_str(";")?;

                if let Some(condition) = stmt.condition.as_ref() {
                    write!(f, "{condition}")?;
                }

                f.write_str(";")?;

                if let Some(after) = stmt.after_stmt.as_ref() {
                    write!(f, "{after}")?;
                }

                write!(f, ") {}", stmt.body)
            }
            Stmt::Expression(es) => write!(f, "{}", es.expr),
            Stmt::Semicolon(expr) => write!(f, "{expr};"),
            Stmt::Assign(stmt) => write!(f, "{} = {}", stmt.left, stmt.expr),
            Stmt::AssignOp(stmt) => {
                write!(f, "{} {} {}", stmt.left, stmt.assign_op, stmt.expr)
            }
            Stmt::WhileLoop(stmt) => write!(f, "while {} {}", stmt.condition, stmt.body),
            Stmt::Break(_) => f.write_str("break"),
            Stmt::Continue(_) => f.write_str("continue"),
            Stmt::Return(ReturnStmt { value, .. }) => write!(f, "return {value}"),
            Stmt::Assert(AssertStmt { value, .. }) => write!(f, "assert {value}"),
        }
    }
}

impl Stmt {
    pub fn assert_eq_up_to_span(&self, other: &Stmt) {
        fn assert_opt<T: std::fmt::Debug>(
            a: &Option<T>,
            b: &Option<T>,
            assert_fn: impl FnOnce(&T, &T),
        ) {
            match (a.as_ref(), b.as_ref()) {
                (Some(sa), Some(sb)) => assert_fn(sa, sb),
                (None, None) => {}
                _ => panic!("{a:?} does not equal {b:?} up to span"),
            }
        }

        match (self, other) {
            (Stmt::Let(stmt1), Stmt::Let(stmt2)) => {
                stmt1.identifier.assert_eq_up_to_span(&stmt2.identifier);
                // Compare annotations if both present
                match (&stmt1.annotation, &stmt2.annotation) {
                    (Some(a1), Some(a2)) => a1.assert_eq_up_to_span(a2),
                    (None, None) => {}
                    _ => panic!("Let annotations do not match up to span"),
                }
                stmt1.expr.assert_eq_up_to_span(&stmt2.expr);
            }
            (Stmt::ForLoop(stmt1), Stmt::ForLoop(stmt2)) => {
                stmt1.identifier.assert_eq_up_to_span(&stmt2.identifier);
                stmt1.iterator.assert_eq_up_to_span(&stmt2.iterator);
                stmt1.body.assert_eq_up_to_span(&stmt2.body);
            }
            (Stmt::Expression(es1), Stmt::Expression(es2)) => {
                es1.expr.assert_eq_up_to_span(&es2.expr);
            }
            (Stmt::Semicolon(expr1), Stmt::Semicolon(expr2)) => {
                expr1.assert_eq_up_to_span(expr2);
            }

            (Stmt::Assign(stmt1), Stmt::Assign(stmt2)) => {
                stmt1.left.assert_eq_up_to_span(&stmt2.left);
                stmt1.expr.assert_eq_up_to_span(&stmt2.expr);
            }

            (Stmt::AssignOp(stmt1), Stmt::AssignOp(stmt2)) => {
                assert_eq!(stmt1.assign_op, stmt2.assign_op);
                stmt1.left.assert_eq_up_to_span(&stmt2.left);
                stmt1.expr.assert_eq_up_to_span(&stmt2.expr);
            }

            (
                Stmt::CForLoop(CForLoopStmt {
                    init_stmt: init_stmt1,
                    condition: condition1,
                    after_stmt: after_stmt1,
                    body: body1,
                    ..
                }),
                Stmt::CForLoop(CForLoopStmt {
                    init_stmt,
                    condition,
                    after_stmt,
                    body,
                    ..
                }),
            ) => {
                assert_opt(init_stmt, init_stmt1, |a, b| a.assert_eq_up_to_span(b));
                assert_opt(after_stmt, after_stmt1, |a, b| a.assert_eq_up_to_span(b));
                assert_opt(condition, condition1, |a, b| a.assert_eq_up_to_span(b));

                body.assert_eq_up_to_span(body1);
            }

            (Stmt::WhileLoop(a), Stmt::WhileLoop(b)) => {
                a.condition.assert_eq_up_to_span(&b.condition);
                a.body.assert_eq_up_to_span(&b.body);
            }

            (Stmt::Break(_), Stmt::Break(_)) | (Stmt::Continue(_), Stmt::Continue(_)) => {}

            (
                Stmt::Return(ReturnStmt { value: a, .. }),
                Stmt::Return(ReturnStmt { value: b, .. }),
            )
            | (
                Stmt::Assert(AssertStmt { value: a, .. }),
                Stmt::Assert(AssertStmt { value: b, .. }),
            ) => a.assert_eq_up_to_span(b),

            (
                Stmt::Let(_)
                | Stmt::ForLoop(_)
                | Stmt::Expression(_)
                | Stmt::Semicolon(_)
                | Stmt::Assign(_)
                | Stmt::AssignOp(_)
                | Stmt::CForLoop(_)
                | Stmt::WhileLoop(_)
                | Stmt::Return(_)
                | Stmt::Break(_)
                | Stmt::Continue(_)
                | Stmt::Assert(_),
                _,
            ) => {
                panic!("Types do not match: {self:?} and {other:?}")
            }
        }
    }

    pub fn identifier(&self) -> &Identifier {
        match self {
            Stmt::Let(LetStmt { identifier, .. })
            | Stmt::ForLoop(ForLoopStmt { identifier, .. }) => identifier,

            Stmt::Expression(_) => panic!("expressions don't have identifiers"),
            Stmt::Semicolon(_) => panic!("semicolon expressions don't have identifiers"),
            Stmt::WhileLoop(_) => panic!("while loops don't have identifiers"),
            Stmt::Break(_) => panic!("break statements don't have identifiers"),
            Stmt::Continue(_) => panic!("continue statements don't have identifiers"),
            Stmt::Return(_) => panic!("return statements don't have identifiers"),
            Stmt::Assert(_) => panic!("assert statements don't have identifiers"),
            Stmt::CForLoop(_) => panic!("c-like for loops don't have identifiers"),
            Stmt::Assign(stmt) => match &stmt.left {
                Expression::Identifier(id) => id,
                _ => panic!(
                    "left side of assignment is not an identifier: {:?}",
                    stmt.left
                ),
            },
            Stmt::AssignOp(stmt) => match &stmt.left {
                Expression::Identifier(id) => id,
                _ => panic!(
                    "left side of assignment is not an identifier: {:?}",
                    stmt.left
                ),
            },
        }
    }

    pub fn span(&self) -> &Span {
        match self {
            Stmt::Let(LetStmt { span, .. })
            | Stmt::ForLoop(ForLoopStmt { span, .. })
            | Stmt::CForLoop(CForLoopStmt { span, .. })
            | Stmt::Assign(AssignStmt { span, .. })
            | Stmt::AssignOp(AssignOpStmt { span, .. })
            | Stmt::WhileLoop(WhileStmt { span, .. })
            | Stmt::Return(ReturnStmt { span, .. })
            | Stmt::Break(span)
            | Stmt::Continue(span)
            | Stmt::Assert(AssertStmt { span, .. }) => span,

            Stmt::Expression(es) => &es.span,
            Stmt::Semicolon(expr) => expr.span(),
        }
    }

    // TODO: Get rid of this, just match over the type and grab the body.
    pub fn body(&self) -> &Expression {
        match self {
            Stmt::Let(stmt) => &stmt.expr,
            _ => panic!("body() called on non-let statement"),
        }
    }
}
