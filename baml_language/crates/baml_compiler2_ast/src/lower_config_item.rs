//! CST `ConfigItem` → `Expr`.
//!
//! Generic lowering of CST config values into AST expressions.
//! Used by retry policy, client, and other config-block syntheses
//! so that each synthesis site only needs to wrap the results in
//! a typed `Expr::Object` rather than hand-parsing each field.

use baml_base::{Literal, Name};
use baml_compiler_syntax::{SyntaxKind, ast as cst};
use rowan::ast::AstNode;

use crate::{
    ast::{CallArg, Expr, ExprId},
    lower_expr_body::EnvVarRef,
};

/// Lower a config item's value to an `Expr`, allocating into the caller's arena.
///
/// Handles: integers, floats, quoted strings, `env.VAR` references,
/// bare words (as strings), nested blocks (as untyped objects),
/// and array literals.
///
/// Also collects env var references (`env.X`) into `env_var_refs`.
pub(crate) fn lower_config_value_with_env_refs(
    item: &cst::ConfigItem,
    alloc: &mut impl FnMut(Expr) -> ExprId,
    env_var_refs: &mut Vec<EnvVarRef>,
) -> ExprId {
    // 1. Nested block → Expr::Map (untyped) with recursively lowered children
    if let Some(nested) = item.nested_block() {
        return lower_config_block_to_map_with_env_refs(&nested, alloc, env_var_refs);
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
        env_var_refs.push(EnvVarRef {
            name: var_name.to_string(),
            range: item.syntax().text_range(),
        });
        let callee = alloc(Expr::Path(vec![
            Name::new("baml"),
            Name::new("env"),
            Name::new("get_or_panic"),
        ]));
        let arg = alloc(Expr::Literal(Literal::String(var_name.to_string())));
        return alloc(Expr::Call {
            callee,
            type_args: vec![],
            args: vec![CallArg::positional(arg)],
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

/// Lower a nested `ConfigBlock` into an `Expr::Map`.
///
/// Nested config blocks are untyped key-value structures (not class instances),
/// so they are lowered as maps. This ensures they work correctly when stored in
/// `map<string, unknown>` fields like `request_body`.
fn lower_config_block_to_map_with_env_refs(
    block: &cst::ConfigBlock,
    alloc: &mut impl FnMut(Expr) -> ExprId,
    env_var_refs: &mut Vec<EnvVarRef>,
) -> ExprId {
    let entries: Vec<(ExprId, ExprId)> = block
        .items()
        .filter_map(|item| {
            let key = item.key()?;
            let k = alloc(Expr::Literal(Literal::String(key.text().to_string())));
            let v = lower_config_value_with_env_refs(&item, alloc, env_var_refs);
            Some((k, v))
        })
        .collect();

    alloc(Expr::Map { entries })
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

#[cfg(test)]
mod tests {
    use baml_base::FileId;
    use baml_compiler_lexer::lex_lossless;
    use baml_compiler_parser::parse_file;
    use baml_compiler_syntax::{SyntaxNode, ast as cst};
    use rowan::ast::AstNode;

    use super::*;

    fn parse(source: &str) -> SyntaxNode {
        let tokens = lex_lossless(source, FileId::new(0));
        let (green, errors) = parse_file(&tokens);
        assert!(
            errors.is_empty(),
            "expected no parse errors, got: {errors:#?}"
        );
        SyntaxNode::new_root(green)
    }

    /// Find the first `ConfigItem` whose key matches `target_key` inside a
    /// client definition's config block.
    fn find_config_item(root: &SyntaxNode, target_key: &str) -> cst::ConfigItem {
        root.descendants()
            .filter_map(cst::ClientDef::cast)
            .flat_map(|client| {
                client
                    .config_block()
                    .into_iter()
                    .flat_map(|cb| cb.items().collect::<Vec<_>>())
            })
            .find(|item| item.key().map(|k| k.text().to_string()) == Some(target_key.to_string()))
            .expect("config item not found")
    }

    #[test]
    fn nested_block_lowers_to_map() {
        let source = r#"
client MyClient {
  provider openai
  options {
    model "gpt-4"
    temperature 0.7
  }
}
"#;
        let root = parse(source);
        let options_item = find_config_item(&root, "options");

        let mut exprs: la_arena::Arena<Expr> = la_arena::Arena::new();
        let mut alloc = |expr: Expr| -> ExprId { exprs.alloc(expr) };

        let result_id =
            lower_config_value_with_env_refs(&options_item, &mut alloc, &mut Vec::new());
        let result_expr = &exprs[result_id];

        match result_expr {
            Expr::Map { entries } => {
                assert_eq!(entries.len(), 2);

                // First entry: "model" => "gpt-4"
                assert_eq!(
                    exprs[entries[0].0],
                    Expr::Literal(Literal::String("model".to_string()))
                );
                assert_eq!(
                    exprs[entries[0].1],
                    Expr::Literal(Literal::String("gpt-4".to_string()))
                );

                // Second entry: "temperature" => 0.7
                assert_eq!(
                    exprs[entries[1].0],
                    Expr::Literal(Literal::String("temperature".to_string()))
                );
                assert_eq!(
                    exprs[entries[1].1],
                    Expr::Literal(Literal::Float("0.7".to_string()))
                );
            }
            other => panic!("expected Expr::Map, got {other:?}"),
        }
    }

    #[test]
    fn doubly_nested_block_lowers_to_nested_maps() {
        let source = r#"
client MyClient {
  provider openai
  options {
    headers {
      x-api-key "secret"
    }
  }
}
"#;
        let root = parse(source);
        let options_item = find_config_item(&root, "options");

        let mut exprs: la_arena::Arena<Expr> = la_arena::Arena::new();
        let mut alloc = |expr: Expr| -> ExprId { exprs.alloc(expr) };

        let result_id =
            lower_config_value_with_env_refs(&options_item, &mut alloc, &mut Vec::new());
        let outer_map = &exprs[result_id];

        let entries = match outer_map {
            Expr::Map { entries } => entries.clone(),
            other => panic!("expected Expr::Map, got {other:?}"),
        };
        assert_eq!(entries.len(), 1);

        assert_eq!(
            exprs[entries[0].0],
            Expr::Literal(Literal::String("headers".to_string()))
        );

        // The value should itself be a Map
        let inner_map = &exprs[entries[0].1];
        let inner_entries = match inner_map {
            Expr::Map { entries } => entries,
            other => panic!("expected nested Expr::Map, got {other:?}"),
        };
        assert_eq!(inner_entries.len(), 1);
        assert_eq!(
            exprs[inner_entries[0].0],
            Expr::Literal(Literal::String("x-api-key".to_string()))
        );
        assert_eq!(
            exprs[inner_entries[0].1],
            Expr::Literal(Literal::String("secret".to_string()))
        );
    }
}
