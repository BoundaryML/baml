//! Focused CST goldens for type atoms and upcast targets. Source fixtures also
//! run through the compiler diagnostics/runtime suites; these goldens pin the
//! grammar ownership itself rather than inferring it from downstream behavior.

use baml_db::{
    FileId,
    baml_compiler_lexer::lex_lossless,
    baml_compiler_parser::parse_file,
    baml_compiler_syntax::{SyntaxKind, SyntaxNode},
};

fn snapshot_nodes(name: &str, source: &str, invalid: bool) {
    let tokens = lex_lossless(source, FileId::new(0));
    let (green, errors) = parse_file(&tokens);
    let root = SyntaxNode::new_root(green);
    assert_eq!(
        root.text().to_string(),
        source,
        "CST must retain all source text"
    );
    assert_eq!(!errors.is_empty(), invalid, "{name}: {errors:#?}");
    // Snapshot aliases, upcasts and method type arguments, keeping unrelated
    // runtime test bodies out of the golden. Recovery and ownership stay visible.
    let nodes = root
        .descendants()
        .filter(|node| {
            matches!(
                node.kind(),
                SyntaxKind::TYPE_ALIAS_DEF | SyntaxKind::UPCAST_EXPR | SyntaxKind::GENERIC_ARGS
            )
        })
        .map(|node| format!("{node:#?}"))
        .collect::<Vec<_>>()
        .join("\n");
    insta::with_settings!({snapshot_path => "../snapshots/cst_type_nodes", omit_expression => true}, {
        insta::assert_snapshot!(name, nodes);
    });
}

#[test]
fn invalid_type_nodes_preserve_recovery_syntax() {
    for (name, source) in [
        (
            "upcast",
            include_str!("../projects/broken_syntax/cst_type_nodes/upcast.baml"),
        ),
        (
            "literal",
            include_str!("../projects/broken_syntax/cst_type_nodes/literal.baml"),
        ),
        (
            "grouping",
            include_str!("../projects/broken_syntax/cst_type_nodes/grouping.baml"),
        ),
    ] {
        snapshot_nodes(name, source, true);
    }
}

#[test]
fn valid_type_nodes_own_their_complete_operands() {
    snapshot_nodes(
        "valid",
        include_str!("../baml_src/ns_cst_type_nodes/type_nodes.baml"),
        false,
    );
}
