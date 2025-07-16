use internal_baml_diagnostics::{DatamodelError, Diagnostics};

use super::{
    helpers::{parsing_catch_all, Pair},
    parse_identifier::parse_identifier,
    Rule,
};
use crate::{
    assert_correct_parser,
    ast::{
        self, expr::ExprFn, App, ArgumentsList, Expression, ExpressionBlock, ForLoopStmt, LetStmt,
        Stmt, TopLevelAssignment, *,
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
    let stmt = parse_statement(tokens.next()?, diagnostics)?;
    Some(TopLevelAssignment { stmt })
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
        Rule::let_expr => {
            let mut let_binding_tokens = stmt_token.into_inner();
            let identifier = parse_identifier(let_binding_tokens.next()?, diagnostics);

            let rhs = let_binding_tokens.next()?;
            let rhs_span = diagnostics.span(rhs.as_span());
            let maybe_body = match rhs.as_rule() {
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
            };
            maybe_body.map(|body| {
                Stmt::Let(LetStmt {
                    identifier,
                    expr: body,
                    span: span.clone(),
                })
            })
        }
        Rule::for_loop => parse_for_loop(stmt_token, diagnostics),
        _ => {
            diagnostics.push_error(DatamodelError::new_static(
                "Expected let expression or for loop",
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
                if expr.is_none() {
                    diagnostics.push_error(DatamodelError::new_static(
                        "Function must end in an expression.",
                        span.clone(),
                    ));
                }
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
    expr.map(|e| ExpressionBlock {
        stmts,
        expr: Box::new(e),
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
    let then_branch = parse_expression(tokens.next()?, diagnostics)?;
    let else_branch = parse_expression(tokens.next()?, diagnostics);
    Some(Expression::If(
        Box::new(condition),
        Box::new(then_branch),
        else_branch.map(Box::new),
        span,
    ))
}
