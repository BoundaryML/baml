use baml_syntax::{index::TreeIndex, nodes::FileTree, tree::TreeWalk as _};
use bumpalo::Bump;

use super::{Comments, Gap, Position, SourceComment, docs};
use crate::{
    FormatOptions,
    ir::{Doc, Docs},
};

fn parse(source: &str) -> (FileTree, TreeIndex) {
    let parsed = baml_parser::parse(source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let index = TreeIndex::build(&parsed.tree);
    (parsed.tree, index)
}

/// Every comment of `source` as `text -> Position Node [Placement]`, in
/// source order; a dangling comment names the token it follows.
fn attachments(source: &str) -> Vec<String> {
    let (tree, index) = parse(source);
    let comments = Comments::from_tree(&tree, &index);
    let mut all = comments.all().to_vec();
    all.sort_by_key(|c| c.token);
    all.iter()
        .map(|c| {
            let position = match c.position {
                Position::Leading => "Leading".to_owned(),
                Position::Trailing => "Trailing".to_owned(),
                Position::Dangling { after: None } => "Dangling(start)".to_owned(),
                Position::Dangling { after: Some(token) } => {
                    format!("Dangling(after {:?})", tree.token_text(token))
                }
            };
            format!(
                "{} -> {position} {} [{:?}]",
                c.text(&tree),
                tree.arenas().kind_name(c.node),
                c.placement
            )
        })
        .collect()
}

/// The first comment of `source` whose text is `text`.
fn comment_named(tree: &FileTree, comments: &Comments, text: &str) -> SourceComment {
    comments
        .all()
        .iter()
        .find(|c| c.text(tree) == text)
        .copied()
        .unwrap_or_else(|| panic!("no comment {text:?}"))
}

fn print(root: Doc<'_>) -> String {
    root.print(FormatOptions {
        line_width: 80,
        indent_width: 4,
    })
}

#[test]
fn own_line_comments_lead_the_following_child() {
    assert_eq!(
        attachments("// top\nfunction f() -> int {\n    // before a\n    let a = 1;\n    a\n}\n"),
        [
            "// top -> Leading FunctionDef [OwnLine]",
            "// before a -> Leading LetStmt [OwnLine]",
        ]
    );
}

#[test]
fn end_of_line_comments_trail_the_preceding_child() {
    assert_eq!(
        attachments(
            "function f() -> int {\n    let a = 1; // same line\n    // own line\n    a\n}\n"
        ),
        [
            "// same line -> Trailing LetStmt [EndOfLine]",
            "// own line -> Leading PathExpr [OwnLine]",
        ]
    );
    assert_eq!(
        attachments("function f() -> int {\n    g();\n    // after the last\n}\n"),
        ["// after the last -> Trailing ExprStmt [OwnLine]"]
    );
}

#[test]
fn a_comment_after_a_list_separator_belongs_to_the_item_before_it() {
    // `,` belongs to the argument list, not to `a`; the comment still trails
    // `a`, where a line suffix prints it after the comma.
    assert_eq!(
        attachments("function f() -> int {\n    g(a, // c\n      b)\n}\n"),
        ["// c -> Trailing Arg [EndOfLine]"]
    );
    assert_eq!(
        attachments("type T = A | // c\n    B;\n"),
        ["// c -> Trailing PathType [EndOfLine]"]
    );
}

#[test]
fn a_comment_never_crosses_a_token_of_the_enclosing_node() {
    // Between the doc line and the keyword: it trails the doc line rather
    // than leading the name on the far side of `function`.
    assert_eq!(
        attachments("/// a\n// c\nfunction f() -> int { 1 }\n"),
        ["// c -> Trailing DocLine [OwnLine]"]
    );
    assert_eq!(
        attachments("function f() -> map<string, int> {\n    map /* c */ { \"a\": 1 }\n}\n"),
        ["/* c */ -> Dangling(after \"map\") MapLiteral [SameLine]"]
    );
    assert_eq!(
        attachments("function f() -> int {\n    return // c\n        5\n}\n"),
        ["// c -> Dangling(after \"return\") ReturnExpr [EndOfLine]"]
    );
    assert_eq!(
        attachments("function f() -> int {\n    let x = /* v */ 2;\n    x\n}\n"),
        ["/* v */ -> Leading LiteralExpr [SameLine]"]
    );
}

#[test]
fn a_comment_after_an_opening_bracket_dangles_after_it_unless_on_its_own_line() {
    assert_eq!(
        attachments("class Foo { // on the brace line\n    x: int\n}\n"),
        ["// on the brace line -> Dangling(after \"{\") ClassDef [EndOfLine]"]
    );
    assert_eq!(
        attachments("class Foo {\n    // nothing here\n}\n"),
        ["// nothing here -> Dangling(after \"{\") ClassDef [OwnLine]"]
    );
    assert_eq!(
        attachments("function f() -> int {\n    g( // c\n        a)\n}\n"),
        ["// c -> Dangling(after \"(\") ArgList [EndOfLine]"]
    );
    assert_eq!(
        attachments("function f() -> int {\n    g(\n        // c\n        a)\n}\n"),
        ["// c -> Leading Arg [OwnLine]"]
    );
    assert_eq!(
        attachments("function f() -> int[] { [ /* c */ ] }\n"),
        ["/* c */ -> Dangling(after \"[\") ArrayLiteral [SameLine]"]
    );
    assert_eq!(
        attachments(
            "function f() -> int {\n    g() catch ( // p\n        e) { // b\n        _ => 1,\n    }\n}\n"
        ),
        [
            "// p -> Dangling(after \"(\") CatchClause [EndOfLine]",
            "// b -> Dangling(after \"{\") CatchClause [EndOfLine]",
        ]
    );
}

#[test]
fn same_line_block_comments_attach_to_the_child_they_touch() {
    assert_eq!(
        attachments(
            "function f(a: int, b: int) -> int[] {\n    let x = 2;\n    [a /* p */, b, /* q */ x, /* r */]\n}\n"
        ),
        [
            "/* p */ -> Trailing PathExpr [SameLine]",
            "/* q */ -> Leading PathExpr [SameLine]",
            "/* r */ -> Trailing PathExpr [SameLine]",
        ]
    );
}

#[test]
fn a_run_of_comments_on_one_line_is_placed_as_one() {
    assert_eq!(
        attachments("function f() -> int {\n    foo();\n    /* a */ /* b */ bar()\n}\n"),
        [
            "/* a */ -> Leading CallExpr [OwnLine]",
            "/* b */ -> Leading CallExpr [OwnLine]",
        ]
    );
    assert_eq!(
        attachments("function f() -> int {\n    foo(); /* x */ // y\n    bar()\n}\n"),
        [
            "/* x */ -> Trailing ExprStmt [EndOfLine]",
            "// y -> Trailing ExprStmt [EndOfLine]",
        ]
    );
    assert_eq!(
        attachments("function f() -> int {\n    g(a, /* x */ // y\n      b)\n}\n"),
        [
            "/* x */ -> Trailing Arg [EndOfLine]",
            "// y -> Trailing Arg [EndOfLine]",
        ]
    );
}

#[test]
fn comments_at_the_edges_of_the_file_belong_to_the_outermost_item_or_the_root() {
    assert_eq!(
        attachments("// first\n\nclass A {}\n\n// last\n"),
        [
            "// first -> Leading ClassDef [OwnLine]",
            "// last -> Trailing ClassDef [OwnLine]",
        ]
    );
    assert_eq!(
        attachments("// alone\n\n/* and this */\n"),
        [
            "// alone -> Dangling(start) SourceFile [OwnLine]",
            "/* and this */ -> Dangling(start) SourceFile [OwnLine]",
        ]
    );
    assert_eq!(attachments(""), Vec::<String>::new());
}

#[test]
fn owned_comment_tokens_are_not_placed() {
    // A `//#` line in a block is a statement, so the plain comment after it
    // leads the next statement. A `///` line is the declaration's own.
    assert_eq!(
        attachments(
            "/// doc\n// plain between docs\n/// more\nfunction f() -> int {\n    //# section // not a comment\n    // after the header\n    g()\n}\n"
        ),
        [
            "// plain between docs -> Leading DocLine [OwnLine]",
            "// after the header -> Leading CallExpr [OwnLine]",
        ]
    );
}

#[test]
fn the_else_keyword_does_not_come_between_a_block_and_its_trailing_comment() {
    assert_eq!(
        attachments(
            "function f(c: bool) -> int {\n    if (c) {\n        1\n    } // then\n    else {\n        2\n    }\n}\n"
        ),
        ["// then -> Trailing Block [EndOfLine]"]
    );
}

#[test]
fn gaps_count_the_line_breaks_on_each_side() {
    let (tree, index) = parse("class A {}\n\n\n// two blank lines before\n// none\n\nclass B {}\n");
    let comments = Comments::from_tree(&tree, &index);
    let two = comment_named(&tree, &comments, "// two blank lines before");
    assert_eq!(two.before, Gap::Lines(3.try_into().unwrap()));
    assert_eq!(two.after, Gap::Lines(1.try_into().unwrap()));
    assert!(two.before.has_blank_line() && !two.after.has_blank_line());
    assert_eq!(two.position, Position::Leading);
    assert_eq!(comments.gap_before_node(&tree, two.node), two.before);
    let none = comment_named(&tree, &comments, "// none");
    assert_eq!(none.after, Gap::Lines(2.try_into().unwrap()));
    assert_eq!(none.node, two.node);
}

#[test]
fn a_node_s_comments_are_one_run_in_print_order() {
    let (tree, index) =
        parse("function f() -> int {\n    // lead\n    g(); // trail\n    // also trail\n}\n");
    let comments = Comments::from_tree(&tree, &index);
    let stmt = comment_named(&tree, &comments, "// lead").node;
    assert_eq!(tree.arenas().kind_name(stmt), "ExprStmt");
    assert_eq!(comments.of(stmt).len(), 3);
    assert_eq!(comments.leading(stmt).len(), 1);
    assert!(comments.dangling(stmt).is_empty());
    assert_eq!(comments.trailing(stmt).len(), 2);
    assert_eq!(
        comments
            .of(stmt)
            .iter()
            .map(|c| c.position)
            .collect::<Vec<_>>(),
        [Position::Leading, Position::Trailing, Position::Trailing]
    );
    assert_eq!(
        comments.gap_before_node(&tree, stmt),
        Gap::Lines(1.try_into().unwrap())
    );
}

#[test]
fn leading_and_trailing_comments_print_where_they_were_written() {
    let (tree, index) = parse(
        "function f() -> int {\n    // lead\n\n    /* inline */ g(); // trail\n    /* block */\n\n    // after a blank\n}\n",
    );
    let comments = Comments::from_tree(&tree, &index);
    let stmt = comment_named(&tree, &comments, "// lead").node;
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let body = d.concat([
        docs::leading(d, &tree, comments.leading(stmt)),
        d.text("g();"),
        docs::trailing(d, &tree, comments.trailing(stmt)),
    ]);
    let root = d.concat([
        d.text("{"),
        d.indent(d.concat([Doc::HARD_LINE, body])),
        Doc::HARD_LINE,
        d.text("}"),
    ]);
    assert_eq!(
        print(root),
        "{\n    // lead\n\n    /* inline */ g(); // trail\n    /* block */\n\n    // after a blank\n}"
    );
}

#[test]
fn a_leading_comment_on_its_own_line_starts_one_wherever_the_rule_is() {
    let (tree, index) =
        parse("function f() -> int {\n    let x =\n        // c\n        5;\n    x\n}\n");
    let comments = Comments::from_tree(&tree, &index);
    let five = comment_named(&tree, &comments, "// c");
    assert_eq!(five.position, Position::Leading);
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let rhs = d.indent(d.concat([
        docs::leading(d, &tree, comments.leading(five.node)),
        d.text("5"),
    ]));
    let root = d.concat([d.text("let x ="), Doc::SPACE, rhs, d.text(";")]);
    assert_eq!(print(root), "let x =\n    // c\n    5;");
}

#[test]
fn dangling_comments_print_after_the_token_they_follow() {
    let (tree, index) = parse("function f() -> int { // here\n    /* and here */\n}\n");
    let comments = Comments::from_tree(&tree, &index);
    let block = comment_named(&tree, &comments, "// here").node;
    assert_eq!(comments.dangling(block).len(), 2);
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = d.concat([
        d.text("{"),
        d.indent(docs::dangling(d, &tree, comments.dangling(block), true)),
        Doc::HARD_LINE,
        d.text("}"),
    ]);
    assert_eq!(print(root), "{ // here\n    /* and here */\n}");
}

#[test]
fn two_end_of_line_comments_never_merge_into_one() {
    let (tree, index) = parse(
        "function f(c: bool) -> int {\n    if (c) {\n        1\n    } // then\n    else { // else\n        2\n    }\n}\n",
    );
    let comments = Comments::from_tree(&tree, &index);
    let then_block = comment_named(&tree, &comments, "// then").node;
    let else_block = comment_named(&tree, &comments, "// else").node;
    let bump = Bump::new();
    let d = Docs::new(&bump);
    // A rule that keeps `else {` on the `}` line: the second comment ends
    // the line the first is pending on instead of joining it.
    let root = d.concat([
        d.text("}"),
        docs::trailing(d, &tree, comments.trailing(then_block)),
        d.text(" else {"),
        docs::dangling(d, &tree, comments.dangling(else_block), true),
        Doc::HARD_LINE,
        d.text("}"),
    ]);
    assert_eq!(print(root), "} else { // then\n// else\n}");
}
