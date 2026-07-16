#[cfg(test)]
mod builder_tests {
    use rowan::ast::AstNode;

    use crate::{SyntaxKind, SyntaxNode, ast, builder::SyntaxTreeBuilder};

    #[test]
    fn test_build_function() {
        let green = SyntaxTreeBuilder::build_function(
            "GetUser",
            &[("id", "int"), ("name", "string")],
            "User",
        );

        let root = SyntaxNode::new_root(green);
        let source_file = ast::SourceFile::cast(root).unwrap();

        let function = source_file
            .items()
            .find_map(|item| match item {
                ast::Item::Function(f) => Some(f),
                _ => None,
            })
            .unwrap();

        assert_eq!(function.name().unwrap().text(), "GetUser");

        let params: Vec<_> = function.param_list().unwrap().params().collect();
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_build_class() {
        let green = SyntaxTreeBuilder::build_class("User", &[("name", "string"), ("age", "int")]);

        let root = SyntaxNode::new_root(green);
        let source_file = ast::SourceFile::cast(root).unwrap();

        let class = source_file
            .items()
            .find_map(|item| match item {
                ast::Item::Class(c) => Some(c),
                _ => None,
            })
            .unwrap();

        assert_eq!(class.name().unwrap().text(), "User");

        let fields: Vec<_> = class.fields().collect();
        assert_eq!(fields.len(), 2);
    }

    #[test]
    fn integer_literal_rejects_double_minus() {
        // `--42` must NOT decode to `+42` — a malformed sign sequence is
        // invalid, not a noop double-negation. Build a TYPE_EXPR with
        // [MINUS, MINUS, INTEGER_LITERAL "42"] and assert integer_literal()
        // returns None.
        let mut builder = SyntaxTreeBuilder::new();
        builder.start_node(SyntaxKind::TYPE_EXPR);
        builder.token(SyntaxKind::MINUS, "-");
        builder.token(SyntaxKind::MINUS, "-");
        builder.token(SyntaxKind::INTEGER_LITERAL, "42");
        builder.finish_node();
        let green = builder.finish();
        let root = SyntaxNode::new_root(green);
        let type_expr = ast::TypeExpr::cast(root).expect("expected TYPE_EXPR");
        assert!(type_expr.integer_literal().is_none());
    }

    #[test]
    fn integer_literal_accepts_single_minus() {
        let mut builder = SyntaxTreeBuilder::new();
        builder.start_node(SyntaxKind::TYPE_EXPR);
        builder.token(SyntaxKind::MINUS, "-");
        builder.token(SyntaxKind::INTEGER_LITERAL, "42");
        builder.finish_node();
        let green = builder.finish();
        let root = SyntaxNode::new_root(green);
        let type_expr = ast::TypeExpr::cast(root).expect("expected TYPE_EXPR");
        let (negated, tok) = type_expr
            .integer_literal()
            .expect("expected signed literal");
        assert!(negated);
        assert_eq!(tok.text(), "42");
    }

    #[test]
    fn float_literal_rejects_double_minus() {
        let mut builder = SyntaxTreeBuilder::new();
        builder.start_node(SyntaxKind::TYPE_EXPR);
        builder.token(SyntaxKind::MINUS, "-");
        builder.token(SyntaxKind::MINUS, "-");
        builder.token(SyntaxKind::FLOAT_LITERAL, "3.14");
        builder.finish_node();
        let green = builder.finish();
        let root = SyntaxNode::new_root(green);
        let type_expr = ast::TypeExpr::cast(root).expect("expected TYPE_EXPR");
        assert!(type_expr.float_literal().is_none());
    }

    #[test]
    fn test_tree_is_lossless() {
        let mut builder = SyntaxTreeBuilder::new();

        builder.start_node(SyntaxKind::SOURCE_FILE);
        builder.token(SyntaxKind::WORD, "function");
        builder.token(SyntaxKind::WHITESPACE, " ");
        builder.token(SyntaxKind::WORD, "test");
        builder.token(SyntaxKind::L_PAREN, "(");
        builder.token(SyntaxKind::R_PAREN, ")");
        builder.finish_node();

        let green = builder.finish();
        let root = SyntaxNode::new_root(green);

        assert_eq!(root.text(), "function test()");
    }
}
