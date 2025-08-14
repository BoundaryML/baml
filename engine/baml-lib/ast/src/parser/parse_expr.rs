use internal_baml_diagnostics::{DatamodelError, Diagnostics};

use super::{
    helpers::{parsing_catch_all, Pair},
    parse_identifier::parse_identifier,
    Rule,
};
use crate::{
    assert_correct_parser,
    ast::{
        self, expr::ExprFn, App, ArgumentsList, AssignOp, AssignOpStmt, AssignStmt, Expression,
        ExpressionBlock, ForLoopStmt, LetStmt, Stmt, TopLevelAssignment, *,
    },
    parser::{
        parse_arguments::parse_arguments_list, parse_expression::parse_expression,
        parse_field::parse_field_type_chain, parse_identifier,
        parse_named_args_list::parse_named_argument_list, parse_types::parse_field_type,
    },
    unreachable_rule,
};

pub fn parse_expr_fn(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<expr::ExprFn> {
    assert_correct_parser!(token, Rule::expr_fn);
    let span = diagnostics.span(token.as_span());
    let mut tokens = token.into_inner();
    let name = parse_identifier(tokens.next()?, diagnostics);
    let args = parse_named_argument_list(tokens.next()?, diagnostics);
    let arrow_or_body = tokens.next()?;

    // We may or may not have an arrow and a return type.
    // If the args list is immediately followed by an arrow, we have an arrow and a return type.
    // Otherwise, we have just a body.
    let (maybe_return_type, maybe_body) = if matches!(arrow_or_body.as_rule(), Rule::ARROW) {
        let return_type = parse_field_type_chain(tokens.next()?, diagnostics);
        let function_body = parse_function_body(tokens.next()?, diagnostics);
        (Some(return_type), function_body)
    } else {
        diagnostics.push_error(DatamodelError::new_static(
            "function must have a return type: e.g. function Foo() -> int",
            span.clone(),
        ));
        let function_body = parse_function_body(arrow_or_body, diagnostics);
        (None, function_body)
    };
    match (maybe_return_type, maybe_body) {
        (Some(return_type), Some(body)) => Some(ExprFn {
            name,
            args,
            return_type,
            body,
            span,
        }),
        _ => None,
    }
}

pub fn parse_top_level_assignment(
    token: Pair<'_>,
    diagnostics: &mut Diagnostics,
) -> Option<expr::TopLevelAssignment> {
    assert_correct_parser!(token, Rule::top_level_assignment);
    let mut tokens = token.into_inner();

    match parse_statement(tokens.next()?, diagnostics)? {
        Stmt::Let(stmt) => Some(TopLevelAssignment { stmt }),
        Stmt::Assign(stmt) => {
            // NOTE: (Jesus) top-level is generally regarded as order-independent,
            // and assignments need an order of execution.

            diagnostics.push_error(DatamodelError::new_static(
                "assignments are not allowed at top level, only let statements are allowed",
                stmt.span.clone(),
            ));

            None
        }
        Stmt::AssignOp(stmt) => {
            diagnostics.push_error(DatamodelError::new_static(
                "assign operations are not allowed at top level, only let statements are allowed",
                stmt.span.clone(),
            ));

            None
        }

        Stmt::ForLoop(stmt) => {
            diagnostics.push_error(DatamodelError::new_static(
                "for loops are not allowed at top level, only let statements are allowed",
                stmt.span.clone(),
            ));

            None
        }

        Stmt::Expression(expr) => {
            diagnostics.push_error(DatamodelError::new_static(
                "expressions are not allowed at top level, only let statements are allowed",
                expr.span().clone(),
            ));

            None
        }

        Stmt::WhileLoop(stmt) => {
            diagnostics.push_error(DatamodelError::new_static(
                "while loops are not allowed at top level, only let statements are allowed",
                stmt.span.clone(),
            ));

            None
        }
        Stmt::Break(span) => {
            diagnostics.push_error(DatamodelError::new_static(
                "break statements are not allowed at top level, only let statements are allowed",
                span.clone(),
            ));

            None
        }
        Stmt::Continue(span) => {
            diagnostics.push_error(DatamodelError::new_static(
                "continue statements are not allowed at top level, only let statements are allowed",
                span.clone(),
            ));

            None
        }
    }
}

fn parse_while_loop(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<Stmt> {
    assert_correct_parser!(pair, Rule::while_loop);

    let span = diagnostics.span(pair.as_span());
    let mut while_loop = pair.into_inner();

    let condition = parse_expression(while_loop.next()?, diagnostics)?;

    let body = parse_expr_block(while_loop.next()?, diagnostics)?;

    Some(Stmt::WhileLoop(WhileStmt {
        condition,
        body,
        span,
    }))
}

pub fn parse_for_loop(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<Stmt> {
    assert_correct_parser!(token, Rule::for_loop);
    let span = diagnostics.span(token.as_span());
    let mut tokens = token.into_inner();
    let identifier = parse_identifier(tokens.next()?, diagnostics);
    let iterator = parse_expression(tokens.next()?, diagnostics)?;
    let body = parse_expr_block(tokens.next()?, diagnostics)?;
    Some(Stmt::ForLoop(ForLoopStmt {
        identifier,
        iterator,
        body,
        span,
    }))
}

pub fn parse_statement(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<Stmt> {
    assert_correct_parser!(token, Rule::stmt);
    let span = diagnostics.span(token.as_span());
    let mut tokens = token.into_inner();

    let stmt_token = tokens.next()?;
    let stmt = match stmt_token.as_rule() {
        Rule::assign_stmt => {
            let mut assignment_tokens = stmt_token.into_inner();

            let identifier = parse_identifier(assignment_tokens.next()?, diagnostics);

            let rhs = assignment_tokens.next()?;
            let rhs_span = diagnostics.span(rhs.as_span());
            let maybe_body = parse_assignment_expr(diagnostics, rhs, rhs_span);
            maybe_body.map(|body| {
                Stmt::Assign(AssignStmt {
                    identifier,
                    expr: body,
                    span: span.clone(),
                })
            })
        }
        Rule::assign_op_stmt => {
            let mut assignment_tokens = stmt_token.into_inner();

            let identifier = parse_identifier(assignment_tokens.next()?, diagnostics);

            let op_token = assignment_tokens.next()?;

            let assign_op = match op_token.as_rule() {
                Rule::ADD_ASSIGN => AssignOp::AddAssign,
                Rule::SUB_ASSIGN => AssignOp::SubAssign,
                Rule::MUL_ASSIGN => AssignOp::MulAssign,
                Rule::DIV_ASSIGN => AssignOp::DivAssign,
                Rule::MOD_ASSIGN => AssignOp::ModAssign,
                Rule::BIT_AND_ASSIGN => AssignOp::BitAndAssign,
                Rule::BIT_OR_ASSIGN => AssignOp::BitOrAssign,
                Rule::BIT_XOR_ASSIGN => AssignOp::BitXorAssign,
                Rule::BIT_SHL_ASSIGN => AssignOp::ShlAssign,
                Rule::BIT_SHR_ASSIGN => AssignOp::ShrAssign,
                other => unreachable_rule!(op_token, other),
            };

            let rhs = assignment_tokens.next()?;
            let rhs_span = diagnostics.span(rhs.as_span());

            let maybe_body = parse_assignment_expr(diagnostics, rhs, rhs_span);

            maybe_body.map(|body| {
                Stmt::AssignOp(AssignOpStmt {
                    identifier,
                    assign_op,
                    expr: body,
                    span: span.clone(),
                })
            })
        }
        Rule::let_expr => {
            let mut let_binding_tokens = stmt_token.into_inner();

            let is_mutable = if let Rule::MUT_KEYWORD = let_binding_tokens.peek()?.as_rule() {
                let_binding_tokens.next()?;
                true
            } else {
                false
            };

            let identifier = parse_identifier(let_binding_tokens.next()?, diagnostics);

            let rhs = let_binding_tokens.next()?;
            let rhs_span = diagnostics.span(rhs.as_span());
            let maybe_body = parse_assignment_expr(diagnostics, rhs, rhs_span);
            maybe_body.map(|body| {
                Stmt::Let(LetStmt {
                    identifier,
                    is_mutable,
                    expr: body,
                    span: span.clone(),
                })
            })
        }
        Rule::BREAK_KEYWORD => Some(Stmt::Break(diagnostics.span(stmt_token.as_span()))),
        Rule::CONTINUE_KEYWORD => Some(Stmt::Continue(diagnostics.span(stmt_token.as_span()))),
        Rule::while_loop => parse_while_loop(stmt_token, diagnostics),
        Rule::for_loop => parse_for_loop(stmt_token, diagnostics),
        Rule::if_expression => parse_if_expression(stmt_token, diagnostics).map(Stmt::Expression),
        Rule::fn_app => parse_fn_app(stmt_token, diagnostics).map(Stmt::Expression),
        Rule::generic_fn_app => parse_generic_fn_app(stmt_token, diagnostics).map(Stmt::Expression),
        _ => {
            diagnostics.push_error(DatamodelError::new_static(
                "Expected statement",
                span.clone(),
            ));
            None
        }
    };

    let maybe_semicolon = tokens.next();
    match maybe_semicolon {
        Some(p) if p.as_str() == ";" => {}
        _ => {
            if matches!(stmt, Some(Stmt::Let(_))) {
                diagnostics.push_error(DatamodelError::new_static(
                    "Statement must end with a semicolon.",
                    span.clone(),
                ));
            }
        }
    }

    stmt
}

fn parse_assignment_expr(
    diagnostics: &mut Diagnostics,
    rhs: Pair<'_>,
    rhs_span: Span,
) -> Option<Expression> {
    match rhs.as_rule() {
        Rule::expr_block => {
            let block_span = diagnostics.span(rhs.as_span());
            let maybe_expr_block = parse_expr_block(rhs, diagnostics);
            maybe_expr_block.map(|expr_block| Expression::ExprBlock(expr_block, block_span))
        }
        Rule::expression => parse_expression(rhs, diagnostics),
        _ => {
            diagnostics.push_error(DatamodelError::new_static(
                "Parser only allows expr_block and expr here",
                rhs_span,
            ));
            None
        }
    }
}

pub fn parse_expr_block(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<ExpressionBlock> {
    assert_correct_parser!(token, Rule::expr_block);
    let span = diagnostics.span(token.as_span());
    let mut tokens = token.into_inner();
    let mut stmts = Vec::new();
    let mut expr = None;
    let _open_bracket = tokens.next()?;
    for item in tokens {
        match item.as_rule() {
            Rule::stmt => {
                let maybe_stmt = parse_statement(item, diagnostics);
                if let Some(stmt) = maybe_stmt {
                    stmts.push(stmt);
                }
            }
            Rule::expression => {
                let maybe_expr = parse_expression(item, diagnostics);
                if let Some(parsed_expr) = maybe_expr {
                    expr = Some(parsed_expr);
                    break;
                }
            }
            Rule::BLOCK_CLOSE => {
                // Commentend out because we can't have blocks without return
                // expressions otherwise. Plus we need functions with no return
                // types as well.

                // if expr.is_none() {
                //     diagnostics.push_error(DatamodelError::new_static(
                //         "Function must end in an expression.",
                //         span.clone(),
                //     ));
                // }
                break;
            }
            Rule::NEWLINE => {
                continue;
            }
            Rule::comment_block => {
                // Skip comments in function bodies
                continue;
            }
            Rule::empty_lines => {
                // Skip empty lines in function bodies
                continue;
            }
            _ => {
                diagnostics.push_error(DatamodelError::new_static(
                    "Internal Error: Parser only allows statements and expressions in function body.",
                    span.clone()
                ));
            }
        }
    }

    let mut return_expr = expr.map(Box::new);

    // Special case for returning if expressions.
    // TODO: Likely there's no need to separate statements and final expression
    // since a statement can now be an expression. We just need to allow any
    // random expression as a statement as mentioned in the grammar file.
    if return_expr.is_none() && matches!(stmts.last(), Some(Stmt::Expression(Expression::If(..)))) {
        let Some(Stmt::Expression(e)) = stmts.pop() else {
            unreachable!();
        };

        return_expr = Some(Box::new(e));
    }

    Some(ExpressionBlock {
        stmts,
        expr: return_expr,
    })
}

fn parse_fn_args(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Vec<Expression> {
    assert_correct_parser!(token, Rule::fn_args);

    token
        .into_inner()
        .filter_map(|item| parse_expression(item, diagnostics))
        .collect()
}

pub fn parse_fn_app(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<Expression> {
    assert_correct_parser!(token, Rule::fn_app);

    let span = diagnostics.span(token.as_span());
    let mut tokens = token.into_inner();

    let fn_name = parse_identifier(tokens.next()?, diagnostics);

    let args = parse_fn_args(tokens.next()?, diagnostics);

    Some(Expression::App(App {
        name: fn_name,
        type_args: vec![],
        args,
        span,
    }))
}

/// Parse function application with generic type arguments.
///
/// Grammar rules for this one are a little bit more complicated than for
/// normal functions so can't reuse parse_fn_app easily.
pub fn parse_generic_fn_app(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<Expression> {
    assert_correct_parser!(token, Rule::generic_fn_app);

    let span = diagnostics.span(token.as_span());
    let mut tokens = token.into_inner();

    // Grab name from generic_fn_app_identifier rule.
    let fn_name = parse_identifier(tokens.next()?.into_inner().next()?, diagnostics);

    // Move into generic_fn_app_args rule.
    tokens = tokens.next()?.into_inner();

    // Parse type argument. Only one for now.
    let type_arg = parse_field_type_chain(tokens.next()?, diagnostics)?;

    // Parse arguments.
    let args = parse_fn_args(tokens.next()?, diagnostics);

    Some(Expression::App(App {
        name: fn_name,
        type_args: vec![type_arg],
        args,
        span,
    }))
}

pub fn parse_lambda(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<Expression> {
    assert_correct_parser!(token, Rule::lambda);
    let span = diagnostics.span(token.as_span());
    let mut tokens = token.into_inner();
    let mut args = ArgumentsList {
        arguments: Vec::new(),
    };
    parse_arguments_list(tokens.next()?, &mut args, &None, diagnostics);
    let maybe_body = parse_function_body(tokens.next()?, diagnostics);
    maybe_body.map(|body| Expression::Lambda(args, Box::new(body), span))
}

pub fn parse_function_body(
    token: Pair<'_>,
    diagnostics: &mut Diagnostics,
) -> Option<ExpressionBlock> {
    parse_expr_block(token, diagnostics)
}

pub fn parse_if_expression(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<Expression> {
    assert_correct_parser!(token, Rule::if_expression);
    let span = diagnostics.span(token.as_span());
    let mut tokens = token.into_inner();
    let condition = parse_expression(tokens.next()?, diagnostics)?;

    // TODO: Some weird parsing going on here, figure out rules and spans.
    let then_branch_expr_block = tokens.next()?;
    let then_branch_span = diagnostics.span(then_branch_expr_block.as_span());
    let then_branch = parse_expr_block(then_branch_expr_block, diagnostics)?;

    let else_branch = tokens.next().and_then(|else_branch_expr| {
        let else_branch_span = diagnostics.span(else_branch_expr.as_span());

        let else_branch = match else_branch_expr.as_rule() {
            Rule::expr_block => parse_expr_block(else_branch_expr, diagnostics)
                .map(|e| Box::new(Expression::ExprBlock(e, else_branch_span))),

            Rule::if_expression => parse_if_expression(else_branch_expr, diagnostics).map(Box::new),

            _ => unreachable_rule!(else_branch_expr, Rule::if_expression),
        };
        else_branch
    });

    Some(Expression::If(
        Box::new(condition),
        Box::new(Expression::ExprBlock(then_branch, then_branch_span)),
        else_branch,
        span,
    ))
}
