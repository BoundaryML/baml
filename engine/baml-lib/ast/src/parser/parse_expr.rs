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

    let condition_rule = while_loop.next()?;

    let (condition, body) = parse_condition_with_block(condition_rule, diagnostics, "while loop")?;

    let body = body.map_or_else(
        || {
            Some(ExpressionBlock {
                stmts: Vec::new(),
                expr: None,
            })
        },
        |rule| parse_expr_block(rule, diagnostics),
    )?;

    Some(Stmt::WhileLoop(WhileStmt {
        condition,
        body,
        span,
    }))
}

/// Parses `condition_with_block` rule, which is a construct specialized to match
/// wrong parentheses, like `if (true { }`. See more in `datamodel.pest`.
/// Emits diagnostics for bad parentheses and the above case.
/// Gives out the parsed condition & the block rule to parse, if available.
/// See [`parse_if_expression`] for details on why returning a block rule and not parsing it directly.
fn parse_condition_with_block<'src>(
    pair: Pair<'src>,
    diagnostics: &mut Diagnostics,
    construct_name: &'static str,
) -> Option<(Expression, Option<Pair<'src>>)> {
    assert_correct_parser!(pair, Rule::condition_and_block);

    let full_span = pair.as_span();

    let mut tokens = pair.into_inner();
    let lparen_span = consume_span_if_rule(&mut tokens, diagnostics, Rule::openParen);

    // we'll interpret condition rule after we know whether we have an expression block or not.
    let condition_rule = tokens.next()?;
    let in_between_span = diagnostics.span(condition_rule.as_span());

    let rparen_span = consume_span_if_rule(&mut tokens, diagnostics, Rule::closeParen);

    let expr_block = tokens.next();

    let condition = match (&expr_block, rparen_span) {
        (None, None) => 'class_ctor_confusion: {
            // no rparen, no expr block -> may have condition
            if let Rule::expression = condition_rule.as_rule() {
                if let Some(condition_rule) = condition_rule
                    .into_inner()
                    .next()
                    .filter(|r| r.as_rule() == Rule::primary_expression)
                {
                    if let Some(condition_rule) = condition_rule
                        .into_inner()
                        .next()
                        .filter(|r| r.as_rule() == Rule::class_constructor)
                    {
                        let mut inner = condition_rule.into_inner();

                        let identifier =
                            inner.next().expect("class_constructor rule has identifier");

                        let in_between_span = diagnostics.span(identifier.as_span());

                        check_parentheses_around(
                            diagnostics,
                            construct_name,
                            lparen_span,
                            None,
                            in_between_span,
                        );

                        // parser took `if (<ident> {}` to be `if (<ident> {})`,
                        // but there is no block afterwards. We'll say that `if (<ident>) {}` is
                        // what the user wanted.
                        // There is no block that we can make use of, since otherwise the parser would have taken `if (<ident> <block>` and we wouldn't be here. We'll discard the inner
                        // contents.

                        let condition =
                            Expression::Identifier(parse_identifier(identifier, diagnostics));

                        break 'class_ctor_confusion condition;
                    }
                }
            }

            // no idea what this can be.
            diagnostics.push_error(DatamodelError::new_validation_error(
                &format!("cannot understand syntax for {construct_name}"),
                diagnostics.span(full_span),
            ));

            return None;
        }
        (None, Some(span)) => {
            // rparen, but no expr block -> fishy!
            check_parentheses_around(
                diagnostics,
                construct_name,
                lparen_span,
                Some(span.clone()),
                in_between_span,
            );

            diagnostics.push_error(DatamodelError::new_validation_error(
                &format!("missing expression block for {construct_name}"),
                span,
            ));

            parse_expression(condition_rule, diagnostics)?
        }
        // TODO: merge this with above ?.
        // there is an expr block, so the condition is safe from `{` ambiguities.
        (Some(block_rule), rparen_span) => {
            check_parentheses_around(
                diagnostics,
                construct_name,
                lparen_span,
                rparen_span,
                in_between_span,
            );
            parse_expression(condition_rule, diagnostics)?
        }
    };

    Some((condition, expr_block))
}

fn parse_iterator_for_loop(
    rule: Pair<'_>,
    span: Span,
    body: ExpressionBlock,
    diagnostics: &mut Diagnostics,
) -> Option<Stmt> {
    assert_correct_parser!(rule, Rule::iterator_for_loop);
    let mut tokens = rule.into_inner();
    let identifier = parse_identifier(tokens.next()?, diagnostics);
    let iterator = parse_expression(tokens.next()?, diagnostics)?;

    Some(Stmt::ForLoop(ForLoopStmt {
        identifier,
        iterator,
        body,
        span,
    }))
}

fn parse_c_for_loop(
    rule: Pair<'_>,
    span: Span,
    body: ExpressionBlock,
    diagnostics: &mut Diagnostics,
) -> Option<Stmt> {
    assert_correct_parser!(rule, Rule::c_for_loop);

    let mut tokens = rule.into_inner();

    let init_stmt = if tokens
        .peek()
        .is_some_and(|p| matches!(p.as_rule(), Rule::c_for_init_stmt))
    {
        let inner = tokens.next().unwrap().into_inner().next()?;

        let span = diagnostics.span(inner.as_span());

        parse_statement_inner_rule(inner, span, diagnostics).map(Box::new)
    } else {
        None
    };

    let condition = if tokens
        .peek()
        .is_some_and(|p| matches!(p.as_rule(), Rule::expression))
    {
        parse_expression(tokens.next().unwrap(), diagnostics)
    } else {
        None
    };

    let after_stmt = if let Some(rule) = tokens.next() {
        assert_correct_parser!(rule, Rule::c_for_after_stmt);
        let inner = rule.into_inner().next()?;
        let span = diagnostics.span(inner.as_span());

        parse_statement_inner_rule(inner, span, diagnostics).map(Box::new)
    } else {
        None
    };

    Some(Stmt::CForLoop(CForLoopStmt {
        init_stmt,
        condition,
        after_stmt,
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
        Rule::iterator_for_loop => {
            parse_iterator_for_loop(in_between_rule, span, body, diagnostics)
        }
        Rule::c_for_loop => parse_c_for_loop(in_between_rule, span, body, diagnostics),
        _ => panic!("unexpected in-between rule in for-loop."),
    }
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

fn consume_span_if_rule(
    tokens: &mut pest::iterators::Pairs<'_, Rule>,
    diagnostics: &mut Diagnostics,
    rule: Rule,
) -> Option<Span> {
    dbg!(&tokens);
    if tokens.peek().is_some_and(|x| x.as_rule() == rule) {
        Some(diagnostics.span(tokens.next().unwrap().as_span()))
    } else {
        None
    }
}

pub fn parse_statement(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<Stmt> {
    assert_correct_parser!(token, Rule::stmt);
    let span = diagnostics.span(token.as_span());
    let mut tokens = token.into_inner();

    let stmt_token = tokens.next()?;
    let stmt = parse_statement_inner_rule(stmt_token, span.clone(), diagnostics);

    let maybe_semicolon = tokens.next();
    match maybe_semicolon {
        Some(p) if p.as_str() == ";" => {}
        _ => {
            if matches!(stmt, Some(Stmt::Let(_))) {
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

            let identifier = parse_identifier(assignment_tokens.next()?, diagnostics);

            let rhs = assignment_tokens.next()?;
            let rhs_span = diagnostics.span(rhs.as_span());
            let maybe_body = parse_assignment_expr(diagnostics, rhs, rhs_span);
            maybe_body.map(|body| {
                Stmt::Assign(AssignStmt {
                    identifier,
                    expr: body,
                    span,
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
                    span,
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
                    span,
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
        Rule::expr_block => parse_expr_block(stmt_token, diagnostics)
            .map(|expr_block| Stmt::Expression(Expression::ExprBlock(expr_block, span.clone()))),
        _ => {
            diagnostics.push_error(DatamodelError::new_static("Expected statement", span));
            None
        }
    }
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

    let condition_and_block = tokens.next()?;

    let cond_and_block_span = condition_and_block.as_span();
    let (condition, then_branch_rule) =
        parse_condition_with_block(condition_and_block, diagnostics, "if expression")?;

    let (then_branch, then_branch_span) = match then_branch_rule {
        Some(rule) => {
            let span = diagnostics.span(rule.as_span());

            let block = parse_expr_block(rule, diagnostics)?;

            (block, span)
        }
        None => {
            (
                ExpressionBlock {
                    stmts: Vec::new(),
                    expr: None,
                },
                // emit a span at the end of `cond_and_block` because there is no block.
                Span {
                    file: diagnostics.span(cond_and_block_span).file,
                    start: cond_and_block_span.end(),
                    end: cond_and_block_span.end(),
                },
            )
        }
    };

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
