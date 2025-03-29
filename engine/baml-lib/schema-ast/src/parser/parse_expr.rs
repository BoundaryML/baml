use super::{
    helpers::{parsing_catch_all, Pair},
    parse_identifier::parse_identifier,
    Rule,
};
use crate::ast::ArgumentsList;
use crate::parser::{
    parse_expression::parse_expression, parse_identifier,
    parse_named_args_list::parse_named_argument_list,
};
use crate::{
    assert_correct_parser,
    ast::{
        expr::{ExprFn, FunctionBody},
        *,
    },
    parser::parse_arguments::parse_arguments_list,
    unreachable_rule,
};
use crate::{
    ast::expr::{self, Expr, ExprWithSpan, Stmt, TopLevelAssignment},
    parser::{parse_field::parse_field_type_chain, parse_types::parse_field_type},
};
use internal_baml_diagnostics::{DatamodelError, Diagnostics};

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
            "fn must have a return type: e.g. fn Foo() -> int",
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

pub fn parse_statement(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<expr::Stmt> {
    dbg!(&token);
    assert_correct_parser!(token, Rule::stmt);
    let span = diagnostics.span(token.as_span());
    let mut tokens = token.into_inner();
    // Our only statements are let bindings, so:
    let let_binding_token = tokens.next()?;
    dbg!(&let_binding_token);
    assert_correct_parser!(let_binding_token, Rule::let_expr);
    let mut let_binding_tokens = let_binding_token.into_inner();
    let identifier = parse_identifier(let_binding_tokens.next()?, diagnostics);

    let rhs = let_binding_tokens.next()?;
    dbg!(&rhs);
    let rhs_span = diagnostics.span(rhs.as_span());
    let maybe_body = match rhs.as_rule() {
        Rule::expr_fn_body => {
            eprintln!("parsing expr_fn_body");
            parse_function_body(rhs, diagnostics)
        }
        Rule::expr => {
            eprintln!("parsing expr");
            let maybe_expr = parse_expr(rhs, diagnostics);
            maybe_expr.map(|expr| FunctionBody {
                stmts: Vec::new(),
                expr,
            })
        }
        _ => {
            diagnostics.push_error(DatamodelError::new_static(
                "Parser only allows expr_fn_body and expr here",
                rhs_span,
            ));
            None
        }
    };
    let maybe_semicolon = tokens.next();
    match maybe_semicolon {
        Some(p) if p.as_str() == ";" => {}
        _ => {
            diagnostics.push_error(DatamodelError::new_static(
                "Statement must end with a semicolon.",
                span.clone(),
            ));
        }
    }
    maybe_body.map(|body| Stmt {
        identifier,
        body,
        span,
    })
}

pub fn parse_expr(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<expr::ExprWithSpan> {
    assert_correct_parser!(token, Rule::expr);
    let span = diagnostics.span(token.as_span());
    let expr_variant = token.into_inner().next()?;
    match expr_variant.as_rule() {
        Rule::expression => {
            let expression = parse_expression(expr_variant, diagnostics);
            expression.map(|e| ExprWithSpan {
                expr: Expr::Atom(e),
                span,
            })
        }
        Rule::fn_app => parse_fn_app(expr_variant, diagnostics),
        Rule::lambda => parse_lambda(expr_variant, diagnostics),
        _ => {
            diagnostics.push_error(DatamodelError::new_static(
                "Internal error: Expected expr node to be either expression, fn_app or lambda.",
                span,
            ));
            None
        }
    }
}

pub fn parse_fn_app(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<expr::ExprWithSpan> {
    assert_correct_parser!(token, Rule::fn_app);
    let span = diagnostics.span(token.as_span());
    let mut tokens = token.into_inner();
    let fn_name = parse_identifier(tokens.next()?, diagnostics);
    let mut args = Vec::new();
    for item in tokens {
        let maybe_arg = parse_expr(item, diagnostics);
        if let Some(arg) = maybe_arg {
            args.push(arg);
        }
    }
    Some(ExprWithSpan {
        expr: Expr::FnApp(fn_name, args),
        span,
    })
}

pub fn parse_lambda(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<expr::ExprWithSpan> {
    assert_correct_parser!(token, Rule::lambda);
    let span = diagnostics.span(token.as_span());
    let mut tokens = token.into_inner();
    let mut args = ArgumentsList {
        arguments: Vec::new(),
    };
    parse_arguments_list(tokens.next()?, &mut args, &None, diagnostics);
    let maybe_body = parse_function_body(tokens.next()?, diagnostics);
    maybe_body.map(|body| ExprWithSpan {
        expr: Expr::Lambda(args, Box::new(body)),
        span,
    })
}

pub fn parse_function_body(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FunctionBody> {
    assert_correct_parser!(token, Rule::expr_fn_body);
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
            Rule::expr => {
                let maybe_expr = parse_expr(item, diagnostics);
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
            _ => {
                diagnostics.push_error(DatamodelError::new_static(
                    "Internal Error: Parser only allows statements and expressions in function body.",
                    span.clone()
                ));
            }
        }
    }
    expr.map(|e| FunctionBody { stmts, expr: e })
}
