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
