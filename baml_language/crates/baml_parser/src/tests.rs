//! Declaration-layer tests: the tree shape and diagnostics for small inputs.
//! Bodies and expressions are `Bogus` until their layers land.

use std::fmt::Write as _;

use baml_syntax::{nodes::Arenas, token::TokenIdx};

use crate::parse;

fn tree(text: &str) -> String {
    let parse = parse(text);
    let mut out = parse.tree.debug_tree();
    if !parse.errors.is_empty() {
        out.push_str("---\n");
        for error in &parse.errors {
            let _ = writeln!(out, "{error:?}");
        }
    }
    out
}

#[test]
fn smoke() {
    let text = "class Foo { x: int }";
    let parse = parse(text);
    assert!(parse.errors.is_empty(), "{:?}", parse.errors);
    assert_eq!(
        parse
            .tree
            .list(parse.tree.get(parse.tree.root()).items)
            .len(),
        1
    );
    insta::assert_snapshot!(tree(text));
}

/// Print the tree and diagnostics of one file: `BAML_PARSE_DUMP=<path>`.
#[test]
#[ignore = "manual tool: set BAML_PARSE_DUMP=<file> and run with `--ignored --nocapture` (nextest: `--run-ignored all --no-capture`)"]
fn dump() {
    let path =
        baml_env::os_var("BAML_PARSE_DUMP").expect("BAML_PARSE_DUMP names the file to parse");
    let text = std::fs::read_to_string(&path).expect("readable file");
    #[expect(clippy::print_stdout, reason = "a manual dump tool")]
    {
        println!("{}", tree(&text));
    }
}

#[test]
fn function_with_llm_body_and_attributes() {
    insta::assert_snapshot!(tree(
        "@@trace(\"x\")\nfunction Classify(text: string, n: int = 3) -> Category throws Err {\n  client: \"openai/gpt-4o\"\n  prompt: \"Hello\"\n}\n"
    ));
}

#[test]
fn function_with_expression_body() {
    insta::assert_snapshot!(tree("function f<T>(x: T) -> T { x }"));
}

#[test]
fn interface_with_signatures_and_implements_for() {
    insta::assert_snapshot!(tree(
        "interface Named<T> requires Show {\n  type Item extends Bound = int,\n  function name(self) -> string;\n  label: string\n}\nimplements Named<int> for Foo {\n  type Item = int\n  name as label,\n  function name(self) -> string { \"\" }\n}\n"
    ));
}

#[test]
fn enum_with_attributes() {
    insta::assert_snapshot!(tree(
        "enum E {\n  @@alias(\"e\")\n  A @alias(\"a\") @description({{ this }}),\n  B;\n}"
    ));
}

#[test]
fn structural_types() {
    insta::assert_snapshot!(tree(
        "type T = (a: int, b?: string) -> map<string, int[]?> | \"lit\" | -1 | (Base as Iface).Item;\nclass C {\n  f: (A | B)[]\n}"
    ));
}

#[test]
fn removed_items_and_recovery() {
    insta::assert_snapshot!(tree(
        "client<llm> GPT { provider openai }\nretry_policy R { max_retries 3 }\ngenerator g { output_type python }\ntemplate_string Tpl(x: string) #\"hi\"#\ntype_builder { class X { a int } }\ndynamic class D { a int }\ntest Legacy { functions [F] args { a 1 } }\nclasss Oops { a int }\ntyppe Two = int\n"
    ));
}

#[test]
fn missing_pieces_are_reported_once() {
    insta::assert_snapshot!(tree(
        "function (x) -> {\nclass {\nenum E { A int | string }\n"
    ));
}

#[test]
fn header_comments_outside_blocks_are_reported() {
    insta::assert_snapshot!(tree(
        "//# ok before an expression function\nfunction f() -> int { 1 }\n//# not ok before a class\nclass C { //# nor in a body\n a: int }\n//# not ok before an LLM function\nfunction g() -> int {\n  client: \"x\"\n  prompt: \"y\"\n}\n"
    ));
}

/// A `//#` line directly above a function is its title, one per function,
/// in any order with its docs; above a method too. A second one, or one
/// above any other declaration, is an error.
#[test]
fn function_titles_are_owned() {
    insta::assert_snapshot!(tree(
        "//# Title\n/// doc\nfunction f() -> int { 1 }\n/// doc first\n//## Second-level title\nfunction g() -> int {\n  client: \"x\"\n  prompt: \"y\"\n}\n//# one\n//# two\nfunction h() -> int { 1 }\nclass C {\n  //# method title\n  function m(self) -> int { 1 }\n}\n"
    ));
}

/// Shapes whose recovery once consumed nothing and looped forever: a map or
/// object member at a token that starts no expression, `@@` in an
/// `implements` body, a catch arm whose pattern fails before its `=>`.
#[test]
fn zero_progress_shapes_terminate() {
    insta::assert_snapshot!(tree(
        "function f() -> int {\n  let m = map { < };\n  let o = Foo { catch_all_panics a: 1 };\n  x catch (e) { - => 1 }\n}\nclass A { x: int, implements I { @@dynamic } }\nimplements I for A { @@ }\n"
    ));
}

/// Lookaheads that accepted more than the grammar: a disagreement is a
/// diagnostic, never a panic.
#[test]
fn lookahead_disagreements_are_reported() {
    insta::assert_snapshot!(tree(
        "interface I requires <T> { x: int }\nfunction f() -> int {\n  let y = (f(x) as Foo).bar;\n  let z = ([1] as I).y;\n  1\n}\nclass C {\n  a: int\n  /// trailing doc at the end of the file"
    ));
}

/// `for x in xs` names its missing `let`; a backtick string is not a type;
/// an unclosed multi-tick template is reported at its opener.
#[test]
fn header_shapes_are_reported_in_place() {
    insta::assert_snapshot!(tree(
        "type M = `a${1}b`;\nfunction f() -> string {\n  for x in xs { x }\n  let t = ``;\n  t\n}\nfunction g() -> int { 1 }\n"
    ));
}

/// Nesting is bounded: past the limit the construct is reported and skipped
/// instead of exhausting the stack.
#[test]
fn nesting_is_bounded() {
    let n = 400;
    let text = format!(
        "function f() -> int {{\n  {}1{}\n}}\n",
        "(".repeat(n),
        ")".repeat(n)
    );
    let deep = parse(&text);
    assert!(
        deep.errors
            .iter()
            .any(|error| error.to_string().contains("nested more than")),
        "{:?}",
        deep.errors
    );
    let text = format!(
        "function f() -> int {{\n  let x = {}{};\n  1\n}}\n",
        "[".repeat(n),
        "]".repeat(n)
    );
    assert!(!parse(&text).errors.is_empty());
    let text = format!(
        "function f() -> int {{\n  {}1{}\n}}\n",
        "{".repeat(n),
        "}".repeat(n)
    );
    assert!(!parse(&text).errors.is_empty());
    let text = format!("type T = {}int{};\n", "(".repeat(n), ")".repeat(n));
    assert!(!parse(&text).errors.is_empty());
}

/// Malformed inputs that once panicked or swallowed the items after them.
#[test]
fn recovery_decides_after_parsing() {
    insta::assert_snapshot!(tree(
        "type V = int | ( < ) -> string throws never\ntype A = (]) -> int\n@@a({{ x }) function f() -> int { x }\n@@foo(1) class A { }\ntype K = ( , T as I).Key\n"
    ));
}

#[test]
fn unclosed_bodies_resync_at_the_next_item() {
    insta::assert_snapshot!(tree(
        "function f() -> int {\n\nclass Foo { a int }\nclient<llm> X {\n provider x\n\nretry_policy R {\n\nenum E { A }\n"
    ));
}

#[test]
fn test_runner_object_literal_is_not_the_body() {
    insta::assert_snapshot!(tree(
        "test \"x\" with R { t: 0.5 } {\n  assert.is_true(true)\n}\n"
    ));
}

#[test]
fn trailing_minus_at_end_of_file() {
    insta::assert_snapshot!(tree("let x = 1\n-"));
}

#[test]
fn unclosed_block_comment_is_reported_once() {
    insta::assert_snapshot!(tree(
        "class A { a int }\n/* never closed\nclass B { b int }\n"
    ));
}

#[test]
fn templates_are_structural() {
    insta::assert_snapshot!(tree(
        "function f(xs: int[]) -> string {\n  let s = `n=${xs.len()} ${for (let x in xs)}${x},${endfor} ${if xs.len() > 1}many${else if xs.len() == 1}one${else}none${endif} ${if (true) { \"y\" } else { \"n\" }} ${ let w = 1; w }`\n  tag`x${s}`\n}\n"
    ));
}

#[test]
fn template_tags_out_of_place_are_reported() {
    insta::assert_snapshot!(tree(
        "function f() -> string {\n  `${endfor} ${for let x in xs}a${endif} ${if x}b${else}c${else}d${endif} ${if y}e`\n}\nfunction g() -> string { ``unclosed\n}\n"
    ));
}

#[test]
fn docs_and_directives_are_fields_of_their_declaration() {
    use baml_syntax::nodes::{AnyDoc, Directive, DirectiveKind, FunctionDef};
    let text = "// file header\n\n/// Adds.\n// a plain comment is transparent\n/// Twice.\n//baml:mut_self\n/// baml:$trace=trace.span\nfunction f(x: int) -> int { x } // tail\n\nclass C {\n  /// The field.\n  x: int\n  @@dynamic\n}\n\n/// alias\ntype A = int;\n/// global\nlet g = 1;\n";
    let parse = parse(text);
    assert!(parse.errors.is_empty(), "{:?}", parse.errors);
    let t = &parse.tree;
    let f = t.get(t.arenas().ids::<FunctionDef>().next().expect("f"));
    let kinds: Vec<Option<DirectiveKind>> = t
        .list(f.docs)
        .iter()
        .map(|doc| match doc {
            AnyDoc::Directive(id) => Some(t.get(*id).kind),
            AnyDoc::Doc(_) | AnyDoc::Header(_) | AnyDoc::Bogus(_) => None,
        })
        .collect();
    assert_eq!(
        kinds,
        [
            None,
            None,
            Some(DirectiveKind::MutSelf),
            Some(DirectiveKind::Trace)
        ]
    );
    let trace = t.arenas().ids::<Directive>().last().expect("the hook");
    let hook = t.token_text(t.get(trace).token.idx);
    assert_eq!(
        DirectiveKind::trace_hook_value(hook).and_then(DirectiveKind::trace_hook_path),
        Some("trace.span")
    );
    insta::assert_snapshot!(tree(text));
}

#[test]
fn docs_that_document_nothing_are_errors() {
    // In a body, before a class-level attribute, and an unknown directive.
    insta::assert_snapshot!(tree(
        "function f() -> int {\n  /// not here\n  1\n}\nclass C {\n  /// orphan\n  @@dynamic\n  x: int\n}\n//baml:nope\nfunction g() -> int { 1 }\n/// baml:$trace=bad()\nfunction h() -> int { 1 }\n"
    ));
}

#[test]
fn string_escapes_are_reported_where_the_literal_is() {
    // An expression offers a raw string as the fix; a type, a pattern and an
    // attribute argument do not; byte strings and backtick text have their
    // own rules; a raw literal has no escapes.
    insta::assert_snapshot!(tree(
        "function f(x: \"a\\d\") -> int {\n  let s = \"\\d\";\n  let b = b\"\\u{41}\u{e9}\";\n  let t = `a\\q${x}b\\z`;\n  let r = r\"\\d\";\n  match (x) { \"\\d\" => 1, _ => 2 }\n}\nclass C { x: string @alias(\"\\d\") }\n"
    ));
    // An unclosed literal has its missing closer as its one diagnostic.
    let parse = parse("function f() -> string { \"\\d }");
    assert!(
        parse
            .errors
            .iter()
            .all(|error| !matches!(error, crate::ParseError::InvalidStringEscape { .. })),
        "{:?}",
        parse.errors
    );
}

#[test]
fn directives_belong_to_function_definitions() {
    // On a class, a field, an enum, a variant, an alias, an interface method
    // signature and a `let`, a directive is an error with the wording of the
    // rowan lowering's check; a second trace hook and a malformed one are
    // reported by function name, or without one when the name is missing.
    insta::assert_snapshot!(tree(
        "/// baml:$trace=hook\nclass C {\n  //baml:mut_self\n  x: int\n  /// baml:$trace=hook\n  function m(self) -> int { 1 }\n}\n//baml:trace_passthrough\nenum E {\n  //baml:vm\n  A\n}\n/// baml:$trace=bad()\ntype T = int;\ninterface I {\n  //baml:fallible\n  function s(self) -> int;\n}\n//baml:nope\nlet g = 1;\n/// baml:$trace=a\n/// baml:$trace=b\nfunction f() -> int { 1 }\n/// baml:$trace=bad()\nfunction g() -> int { 1 }\n/// baml:$trace=bad()\nfunction () -> int { 1 }\n"
    ));
}

#[test]
fn block_attributes_are_written_in_the_body() {
    // A `@@` before a declaration or before a member is not a prefix: at the
    // top level it is reported, in a body it is the enclosing declaration's.
    insta::assert_snapshot!(tree(
        "@@dynamic\nclass C {\n  @@alias(\"c\")\n  function f(self) -> int { 1 }\n  x: int\n}\n/// doc\n@@alias(\"d\")\nfunction g() -> int { 1 }\n"
    ));
}

#[test]
fn four_slashes_are_a_plain_comment() {
    let parse = parse("////////\nfunction f() -> int {\n  //// banner\n  1\n}\n");
    assert!(parse.errors.is_empty(), "{:?}", parse.errors);
    assert_eq!(
        parse
            .tree
            .arenas()
            .ids::<baml_syntax::nodes::DocLine>()
            .count(),
        0
    );
}

#[test]
fn member_block_attributes_take_arguments() {
    let member = parse("class C { x: int @@check(a, \"s\", {{ j }}) }\n");
    assert!(member.errors.is_empty(), "{:?}", member.errors);
    let attr = member
        .tree
        .arenas()
        .ids::<baml_syntax::nodes::BlockAttr>()
        .next()
        .expect("the attribute");
    let args = member.tree.get(attr).args.expect("arguments");
    assert_eq!(member.tree.list(member.tree.get(args).args).len(), 3);
}

#[test]
fn tree_index_links_parents_owners_and_covering_nodes() {
    use baml_syntax::{
        index::{TreeIndex, descendants},
        nodes::{BinaryExpr, FunctionDef, NodeKind},
        tree::AnyNodeId,
    };
    let text = "function f(x: int) -> int {\n  x + 1\n}\n";
    let parse = parse(text);
    assert!(parse.errors.is_empty(), "{:?}", parse.errors);
    let tree = &parse.tree;
    let index = TreeIndex::build(tree);
    assert!(index.is_for(tree));

    let root = tree.root_id();
    assert_eq!(index.parent(root), None);
    let f = AnyNodeId::of(tree.arenas().ids::<FunctionDef>().next().expect("f"));
    assert_eq!(index.parent(f), Some(root));
    let sum = AnyNodeId::of(tree.arenas().ids::<BinaryExpr>().next().expect("x + 1"));
    assert_eq!(index.ancestors(sum).last(), Some(root));
    assert!(index.ancestors(sum).any(|id| id == f));

    // The `+` belongs to the binary expression; the whitespace before it to no node.
    let plus = tree
        .tokens()
        .iter()
        .find(|&t| tree.token_text(t) == "+")
        .expect("+");
    assert_eq!(index.token_owner(plus), Some(sum));
    assert_eq!(index.token_owner(TokenIdx(plus.0 - 1)), None);

    let start = text.find("x + 1").expect("x + 1");
    let range = |s: usize, e: usize| {
        text_size::TextRange::new(
            text_size::TextSize::try_from(s).unwrap(),
            text_size::TextSize::try_from(e).unwrap(),
        )
    };
    assert_eq!(index.covering_node(tree, range(start, start + 5)), sum);
    // A range inside the `+`'s surrounding whitespace is covered by the sum.
    assert_eq!(index.covering_node(tree, range(start + 1, start + 1)), sum);
    // The whole text: the deepest node spanning its first and last owned
    // tokens is the one item, not the root (spans never include trivia).
    assert_eq!(index.covering_node(tree, range(0, text.len())), f);
    // Between `)` and `->`: the signature is the smallest node spanning both.
    let gap = text.find(") ->").expect(") ->") + 1;
    assert_eq!(
        Arenas::kind(index.covering_node(tree, range(gap, gap))),
        NodeKind::FunctionDef
    );

    let all: Vec<AnyNodeId> = descendants(tree, root).collect();
    assert_eq!(all[0], root);
    assert_eq!(all.len(), tree.arenas().all_ids().len());
    assert!(descendants(tree, sum).count() >= 3);
}
