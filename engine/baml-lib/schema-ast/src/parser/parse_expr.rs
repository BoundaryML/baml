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
    ast::{expr::ExprFn, ExpressionBlock, *},
    parser::parse_arguments::parse_arguments_list,
    unreachable_rule,
};
use crate::{
    ast::{self, Expression, Stmt, TopLevelAssignment},
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

pub fn parse_statement(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<Stmt> {
    assert_correct_parser!(token, Rule::stmt);
    let span = diagnostics.span(token.as_span());
    let mut tokens = token.into_inner();
    // Our only statements are let bindings, so:
    let let_binding_token = tokens.next()?;
    assert_correct_parser!(let_binding_token, Rule::let_expr);
    let mut let_binding_tokens = let_binding_token.into_inner();
    let identifier = parse_identifier(let_binding_tokens.next()?, diagnostics);

    let rhs = let_binding_tokens.next()?;
    let rhs_span = diagnostics.span(rhs.as_span());
    let maybe_body = match rhs.as_rule() {
        Rule::expr_block => {
            let block_span = diagnostics.span(rhs.as_span());
            // eprintln!("parsing expr_block");
            let maybe_expr_block = parse_expr_block(rhs, diagnostics);
            maybe_expr_block.map(|expr_block| Expression::ExprBlock(expr_block, block_span))
        }
        Rule::expression => {
            // eprintln!("parsing expr");
            let maybe_expr = parse_expression(rhs, diagnostics);
            maybe_expr
        }
        _ => {
            diagnostics.push_error(DatamodelError::new_static(
                "Parser only allows expr_block and expr here",
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

pub fn parse_fn_app(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<Expression> {
    assert_correct_parser!(token, Rule::fn_app);
    let span = diagnostics.span(token.as_span());
    let mut tokens = token.into_inner();
    let fn_name = parse_identifier(tokens.next()?, diagnostics);
    let mut args = Vec::new();
    for item in tokens {
        let maybe_arg = parse_expression(item, diagnostics);
        if let Some(arg) = maybe_arg {
            args.push(arg);
        }
    }
    Some(Expression::FnApp(fn_name, args, span))
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
