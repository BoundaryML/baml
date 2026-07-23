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

    fn build_attribute(kind: SyntaxKind, argument_kinds: Option<&[SyntaxKind]>) -> SyntaxNode {
        let mut builder = SyntaxTreeBuilder::new();
        builder.start_node(kind);
        match kind {
            SyntaxKind::ATTRIBUTE => builder.token(SyntaxKind::AT, "@"),
            SyntaxKind::BLOCK_ATTRIBUTE => builder.token(SyntaxKind::AT_AT, "@@"),
            _ => unreachable!("expected attribute syntax kind"),
        }
        builder.token(SyntaxKind::WORD, "test");

        if let Some(argument_kinds) = argument_kinds {
            builder.start_node(SyntaxKind::ATTRIBUTE_ARGS);
            builder.token(SyntaxKind::L_PAREN, "(");
            for (index, argument_kind) in argument_kinds.iter().enumerate() {
                if index > 0 {
                    builder.token(SyntaxKind::COMMA, ",");
                }
                builder.start_node(*argument_kind);
                builder.token(SyntaxKind::WORD, "value");
                builder.finish_node();
            }
            builder.token(SyntaxKind::R_PAREN, ")");
            builder.finish_node();
        }

        builder.finish_node();
        SyntaxNode::new_root(builder.finish())
    }

    macro_rules! check_attribute_argument_accessors {
        ($attribute:ty, $kind:expr) => {{
            let attribute =
                <$attribute>::cast(build_attribute($kind, None)).expect("expected attribute");
            assert!(!attribute.has_args());
            assert!(attribute.args_span().is_none());
            assert_eq!(attribute.arg_count(), 0);
            assert!(!attribute.arg_is_string_literal());
            assert!(!attribute.arg_is_string_or_unquoted());

            let cases = [
                (&[][..], false, false),
                (&[SyntaxKind::STRING_LITERAL][..], true, true),
                (&[SyntaxKind::RAW_STRING_LITERAL][..], true, true),
                (&[SyntaxKind::EXPR][..], false, false),
                (&[SyntaxKind::UNQUOTED_STRING][..], false, true),
            ];

            for (argument_kinds, is_string_literal, is_string_or_unquoted) in cases {
                let attribute = <$attribute>::cast(build_attribute($kind, Some(argument_kinds)))
                    .expect("expected attribute");
                assert!(attribute.has_args());
                assert!(attribute.args_span().is_some());
                assert_eq!(attribute.arg_count(), argument_kinds.len());
                assert_eq!(attribute.arg_is_string_literal(), is_string_literal);
                assert_eq!(attribute.arg_is_string_or_unquoted(), is_string_or_unquoted);
                assert_eq!(
                    attribute
                        .args()
                        .map(|argument| argument.kind())
                        .collect::<Vec<_>>(),
                    argument_kinds
                );
            }
        }};
    }

    #[test]
    fn attribute_argument_accessors_agree_for_inline_and_block_attributes() {
        check_attribute_argument_accessors!(ast::Attribute, SyntaxKind::ATTRIBUTE);
        check_attribute_argument_accessors!(ast::BlockAttribute, SyntaxKind::BLOCK_ATTRIBUTE);
    }

    fn build_attribute_string_arg(
        kind: SyntaxKind,
        with_args: bool,
        argument_kind: Option<SyntaxKind>,
        tokens: &[(SyntaxKind, &str)],
    ) -> SyntaxNode {
        let mut builder = SyntaxTreeBuilder::new();
        builder.start_node(kind);
        match kind {
            SyntaxKind::ATTRIBUTE => builder.token(SyntaxKind::AT, "@"),
            SyntaxKind::BLOCK_ATTRIBUTE => builder.token(SyntaxKind::AT_AT, "@@"),
            _ => unreachable!("expected attribute syntax kind"),
        }
        builder.token(SyntaxKind::WORD, "test");

        if with_args {
            builder.start_node(SyntaxKind::ATTRIBUTE_ARGS);
            builder.token(SyntaxKind::L_PAREN, "(");
            if let Some(argument_kind) = argument_kind {
                builder.start_node(argument_kind);
                for &(token_kind, text) in tokens {
                    builder.token(token_kind, text);
                }
                builder.finish_node();
            }
            builder.token(SyntaxKind::R_PAREN, ")");
            builder.finish_node();
        }

        builder.finish_node();
        SyntaxNode::new_root(builder.finish())
    }

    macro_rules! check_attribute_string_args {
        ($attribute:ty, $kind:expr) => {{
            let cases: &[(
                bool,
                Option<SyntaxKind>,
                &[(SyntaxKind, &str)],
                Option<&str>,
            )] = &[
                (false, None, &[], None),
                (true, None, &[], None),
                (
                    true,
                    Some(SyntaxKind::STRING_LITERAL),
                    &[(SyntaxKind::WORD, "\"hello  world\\n\"")],
                    Some("hello  world\n"),
                ),
                (
                    true,
                    Some(SyntaxKind::RAW_STRING_LITERAL),
                    &[(SyntaxKind::WORD, "##\" raw  text \"##")],
                    Some(" raw  text "),
                ),
                (
                    true,
                    Some(SyntaxKind::UNQUOTED_STRING),
                    &[
                        (SyntaxKind::WHITESPACE, " "),
                        (SyntaxKind::WORD, "alpha"),
                        (SyntaxKind::BLOCK_COMMENT, "/* ignored */"),
                        (SyntaxKind::COMMA, ","),
                        (SyntaxKind::NEWLINE, "\n"),
                        (SyntaxKind::WORD, "beta"),
                    ],
                    Some("alphabeta"),
                ),
                (
                    true,
                    Some(SyntaxKind::RAW_STRING_LITERAL),
                    &[(SyntaxKind::WORD, "#\"unterminated")],
                    Some("#\"unterminated"),
                ),
                (
                    true,
                    Some(SyntaxKind::UNQUOTED_STRING),
                    &[
                        (SyntaxKind::WHITESPACE, " "),
                        (SyntaxKind::LINE_COMMENT, "// ignored"),
                        (SyntaxKind::COMMA, ","),
                    ],
                    None,
                ),
            ];

            for &(with_args, argument_kind, tokens, expected) in cases {
                let attribute = <$attribute>::cast(build_attribute_string_arg(
                    $kind,
                    with_args,
                    argument_kind,
                    tokens,
                ))
                .expect("expected attribute");
                assert_eq!(attribute.string_arg().as_deref(), expected);
            }
        }};
    }

    #[test]
    fn attribute_string_decoding_agrees_for_inline_and_block_attributes() {
        check_attribute_string_args!(ast::Attribute, SyntaxKind::ATTRIBUTE);
        check_attribute_string_args!(ast::BlockAttribute, SyntaxKind::BLOCK_ATTRIBUTE);
    }

    fn build_named_definition(kind: SyntaxKind) -> SyntaxNode {
        let mut builder = SyntaxTreeBuilder::new();
        builder.start_node(kind);
        builder.start_node(SyntaxKind::EXPR);
        builder.token(SyntaxKind::WORD, "nested");
        builder.finish_node();
        builder.token(SyntaxKind::WORD, "direct");
        builder.finish_node();
        SyntaxNode::new_root(builder.finish())
    }

    macro_rules! check_direct_definition_name {
        ($definition:ty, $kind:expr) => {
            assert_eq!(
                <$definition>::cast(build_named_definition($kind))
                    .expect("expected definition")
                    .name()
                    .expect("expected direct name")
                    .text(),
                "direct"
            );
        };
    }

    #[test]
    fn definition_names_ignore_nested_word_tokens() {
        check_direct_definition_name!(ast::TemplateStringDef, SyntaxKind::TEMPLATE_STRING_DEF);
        check_direct_definition_name!(ast::ClassDef, SyntaxKind::CLASS_DEF);
        check_direct_definition_name!(ast::InterfaceDef, SyntaxKind::INTERFACE_DEF);
        check_direct_definition_name!(ast::EnumDef, SyntaxKind::ENUM_DEF);
        check_direct_definition_name!(ast::ClientDef, SyntaxKind::CLIENT_DEF);
        check_direct_definition_name!(ast::RetryPolicyDef, SyntaxKind::RETRY_POLICY_DEF);
        check_direct_definition_name!(ast::TestDef, SyntaxKind::TEST_DEF);
        check_direct_definition_name!(ast::TypeAliasDef, SyntaxKind::TYPE_ALIAS_DEF);
    }

    fn build_arm(kind: SyntaxKind, with_arrow: bool, with_body: bool) -> SyntaxNode {
        let mut builder = SyntaxTreeBuilder::new();
        builder.start_node(kind);
        builder.start_node(SyntaxKind::EXPR);
        builder.token(SyntaxKind::WORD, "before");
        builder.finish_node();
        if with_arrow {
            builder.token(SyntaxKind::FAT_ARROW, "=>");
        }
        builder.token(SyntaxKind::WORD, "token");
        if with_body {
            builder.start_node(SyntaxKind::BLOCK_EXPR);
            builder.token(SyntaxKind::WORD, "body");
            builder.finish_node();
            builder.start_node(SyntaxKind::EXPR);
            builder.token(SyntaxKind::WORD, "after");
            builder.finish_node();
        }
        builder.finish_node();
        SyntaxNode::new_root(builder.finish())
    }

    macro_rules! check_arm_body {
        ($arm:ty, $kind:expr) => {{
            let arm = <$arm>::cast(build_arm($kind, true, true)).expect("expected arm");
            assert_eq!(
                arm.body().expect("expected body").kind(),
                SyntaxKind::BLOCK_EXPR
            );

            let arm = <$arm>::cast(build_arm($kind, false, true)).expect("expected arm");
            assert!(arm.body().is_none());

            let arm = <$arm>::cast(build_arm($kind, true, false)).expect("expected arm");
            assert!(arm.body().is_none());
        }};
    }

    #[test]
    fn match_and_catch_bodies_are_first_nodes_after_fat_arrows() {
        check_arm_body!(ast::MatchArm, SyntaxKind::MATCH_ARM);
        check_arm_body!(ast::CatchArm, SyntaxKind::CATCH_ARM);
    }
}
