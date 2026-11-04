//! Declaration-layer tests: the tree shape and diagnostics for small inputs.
//! Bodies and expressions are `Bogus` until their layers land.

use std::fmt::Write as _;

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
