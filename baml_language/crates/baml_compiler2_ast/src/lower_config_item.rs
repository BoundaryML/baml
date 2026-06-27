//! CST `ConfigItem` → `Expr`.
//!
//! Generic lowering of CST config values into AST expressions.
//! Used by retry policy, client, and other config-block syntheses
//! so that each synthesis site only needs to wrap the results in
//! a typed `Expr::Object` rather than hand-parsing each field.

use baml_base::{Literal, Name};
use baml_compiler_syntax::{SyntaxKind, SyntaxNode, SyntaxNodeExt, ast as cst};
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

    // 2. Value node — array literal (recursing per element) or scalar.
    let Some(cv_node) = item.config_value_node() else {
        return alloc(Expr::Null);
    };
    lower_config_value_node(&cv_node, cv_node.span_range(), alloc, env_var_refs)
}

/// Lower a `CONFIG_VALUE` node into an `Expr`.
///
/// Handles array literals (recursing per element) and scalars: integers,
/// floats, `env.VAR` references, bool literals, and quoted / bare-word strings.
/// Shared by a config item's value and every array element so that a non-string
/// element — e.g. the `123` in `allowed_roles ["user", 123]` — keeps its real
/// type instead of being erased to `Expr::Null` (which would surface to the user
/// as a spurious `got null` type error).
///
/// `env_range` is the span recorded for an `env.VAR` reference found directly at
/// this node; array elements pass their own element span.
fn lower_config_value_node(
    cv_node: &SyntaxNode,
    env_range: rowan::TextRange,
    alloc: &mut impl FnMut(Expr) -> ExprId,
    env_var_refs: &mut Vec<EnvVarRef>,
) -> ExprId {
    // Array literal → recurse per element so each keeps its real type.
    if let Some(array_literal) = cv_node
        .children()
        .find(|child| child.kind() == SyntaxKind::ARRAY_LITERAL)
    {
        let elements: Vec<ExprId> = array_literal
            .children()
            .filter_map(|element| match element.kind() {
                SyntaxKind::CONFIG_VALUE => {
                    let element_range = element.span_range();
                    Some(lower_config_value_node(
                        &element,
                        element_range,
                        alloc,
                        env_var_refs,
                    ))
                }
                // A `{ … }` array element (e.g. `tools [{ url "…" }]`) is a bare
                // `CONFIG_BLOCK`; lower it to an untyped map like a nested block
                // value, instead of silently dropping it.
                SyntaxKind::CONFIG_BLOCK => cst::ConfigBlock::cast(element).map(|block| {
                    lower_config_block_to_map_with_env_refs(&block, alloc, env_var_refs)
                }),
                _ => None,
            })
            .collect();
        return alloc(Expr::Array { elements });
    }

    // Scalar — classify by token kind. Using `.all()` ensures mixed-token values
    // like `"anthropic.claude-3-haiku-20240307-v1:0"` (which contains an
    // INTEGER_LITERAL `0` among other tokens) are NOT misclassified as numbers.
    //
    // BUG: a few scalar forms still lower to a wrong-typed string rather than
    // their literal because the config grammar/lexer doesn't surface them here:
    // a negative number (`-5`) tokenizes as `MINUS` + `INTEGER_LITERAL`, so
    // `is_int` is false and it becomes `String("-5")`; a bigint literal (`42n`)
    // and an out-of-`i64`-range integer also fall through to a string; and an
    // `env.X` reference *inside an array* mis-parses upstream. Fixing these
    // belongs in the parser/lexer, not this lowering.
    let non_trivia = || {
        cv_node
            .descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| !token.kind().is_trivia())
    };
    let is_int = non_trivia().all(|token| token.kind() == SyntaxKind::INTEGER_LITERAL);
    let is_float = non_trivia().all(|token| token.kind() == SyntaxKind::FLOAT_LITERAL);
    let scalar_text = cst::ConfigValue::cast(cv_node.clone()).and_then(|cv| cv.scalar_text());

    if is_float && let Some(text) = &scalar_text {
        return alloc(Expr::Literal(Literal::Float(text.clone())));
    }

    if is_int
        && let Some(value) = non_trivia()
            .find(|token| token.kind() == SyntaxKind::INTEGER_LITERAL)
            .and_then(|token| token.text().parse::<i64>().ok())
    {
        return alloc(Expr::Literal(Literal::Int(value)));
    }

    // Fall through to text-based analysis.
    let text = scalar_text.unwrap_or_default();

    // env.VAR_NAME → baml.env.get_or_panic("VAR_NAME")
    if let Some(var_name) = text.strip_prefix("env.") {
        env_var_refs.push(EnvVarRef {
            name: var_name.to_string(),
            range: env_range,
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
            .filter_map(cst::ConfigItem::cast)
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

    #[test]
    fn commented_numeric_config_value_lowers_to_int() {
        let source = r#"
client MyClient {
  provider openai
  options {
    max_retries // key trailing
    // value leading
    3 // value trailing
    max_delay_ms /* key trailing */
    /* value leading */
    1000 /* value trailing */
  }
}
"#;
        let root = parse(source);
        let max_retries = find_config_item(&root, "max_retries");
        let max_delay_ms = find_config_item(&root, "max_delay_ms");

        let mut exprs: la_arena::Arena<Expr> = la_arena::Arena::new();
        let mut alloc = |expr: Expr| -> ExprId { exprs.alloc(expr) };

        let max_retries_id =
            lower_config_value_with_env_refs(&max_retries, &mut alloc, &mut Vec::new());
        let max_delay_ms_id =
            lower_config_value_with_env_refs(&max_delay_ms, &mut alloc, &mut Vec::new());

        assert_eq!(exprs[max_retries_id], Expr::Literal(Literal::Int(3)));
        assert_eq!(exprs[max_delay_ms_id], Expr::Literal(Literal::Int(1000)));
    }

    #[test]
    fn array_keeps_non_string_element_types() {
        // Regression: a non-string array element (`123`) must lower to its real
        // literal, not `Expr::Null`. Erasing it to null makes the type checker
        // report a bogus `got null` rather than `got 123` for the bad value.
        let source = r#"
client MyClient {
  provider openai
  options {
    allowed_roles ["user", 123, "assistant"]
  }
}
"#;
        let root = parse(source);
        let allowed_roles = find_config_item(&root, "allowed_roles");

        let mut exprs: la_arena::Arena<Expr> = la_arena::Arena::new();
        let mut alloc = |expr: Expr| -> ExprId { exprs.alloc(expr) };

        let result_id =
            lower_config_value_with_env_refs(&allowed_roles, &mut alloc, &mut Vec::new());

        let elements = match &exprs[result_id] {
            Expr::Array { elements } => elements.clone(),
            other => panic!("expected Expr::Array, got {other:?}"),
        };
        assert_eq!(elements.len(), 3);
        assert_eq!(
            exprs[elements[0]],
            Expr::Literal(Literal::String("user".to_string()))
        );
        assert_eq!(exprs[elements[1]], Expr::Literal(Literal::Int(123)));
        assert_eq!(
            exprs[elements[2]],
            Expr::Literal(Literal::String("assistant".to_string()))
        );
    }

    #[test]
    fn array_keeps_config_block_elements() {
        // Regression (M7): a `{ … }` array element is a bare CONFIG_BLOCK; it must
        // lower to an untyped map, not be silently dropped from the array.
        let source = r#"
client MyClient {
  provider openai
  options {
    tools [{ url "https://example.com" }, "plain"]
  }
}
"#;
        let root = parse(source);
        let tools = find_config_item(&root, "tools");

        let mut exprs: la_arena::Arena<Expr> = la_arena::Arena::new();
        let mut alloc = |expr: Expr| -> ExprId { exprs.alloc(expr) };

        let result_id = lower_config_value_with_env_refs(&tools, &mut alloc, &mut Vec::new());

        let elements = match &exprs[result_id] {
            Expr::Array { elements } => elements.clone(),
            other => panic!("expected Expr::Array, got {other:?}"),
        };
        assert_eq!(elements.len(), 2, "the block element must not be dropped");
        assert!(
            matches!(exprs[elements[0]], Expr::Map { .. }),
            "block array element should lower to a map, got {:?}",
            exprs[elements[0]]
        );
        assert_eq!(
            exprs[elements[1]],
            Expr::Literal(Literal::String("plain".to_string()))
        );
    }
}
