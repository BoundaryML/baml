use internal_baml_diagnostics::{DatamodelError, Diagnostics};

use super::{
    helpers::{parsing_catch_all, Pair},
    parse_identifier::parse_identifier,
    Rule,
};
use crate::{
    assert_correct_parser,
    ast::{
        self, expr::ExprFn, App, ArgumentsList, AssignOp, AssignOpStmt, AssignStmt, CForLoopStmt,
        Expression, ExpressionBlock, ForLoopStmt, LetStmt, Stmt, TopLevelAssignment, *,
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

    let only_let_stmt = |name, span, diagnostics: &mut Diagnostics| {
        diagnostics.push_error(DatamodelError::new_validation_error(
            &format!("{name} are not allowed at top level, only let statements are allowed"),
            span,
        ));

        None
    };

    match parse_statement(tokens.next()?, diagnostics)? {
        Stmt::Let(stmt) => Some(TopLevelAssignment { stmt }),
        Stmt::Assign(stmt) => only_let_stmt("assignments", stmt.span, diagnostics),
        Stmt::AssignOp(stmt) => only_let_stmt("assignments", stmt.span, diagnostics),
        Stmt::ForLoop(ForLoopStmt { span, .. }) | Stmt::CForLoop(CForLoopStmt { span, .. }) => {
            only_let_stmt("for loops", span, diagnostics)
        }
        Stmt::Expression(expr) => only_let_stmt("expressions", expr.span().clone(), diagnostics),
        Stmt::Semicolon(expr) => {
            only_let_stmt("semicolon expressions", expr.span().clone(), diagnostics)
        }
        Stmt::WhileLoop(stmt) => only_let_stmt("while loops", stmt.span, diagnostics),
        Stmt::Break(span) => only_let_stmt("break statements", span, diagnostics),
        Stmt::Continue(span) => only_let_stmt("continue statements", span, diagnostics),
        Stmt::Return(ReturnStmt { span, .. }) => {
            only_let_stmt("return statements", span, diagnostics)
        }
        Stmt::Assert(AssertStmt { span, .. }) => {
            only_let_stmt("assert statements", span, diagnostics)
        }
    }
}

fn parse_while_loop(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<Stmt> {
    assert_correct_parser!(pair, Rule::while_loop);

    let span = diagnostics.span(pair.as_span());
    let mut while_loop = pair.into_inner();

    let condition_rule =
        check_parentheses_around_rule(&mut while_loop, diagnostics, "while loop condition")?;

    let condition = parse_block_aware_tail_expression(condition_rule, diagnostics)?;

    let body = parse_expr_block(while_loop.next()?, diagnostics)?;

    Some(Stmt::WhileLoop(WhileStmt {
        condition,
        body,
        span,
    }))
}

fn parse_for_loop(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<Stmt> {
    assert_correct_parser!(token, Rule::for_loop);

    let span = diagnostics.span(token.as_span());

    let mut tokens = token.into_inner();

    let in_between_rule =
        check_parentheses_around_rule(&mut tokens, diagnostics, "for loop header")?;

    let body = parse_expr_block(tokens.next()?, diagnostics)?;

    match in_between_rule.as_rule() {
        Rule::c_for_loop => {
            parse_c_for_loop(in_between_rule, body, span, diagnostics).map(Stmt::CForLoop)
        }
        Rule::iterator_for_loop => {
            parse_iterator_for_loop(in_between_rule, body, span, diagnostics).map(Stmt::ForLoop)
        }
        _ => unreachable_rule!(in_between_rule, Rule::for_loop),
    }
}

fn parse_c_for_loop(
    token: Pair<'_>,
    body: ExpressionBlock,
    span: Span,
    diagnostics: &mut Diagnostics,
) -> Option<CForLoopStmt> {
    assert_correct_parser!(token, Rule::c_for_loop);

    let mut header = token.into_inner();

    let init_stmt = consume_if_rule(&mut header, Rule::c_for_init_stmt).map(|rule| {
        rule.into_inner()
            .next()
            .expect("c_for_init_stmt cannot accept empty input")
    });
    let condition = consume_if_rule(&mut header, Rule::expression);
    let after_stmt = consume_if_rule(&mut header, Rule::c_for_after_stmt).map(|rule| {
        rule.into_inner()
            .next()
            .expect("c_for_after_stmt cannot accept empty input")
    });

    let init_stmt = parse_optional_rule(init_stmt, |rule| {
        let span = diagnostics.span(rule.as_span());
        parse_statement_inner_rule(rule, span, diagnostics)
    })?
    .map(Box::new);

    let condition = parse_optional_rule(condition, |rule| parse_expression(rule, diagnostics))?;

    let after_stmt = parse_optional_rule(after_stmt, |rule| {
        let span = diagnostics.span(rule.as_span());

        match rule.as_rule() {
            Rule::block_aware_assign_stmt => {
                let mut tokens = rule.into_inner();

                let left = parse_expression(tokens.next()?, diagnostics)?;

                let expr = parse_block_aware_tail_expression(tokens.next()?, diagnostics)?;

                Some(Stmt::Assign(AssignStmt { left, expr, span }))
            }
            Rule::block_aware_assign_op_stmt => {
                let mut tokens = rule.into_inner();

                let left = tokens.next()?;
                let op = tokens.next()?;
                let expr = parse_block_aware_tail_expression(tokens.next()?, diagnostics);

                finish_assign_op_stmt(span, diagnostics, left, op, expr).map(Stmt::AssignOp)
            }
            _ => parse_statement_inner_rule(rule, span, diagnostics),
        }
    })?
    .map(Box::new);

    Some(CForLoopStmt {
        init_stmt,
        condition,
        after_stmt,
        body,
        span,
    })
}

fn parse_iterator_for_loop(
    token: Pair<'_>,
    body: ExpressionBlock,
    span: Span,
    diagnostics: &mut Diagnostics,
) -> Option<ForLoopStmt> {
    assert_correct_parser!(token, Rule::iterator_for_loop);

    let mut header = token.into_inner();

    let identifier = parse_identifier(header.next()?, diagnostics);
    let iterator = parse_block_aware_tail_expression(header.next()?, diagnostics)?;

    Some(ForLoopStmt {
        identifier,
        iterator,
        body,
        span,
    })
}

fn parse_block_aware_tail_expression(
    pair: Pair<'_>,
    diagnostics: &mut Diagnostics,
) -> Option<Expression> {
    assert_correct_parser!(pair, Rule::block_aware_tail_expression);

    let inner = pair
        .into_inner()
        .next()
        .expect("block aware tail expression is not empty");

    match inner.as_rule() {
        Rule::expression => parse_expression(inner, diagnostics),
        Rule::identifier => Some(Expression::Identifier(parse_identifier(inner, diagnostics))),
        _ => unreachable_rule!(inner, Rule::block_aware_tail_expression),
    }
}

/// Lifts the error from `parse` into the top-level optional. The second level optional will
/// reflect whether there was a rule in the first place.
fn parse_optional_rule<T>(
    rule: Option<Pair<'_>>,
    parse: impl FnOnce(Pair<'_>) -> Option<T>,
) -> Option<Option<T>> {
    rule.map_or(Some(None), |rule| parse(rule).map(Some))
}

fn check_parentheses_around_rule<'src>(
    tokens: &mut pest::iterators::Pairs<'src, Rule>,
    diagnostics: &mut Diagnostics,
    construct_name: &'static str,
) -> Option<pest::iterators::Pair<'src, Rule>> {
    let lparen_span = consume_span_if_rule(tokens, diagnostics, Rule::openParen);

    let in_between_rule = tokens.next()?;

    let rparen_span = consume_span_if_rule(tokens, diagnostics, Rule::closeParen);

    let in_between_span = diagnostics.span(in_between_rule.as_span());

    check_parentheses_around(
        diagnostics,
        construct_name,
        lparen_span,
        rparen_span,
        in_between_span,
    );

    Some(in_between_rule)
}

/// Emits diagnostics depending on what parentheses spans are `None`.
fn check_parentheses_around(
    diagnostics: &mut Diagnostics,
    construct_name: &'static str,
    lparen_span: Option<Span>,
    rparen_span: Option<Span>,
    in_between_span: Span,
) {
    match (lparen_span, rparen_span) {
        (None, None) => diagnostics.push_error(DatamodelError::new_validation_error(
            &format!("expected parentheses around {construct_name}"),
            in_between_span,
        )),
        (None, Some(r)) => diagnostics.push_error(DatamodelError::new_validation_error(
            "expected opening parentheses for this closing parentheses",
            r,
        )),
        (Some(l), None) => diagnostics.push_error(DatamodelError::new_validation_error(
            "expected closing parentheses for this opening parentheses",
            l,
        )),
        // both present. Nothing to check.
        (Some(_), Some(_)) => {}
    }
}

pub fn consume_if_rule<'src>(
    tokens: &mut pest::iterators::Pairs<'src, Rule>,
    rule: Rule,
) -> Option<Pair<'src>> {
    if tokens.peek().is_some_and(|x| x.as_rule() == rule) {
        Some(tokens.next().unwrap())
    } else {
        None
    }
}

pub fn consume_span_if_rule(
    tokens: &mut pest::iterators::Pairs<'_, Rule>,
    diagnostics: &Diagnostics,
    rule: Rule,
) -> Option<Span> {
    consume_if_rule(tokens, rule).map(|rule| diagnostics.span(rule.as_span()))
}

pub fn parse_statement(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<Stmt> {
    assert_correct_parser!(token, Rule::stmt);
    let span = diagnostics.span(token.as_span());
    let mut tokens = token.into_inner();

    let mut stmt = parse_statement_inner_rule(tokens.next()?, span.clone(), diagnostics);

    match tokens.next() {
        Some(maybe_semicolon) if maybe_semicolon.as_str() == ";" => {
            if let Some(Stmt::Expression(expr)) = stmt {
                stmt = Some(Stmt::Semicolon(expr));
            }
        }
        _ => {
            if matches!(
                stmt,
                Some(Stmt::Let(_) | Stmt::Assign(_) | Stmt::AssignOp(_))
            ) {
                diagnostics.push_error(DatamodelError::new_static(
                    "Statement must end with a semicolon.",
                    span,
                ));
            }
        }
    }

    stmt
}

fn parse_statement_inner_rule(
    stmt_token: Pair<'_>,
    span: Span,
    diagnostics: &mut Diagnostics,
) -> Option<Stmt> {
    match stmt_token.as_rule() {
        Rule::assert_stmt => {
            let assert_value = stmt_token.into_inner().next()?;
            let value = parse_expression(assert_value, diagnostics)?;

            Some(Stmt::Assert(AssertStmt { value, span }))
        }

        Rule::return_stmt => {
            let return_value = stmt_token.into_inner().next()?;
            let value = parse_expression(return_value, diagnostics)?;

            Some(Stmt::Return(ReturnStmt { value, span }))
        }
        Rule::assign_stmt => {
            let mut assignment_tokens = stmt_token.into_inner();

            let lhs = parse_expression(assignment_tokens.next()?, diagnostics)?;

            let rhs = assignment_tokens.next()?;
            let rhs_span = diagnostics.span(rhs.as_span());
            let maybe_body = parse_assignment_expr(diagnostics, rhs, rhs_span);
            maybe_body.map(|body| {
                Stmt::Assign(AssignStmt {
                    left: lhs,
                    expr: body,
                    span,
                })
            })
        }
        Rule::assign_op_stmt => {
            let mut assignment_tokens = stmt_token.into_inner();

            let lhs = assignment_tokens.next()?;
            let op_token = assignment_tokens.next()?;
            let rhs = assignment_tokens.next()?;

            let rhs_span = diagnostics.span(rhs.as_span());
            let maybe_body = parse_assignment_expr(diagnostics, rhs, rhs_span);

            finish_assign_op_stmt(span, diagnostics, lhs, op_token, maybe_body).map(Stmt::AssignOp)
        }
        Rule::let_expr => {
            let mut let_binding_tokens = stmt_token.into_inner();

            let is_mutable = true; // Always mutable now after mut keyword removal

            let identifier = parse_identifier(let_binding_tokens.next()?, diagnostics);

            let rhs = let_binding_tokens.next()?;
            let rhs_span = diagnostics.span(rhs.as_span());
            let maybe_body = parse_assignment_expr(diagnostics, rhs, rhs_span);
            maybe_body.map(|body| {
                Stmt::Let(LetStmt {
                    identifier,
                    is_mutable,
                    expr: body,
                    span,
                })
            })
        }
        Rule::BREAK_KEYWORD => Some(Stmt::Break(diagnostics.span(stmt_token.as_span()))),
        Rule::CONTINUE_KEYWORD => Some(Stmt::Continue(diagnostics.span(stmt_token.as_span()))),
        Rule::while_loop => parse_while_loop(stmt_token, diagnostics),
        Rule::for_loop => parse_for_loop(stmt_token, diagnostics),
        Rule::if_expression => parse_if_expression(stmt_token, diagnostics).map(Stmt::Expression),
        Rule::expression => parse_expression(stmt_token, diagnostics).map(Stmt::Expression),
        Rule::expr_block => parse_expr_block(stmt_token, diagnostics)
            .map(|expr_block| Stmt::Expression(Expression::ExprBlock(expr_block, span.clone()))),
        _ => {
            diagnostics.push_error(DatamodelError::new_static("Expected statement", span));
            None
        }
    }
}

/// Given identifier & operator rules, allows different parse strategies for the
/// rvalue expression.
fn finish_assign_op_stmt(
    span: Span,
    diagnostics: &mut Diagnostics,
    lhs_rule: Pair<'_>,
    op_token: Pair<'_>,
    maybe_body: Option<Expression>,
) -> Option<AssignOpStmt> {
    let left = parse_expression(lhs_rule, diagnostics)?;

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

    maybe_body.map(|body| AssignOpStmt {
        left,
        assign_op,
        expr: body,
        span,
    })
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

    // Recursively decide if the trailing expression should be a statement or
    // really is a trailing expression that produces a value.
    let is_return_value = expr.as_ref().is_some_and(|expr| match expr {
        // Base case. Expression that produce some kind of value.
        Expression::BoolValue(..)
        | Expression::StringValue(..)
        | Expression::RawStringValue(..)
        | Expression::NumericValue(..)
        | Expression::JinjaExpressionValue(..)
        | Expression::Identifier(..)
        | Expression::App(..)
        | Expression::MethodCall { .. }
        | Expression::ArrayAccess(..)
        | Expression::FieldAccess(..)
        | Expression::Array(..)
        | Expression::Map(..)
        | Expression::ClassConstructor(..)
        | Expression::BinaryOperation { .. }
        | Expression::UnaryOperation { .. }
        | Expression::Paren(..) => true,

        // If the trailing expression happens to be a block, check if the
        // block itself has a trailing expression that produces a value.
        Expression::ExprBlock(block, _) => block.expr.is_some(),

        // If trailing expression is an if statement, check if the statment
        // itself has a trailing expression.
        Expression::If(_, if_branch, else_branch, _) => match if_branch.as_ref() {
            Expression::ExprBlock(block, _) => block.expr.is_some(),
            _ => match else_branch.as_ref().map(Box::as_ref) {
                Some(Expression::ExprBlock(block, _)) => block.expr.is_some(),
                // This should not happen since branches are always blocks.
                _ => true,
            },
        },

        // TODO: Is this possible?
        Expression::Lambda(..) => todo!("exprs that evaluate to lambda"),
    });

    // If the block actually returns a value, keep it as trailing expression.
    // Otherwise, promote the expression to a statement.
    let trailing_expr = if is_return_value {
        expr.map(Box::new)
    } else {
        if let Some(expr) = expr {
            stmts.push(Stmt::Expression(expr));
        }

        None
    };

    Some(ExpressionBlock {
        stmts,
        expr: trailing_expr,
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

    let condition_rule =
        check_parentheses_around_rule(&mut tokens, diagnostics, "if expression condition")?;

    let condition = parse_block_aware_tail_expression(condition_rule, diagnostics)?;

    let then_branch_rule = tokens.next()?;
    let then_branch_span = diagnostics.span(then_branch_rule.as_span());
    let then_branch = parse_expr_block(then_branch_rule, diagnostics)?;

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
