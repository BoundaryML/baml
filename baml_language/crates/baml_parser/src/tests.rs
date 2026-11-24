//! Declaration-layer tests: the tree shape and diagnostics for small inputs.
//! Bodies and expressions are `Bogus` until their layers land.

use std::fmt::Write as _;

use baml_syntax::token::TokenIdx;

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

/// A prefix operator binds above every binary operator: `-a * b` is
/// `(-a) * b`; its operand still takes the postfix forms (`-a.b()`).
#[test]
fn prefix_operators_bind_above_binary_operators() {
    insta::assert_snapshot!(tree(
        "function f(a: int, b: int) -> int { -a * b + -a.b() }"
    ));
}

/// BAML has no `++`/`--`: `--x` is two negations and `++x` is an error at
/// its first `+`.
#[test]
fn there_are_no_increment_operators() {
    insta::assert_snapshot!(tree(
        "function f(x: int) -> int {\n  let a = --x;\n  let b = ++x;\n  a\n}\n"
    ));
}

/// Headers where no statement can stand never panic: a `let` with nothing
/// after it, a header before the `}` that ends a body, a signature whose
/// `->` a comment split (from the formatter fuzz corpus).
#[test]
fn header_comment_recovery_never_panics() {
    insta::assert_snapshot!(tree("let//#\n}"));
    insta::assert_snapshot!(tree("let//#\n   "));
    insta::assert_snapshot!(tree(
        "function classify(n: int) -/*C1*/> string {\n    //# statements\n    let label = match (n) {\n        0 => \"zero\",\n        _ => \"big\",\n    };\n    //# result\n    label\n}\n"
    ));
}

/// A `-` opening a line continues the expression before it unless it is
/// glued to its operand: blanks, trailing blanks, a comment or a line break
/// after it all subtract; `-1` on a line of its own negates.
#[test]
fn a_line_opening_minus_subtracts_unless_glued() {
    for (name, body) in [
        ("blank", "a\n  - 1"),
        ("line_break", "a\n  -\n  1"),
        ("trailing_blank", "a\n  - \n  1"),
        ("comment", "a\n  -//c\n  1"),
        ("glued", "a\n  -1"),
    ] {
        insta::assert_snapshot!(
            format!("line_opening_minus_{name}"),
            tree(&format!("function f(a: int) -> int {{\n  {body}\n}}\n"))
        );
    }
}

/// Both `;` of a C-style `for` header are required.
#[test]
fn a_c_style_for_header_demands_both_semicolons() {
    insta::assert_snapshot!(tree(
        "function f() -> int {\n  for (let i = 0 i < 3; i += 1) {}\n  for (let i = 0; i < 3 i += 1) {}\n  for (;;) {}\n  0\n}\n"
    ));
}

/// A `<` inside a parenthesised argument is a comparison: the generic
/// look-ahead skips bracket groups whole and stops at an unmatched closer.
#[test]
fn a_comparison_inside_a_parenthesised_argument_is_not_a_generic() {
    insta::assert_snapshot!(tree(
        "function f(a: int, b: int, c: int, d: int) -> bool {\n  g((a < b), (c > (d)))\n}\n"
    ));
    insta::assert_snapshot!(tree(
        "function f() -> int {\n  h<(a: int) -> int>(x) + k<int[]>()\n}\n"
    ));
}

/// A quoted key in a constructed object is reported where it stands; the
/// constructor is not left behind as a bare path.
#[test]
fn a_quoted_object_key_is_an_error_at_the_key() {
    insta::assert_snapshot!(tree(
        "function f() -> Foo {\n  let s = Foo { \"a\": 1, b: 2, `c`: 3 };\n  s\n}\n"
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
    // A block entered at the limit: its first statement is reported and
    // skipped, whatever exact level the block itself landed on (no panic at
    // any depth around the limit; past it, an error).
    for n in 120..=136 {
        let text = format!(
            "function f() -> int {{\n  {}{{ x }}{}\n}}\n",
            "g(".repeat(n),
            ")".repeat(n)
        );
        let parsed = parse(&text);
        assert!(n < 130 || !parsed.errors.is_empty(), "{n}");
    }
    // Lambdas nest about four levels each; template interpolations one.
    let text = format!(
        "function f() -> int {{\n  {}1{}\n}}\n",
        "g((x) -> {{ let y = g(".repeat(40),
        "); y }})".repeat(40)
    );
    assert!(!parse(&text).errors.is_empty());
    let text = format!(
        "function f() -> string {{\n  {}1{}\n}}\n",
        "`${{".repeat(140),
        "}}`".repeat(140)
    );
    assert!(!parse(&text).errors.is_empty());
    // A prefix operator or an `await` nests one level per operator.
    for prefix in ["-", "!", "await "] {
        let text = format!(
            "function f() -> int {{\n  {}x\n}}\n",
            prefix.repeat(100_000)
        );
        assert!(!parse(&text).errors.is_empty(), "{prefix:?}");
    }
    // A flat assignment chain is not nesting: one error, no stack per link.
    let text = format!(
        "function f() -> int {{\n  x {}= 1;\n  1\n}}\n",
        "= x ".repeat(20_000)
    );
    let chain = parse(&text);
    assert_eq!(
        chain.errors.len(),
        1,
        "{:?}",
        &chain.errors[..chain.errors.len().min(3)]
    );
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
        index.covering_node(tree, range(gap, gap)).kind(),
        NodeKind::FunctionDef
    );

    let all: Vec<AnyNodeId> = descendants(tree, root).collect();
    assert_eq!(all[0], root);
    assert_eq!(all.len(), tree.arenas().all_ids().len());
    assert!(descendants(tree, sum).count() >= 3);
}

/// `value.upcast<Interface>` is the interface view. The removed
/// `value.as<Interface>` spelling is E0098 once per use, at its `as`, naming
/// the replacement with the written type arguments; a base that is a path
/// does not absorb `.as` as a segment first, and the view is still a node.
/// Without a `<`, `upcast` and `as` are plain member names.
#[test]
fn the_removed_as_spelling_of_upcast_names_its_replacement() {
    use crate::ParseError;

    let accepted = parse(
        "function f(p: P, q: Q) -> int {\n  p.upcast<Named>.name;\n  self.upcast<I>.m();\n  g().upcast<Boxed<int>>;\n  q.upcast + q.upcast() + q.as + q.as()\n}\n",
    );
    assert_eq!(accepted.errors, [], "the new spelling parses clean");
    let tree = accepted.tree.debug_tree();
    assert_eq!(tree.matches("UpcastExpr").count(), 3, "{tree}");

    for (source, replacement) in [
        ("p.as<Named>.name", "`.upcast<Named>`"),
        ("self.as<I>.m()", "`.upcast<I>`"),
        ("a.b.as<Boxed<int>>", "`.upcast<Boxed<int>>`"),
        (
            "g().as<Codec<Text, Output = string>>.decode(x)",
            "`.upcast<Codec<Text, Output = string>>`",
        ),
    ] {
        let text = format!("function f() -> int {{\n  {source}\n}}\n");
        let removed = parse(&text);
        let [ParseError::RemovedFeature { message, range }] = removed.errors.as_slice() else {
            panic!(
                "expected one removed-feature error for `{source}`, got {:#?}",
                removed.errors
            );
        };
        assert_eq!(
            &text[*range], "as",
            "the report points at `as` in `{source}`"
        );
        assert!(
            message.contains(&format!("write {replacement} instead")),
            "`{source}`: {message}"
        );
        // The view stays a node, so its base and target are still checked.
        assert_eq!(
            removed.tree.debug_tree().matches("UpcastExpr").count(),
            1,
            "`{source}`"
        );
    }
}
