//! Field vs type attribute disambiguation.
//!
//! After the parser change (all `@` attributes stay inside `TYPE_EXPR`),
//! this module provides:
//! - `is_field_attr()` — classify an attribute name as field-level
//! - `validate_field_attrs()` — post-lowering pass that rejects field attrs
//!   remaining on any `TypeExpr` (they're in invalid positions), including
//!   type annotations inside expression bodies (let, patterns, lambdas)

use crate::ast::{
    ClassDef, Expr, ExprBody, FunctionBodyDef, FunctionDef, Item, LambdaDef, LetDef, TypeAliasDef,
    TypeExpr, TypeExprKind,
};

/// The canonical set of field attribute names.
const FIELD_ATTR_NAMES: &[&str] = &["alias", "description", "skip"];

/// Check if an attribute name is a field attribute.
pub fn is_field_attr(name: &str) -> bool {
    FIELD_ATTR_NAMES.contains(&name)
}

/// Post-lowering validation: report field attrs that appear in nested type
/// positions (inside parens, on union members, inside generics).
/// These were not hoisted during lowering because they weren't at the
/// outermost position.
///
/// Returns (`attr_name`, span) pairs for each invalid field attr.
pub(crate) fn validate_field_attrs(items: &[Item]) -> Vec<(String, text_size::TextRange)> {
    let mut diagnostics = Vec::new();

    for item in items {
        match item {
            Item::Class(class) => validate_class(class, &mut diagnostics),
            Item::Function(func) => validate_function(func, &mut diagnostics),
            Item::TypeAlias(alias) => validate_type_alias(alias, &mut diagnostics),
            Item::Let(let_def) => validate_let_def(let_def, &mut diagnostics),
            _ => {}
        }
    }

    diagnostics
}

fn validate_class(class: &ClassDef, diagnostics: &mut Vec<(String, text_size::TextRange)>) {
    for field in &class.fields {
        // After hoisting, the outermost TypeExpr should have no field attrs left.
        // Any remaining field attrs are in invalid positions.
        validate_type_expr_tree(&field.type_expr, diagnostics);
    }
    // Also validate method signatures
    for method in &class.methods {
        validate_function(method, diagnostics);
    }
}

fn validate_function(func: &FunctionDef, diagnostics: &mut Vec<(String, text_size::TextRange)>) {
    for param in &func.params {
        if let Some(ref spanned) = param.type_expr {
            validate_type_expr_tree(spanned, diagnostics);
        }
    }
    if let Some(ref spanned) = func.return_type {
        validate_type_expr_tree(spanned, diagnostics);
    }
    if let Some(ref spanned) = func.throws {
        validate_type_expr_tree(spanned, diagnostics);
    }
    // Walk expression body for let and pattern type annotations.
    if let Some(FunctionBodyDef::Expr(ref body, _)) = func.body {
        validate_expr_body(body, diagnostics);
    }
}

fn validate_let_def(let_def: &LetDef, diagnostics: &mut Vec<(String, text_size::TextRange)>) {
    if let Some((ref body, _)) = let_def.initializer {
        validate_expr_body(body, diagnostics);
    }
}

/// Walk an expression body, checking all type annotations and typed patterns
/// for field attributes that are invalid in expression-level type positions.
fn validate_expr_body(body: &ExprBody, diagnostics: &mut Vec<(String, text_size::TextRange)>) {
    // Check every type annotation in the arena (used by let bindings, match scrutinees, etc.).
    // In expression bodies, field attrs are NEVER valid on type annotations — there is no
    // hoisting step, so every field attr here is an error.
    for (_, ty) in body.type_annotations.iter() {
        validate_type_expr_tree(ty, diagnostics);
    }

    // Check typed patterns (e.g. `let x: string @alias("n") = ...`). Type
    // information lives in `Pattern::Type` atoms, including those that appear
    // as later links of a `Chain`. The arena iteration covers all of them.
    for (_, pat) in body.patterns.iter() {
        if let crate::ast::Pattern::Type(ty) = pat {
            validate_type_expr_tree(ty, diagnostics);
        }
    }

    // Recurse into lambda bodies — each has its own nested `ExprBody`.
    for (_, expr) in body.exprs.iter() {
        if let Expr::Lambda(lambda) = expr {
            validate_lambda(lambda, diagnostics);
        }
    }
}

fn validate_lambda(lambda: &LambdaDef, diagnostics: &mut Vec<(String, text_size::TextRange)>) {
    for param in &lambda.params {
        if let Some(ref spanned) = param.type_expr {
            validate_type_expr_tree(spanned, diagnostics);
        }
    }
    if let Some(ref spanned) = lambda.return_type {
        validate_type_expr_tree(spanned, diagnostics);
    }
    if let Some(ref spanned) = lambda.throws {
        validate_type_expr_tree(spanned, diagnostics);
    }
    // The body lives in the arena this walk is already covering, so there is
    // nothing further to recurse into.
}

fn validate_type_alias(
    alias: &TypeAliasDef,
    diagnostics: &mut Vec<(String, text_size::TextRange)>,
) {
    if let Some(ref spanned) = alias.type_expr {
        validate_type_expr_tree(spanned, diagnostics);
    }
}

/// Walk the entire `TypeExpr` tree. Any field attr on any node is an error.
/// (The hoisting in lowering already removed valid ones from the outermost node.)
fn validate_type_expr_tree(expr: &TypeExpr, diagnostics: &mut Vec<(String, text_size::TextRange)>) {
    // Check attrs on this node
    for attr in expr.attrs() {
        if is_field_attr(attr.name.as_str()) {
            diagnostics.push((attr.name.to_string(), attr.span));
        }
    }

    // Recurse into children
    match &expr.kind {
        TypeExprKind::Optional { inner, .. } | TypeExprKind::List { inner, .. } => {
            validate_type_expr_tree(inner, diagnostics);
        }
        TypeExprKind::Map { key, value, .. } => {
            validate_type_expr_tree(key, diagnostics);
            validate_type_expr_tree(value, diagnostics);
        }
        TypeExprKind::Union { variants, .. } => {
            for v in variants {
                validate_type_expr_tree(v, diagnostics);
            }
        }
        TypeExprKind::Function {
            params,
            ret,
            throws,
            ..
        } => {
            for p in params {
                validate_type_expr_tree(&p.ty, diagnostics);
            }
            validate_type_expr_tree(ret, diagnostics);
            if let Some(throws) = throws {
                validate_type_expr_tree(throws, diagnostics);
            }
        }
        _ => {}
    }
}
