//! CST `ConfigItem` → `Expr`.
//!
//! Generic lowering of CST config values into AST expressions.
//! Used by retry policy, client, and other config-block syntheses
//! so that each synthesis site only needs to wrap the results in
//! a typed `Expr::Object` rather than hand-parsing each field.

use baml_base::{Literal, Name};
use baml_compiler_syntax::{SyntaxKind, ast as cst};

use crate::ast::{Expr, ExprId};

/// Lower a config item's value to an `Expr`, allocating into the caller's arena.
///
/// Handles: integers, floats, quoted strings, `env.VAR` references,
/// bare words (as strings), nested blocks (as untyped objects),
/// and array literals.
pub(crate) fn lower_config_value(
    item: &cst::ConfigItem,
    alloc: &mut impl FnMut(Expr) -> ExprId,
) -> ExprId {
    // 1. Nested block → Expr::Object (untyped) with recursively lowered children
    if let Some(nested) = item.nested_block() {
        return lower_config_block_to_object(&nested, alloc);
    }

    // 2. Array literal
    if item.is_array() {
        return lower_array_value(item, alloc);
    }

    // 3. Scalar value — look at the raw tokens inside CONFIG_VALUE
    let Some(cv_node) = item.config_value_node() else {
        return alloc(Expr::Null);
    };

    // Check whether *all* meaningful tokens are a single numeric literal.
    // Using `.all()` ensures that mixed-token values like
    // `"anthropic.claude-3-haiku-20240307-v1:0"` (which contains an
    // INTEGER_LITERAL `0` among other tokens) are NOT misclassified as int.
    let is_int = cv_node
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| !matches!(t.kind(), SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE))
        .all(|t| t.kind() == SyntaxKind::INTEGER_LITERAL);

    let is_float = cv_node
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| !matches!(t.kind(), SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE))
        .all(|t| t.kind() == SyntaxKind::FLOAT_LITERAL);

    if is_float {
        if let Some(cv) = item.config_value() {
            if let Some(text) = cv.scalar_text() {
                return alloc(Expr::Literal(Literal::Float(text)));
            }
        }
    }

    if is_int {
        if let Some(v) = item.value_int() {
            return alloc(Expr::Literal(Literal::Int(v)));
        }
    }

    // Fall through to text-based analysis
    let text = item
        .config_value()
        .and_then(|cv| cv.scalar_text())
        .unwrap_or_default();

    // env.VAR_NAME → baml.env.get_or_panic("VAR_NAME")
    if let Some(var_name) = text.strip_prefix("env.") {
        let callee = alloc(Expr::Path(vec![
            Name::new("baml"),
            Name::new("env"),
            Name::new("get_or_panic"),
        ]));
        let arg = alloc(Expr::Literal(Literal::String(var_name.to_string())));
        return alloc(Expr::Call {
            callee,
            args: vec![arg],
        });
    }

    // Bool literals
    if text == "true" {
        return alloc(Expr::Literal(Literal::Bool(true)));
    }
    if text == "false" {
        return alloc(Expr::Literal(Literal::Bool(false)));
    }

    // Quoted string or bare word → String literal
    let cleaned = text.trim_matches('"');
    alloc(Expr::Literal(Literal::String(cleaned.to_string())))
}

/// Lower a nested `ConfigBlock` into an untyped `Expr::Object`.
fn lower_config_block_to_object(
    block: &cst::ConfigBlock,
    alloc: &mut impl FnMut(Expr) -> ExprId,
) -> ExprId {
    let fields: Vec<(Name, ExprId)> = block
        .items()
        .filter_map(|item| {
            let key = item.key()?;
            let value = lower_config_value(&item, alloc);
            Some((Name::new(key.text()), value))
        })
        .collect();

    alloc(Expr::Object {
        type_name: None,
        fields,
        spreads: vec![],
    })
}

/// Lower an array config value into `Expr::Array`.
fn lower_array_value(item: &cst::ConfigItem, alloc: &mut impl FnMut(Expr) -> ExprId) -> ExprId {
    // Use array_string_elements which gives us each element
    if let Some(elements) = item.array_string_elements() {
        let exprs: Vec<ExprId> = elements
            .into_iter()
            .map(|(maybe_str, _range)| {
                if let Some(s) = maybe_str {
                    alloc(Expr::Literal(Literal::String(s)))
                } else {
                    alloc(Expr::Null)
                }
            })
            .collect();
        return alloc(Expr::Array { elements: exprs });
    }

    alloc(Expr::Array { elements: vec![] })
}
