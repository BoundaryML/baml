use std::path::{Path, PathBuf};

use baml_base::SourceRootKind;
use baml_db::{ProjectDatabase, SourceRootSpec};

use crate::{FunctionMetrics, ProjectMetrics, project_metrics};

fn project(source: &str) -> ProjectMetrics {
    let mut db = ProjectDatabase::new();
    db.ensure_stdlib_sources();
    let root = db
        .add_source_root(SourceRootSpec::new(
            PathBuf::from("/metrics-test"),
            SourceRootKind::Workspace,
        ))
        .unwrap_or_else(|e| unreachable!("fresh database accepts one workspace root: {e}"));
    db.add_or_update_file_in(root, Path::new("/metrics-test/main.baml"), source);
    let files = root.files(&db).clone();
    project_metrics(&db, &files)
}

fn function<'a>(metrics: &'a ProjectMetrics, name: &str) -> &'a FunctionMetrics {
    metrics
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no function {name}"))
}

/// `(cyclomatic, cognitive, nesting)` of the one function named `f`.
fn scores(source: &str) -> (u32, u32, u32) {
    let metrics = project(source);
    let f = function(&metrics, "f");
    (f.cyclomatic, f.cognitive, f.nesting)
}

fn npath(source: &str) -> u64 {
    function(&project(source), "f").npath
}

#[test]
fn straight_line() {
    assert_eq!(scores("function f(a: int) -> int { a + 1 }"), (1, 0, 0));
}

/// The `sumOfPrimes` example from the Sonar cognitive complexity white paper
/// (cyclomatic 4, cognitive 7). BAML has no labeled `continue`, and the spec
/// only scores jumps to a label, so the plain `continue` here leaves 6.
#[test]
fn nested_loops_increment_by_depth() {
    let source = r#"
function f(xs: int[]) -> int {
    let total = 0;
    for (let x in xs) {
        for (let y in xs) {
            if (x % y == 0) {
                continue;
            }
        }
        total += x;
    }
    total
}
"#;
    assert_eq!(scores(source), (4, 6, 3));
}

#[test]
fn else_if_chain_is_flat() {
    let source =
        "function f(a: bool, b: bool) -> int { if (a) { 1 } else if (b) { 2 } else { 3 } }";
    assert_eq!(scores(source), (3, 3, 1));
}

/// The `getWords` example from the white paper: a `match` costs one
/// cognitive point however many arms it has.
#[test]
fn match_counts_once() {
    let source = r#"
function f(n: int) -> string {
    match (n) {
        1 => "one",
        2 => "a couple",
        3 => "a few",
        _ => "lots",
    }
}
"#;
    assert_eq!(scores(source), (4, 1, 1));
}

#[test]
fn boolean_runs() {
    assert_eq!(
        scores("function f(a: bool, b: bool, c: bool) -> bool { a && b && c }"),
        (3, 1, 0)
    );
    assert_eq!(
        scores("function f(a: bool, b: bool, c: bool) -> bool { a && b || c }"),
        (3, 2, 0)
    );
}

#[test]
fn lambda_body_is_nested() {
    let source = r#"
function f() -> int {
    let g = (x: int) -> int {
        if (x > 0) { 1 } else { 2 }
    };
    g(1)
}
"#;
    assert_eq!(scores(source), (2, 3, 2));
}

#[test]
fn optional_access_and_coalesce_are_cyclomatic_only() {
    let source = r#"
class A {
    b: A?
}
function f(a: A?) -> A? { a?.b ?? null }
"#;
    assert_eq!(scores(source), (3, 0, 0));
}

#[test]
fn template_control_flow() {
    let source = r#"
function f(xs: string[], flag: bool) -> string {
    `
    ${if (flag)} yes ${else} no ${endif}
    ${for (let x in xs)} ${x} ${endfor}
    `
}
"#;
    assert_eq!(scores(source), (3, 3, 1));
}

/// Sonar's rule: every function in a recursion cycle, direct or mutual,
/// earns one cognitive increment.
#[test]
fn recursion_is_counted() {
    let source = r#"
function fact(n: int) -> int { if (n <= 1) { 1 } else { n * fact(n - 1) } }
function is_even(n: int) -> bool { if (n == 0) { true } else { is_odd(n - 1) } }
function is_odd(n: int) -> bool { if (n == 0) { false } else { is_even(n - 1) } }
function plain(n: int) -> int { if (n == 0) { 1 } else { fact(n) } }
"#;
    let metrics = project(source);
    for name in ["fact", "is_even", "is_odd"] {
        assert_eq!(
            function(&metrics, name).cognitive,
            3,
            "{name}: if + else + recursion"
        );
    }
    assert_eq!(function(&metrics, "plain").cognitive, 2);
}

#[test]
fn fan_in_and_fan_out_count_distinct_partners() {
    let source = r#"
function leaf(x: int) -> int { x }
function a(x: int) -> int { leaf(leaf(x)) }
function b(x: int) -> int { leaf(x) + a(x) }
function c(x: int) -> int { if (x == 0) { 0 } else { c(x - 1) } }
"#;
    let metrics = project(source);
    let fans = |name| {
        let f = function(&metrics, name);
        (f.fan_in, f.fan_out)
    };
    assert_eq!(fans("leaf"), (2, 0));
    assert_eq!(fans("a"), (1, 1));
    assert_eq!(fans("b"), (0, 2));
    assert_eq!(fans("c"), (0, 0), "self-calls are recursion, not fan");
}

#[test]
fn npath_sequential_branches_multiply() {
    let source = r#"
function f(a: bool, b: bool) -> int {
    let x = 0;
    if (a) { x = 1; }
    if (b) { x = 2; }
    x
}
"#;
    assert_eq!(npath(source), 4);
}

#[test]
fn npath_adds_boolean_operators_to_conditions() {
    assert_eq!(
        npath("function f(a: bool, b: bool) -> int { if (a && b) { 1 } else { 2 } }"),
        3
    );
}

#[test]
fn npath_match_sums_arms_and_loops_add_one() {
    let source = r#"
function f(n: int, xs: int[]) -> int {
    let y = match (n) {
        1 => 1,
        2 => 2,
        _ => 3,
    };
    for (let x in xs) {
        y += x;
    }
    y
}
"#;
    assert_eq!(npath(source), 3 * 2);
}

#[test]
fn exits_count_returns_and_throws_outside_lambdas() {
    let source = r#"
function f(n: int) -> int {
    let g = (x: int) -> int { return x; };
    if (n < 0) {
        throw "negative";
    }
    if (n == 0) {
        return 0;
    }
    g(n)
}
"#;
    assert_eq!(function(&project(source), "f").exits, 3);
}

#[test]
fn line_counts() {
    let source = r#"
function f() -> int {
    // comment

    let x = 1; // trailing
    x
}
"#;
    assert_eq!(function(&project(source), "f").lines, 4);
}

#[test]
fn file_lines_split_out_tests() {
    let source = r#"
function f() -> int {
    1
}

// comment
test "one" {
    assert.equal(f(), 1);
}

testset "more" {
    test "two" {
        assert.equal(f(), 1);
    }
}
"#;
    let file = &project(source).files[0];
    assert_eq!((file.lines, file.tests, file.test_lines), (3, 2, 8));
}

#[test]
fn methods_are_qualified() {
    let source = r#"
class A {
    function m(self) -> int { 1 }
}
"#;
    assert!(project(source).functions.iter().any(|f| f.name == "A.m"));
}

/// An LLM function is measured by its prompt alone: the runner code its
/// `@spec` body wraps the prompt in, and the injected `client` / `on_event`
/// parameters, are not the author's. Calls in the prompt are its fan-out.
#[test]
fn llm_function_measures_prompt() {
    let source = r#"
client<llm> GPT4 {
    provider openai
    options {
        model "gpt-4o"
    }
}

function shout(x: string) -> string { x }

function Plain(x: string) -> string {
    client: GPT4
    prompt: `Say ${x}`
}

function Branchy(xs: string[], flag: bool) -> string {
    client: GPT4
    prompt: `
        ${if (flag)} yes ${else} no ${endif}
        ${for (let x in xs)} ${shout(x)} ${endfor}
    `
}
"#;
    let metrics = project(source);
    let plain = function(&metrics, "Plain");
    let branchy = function(&metrics, "Branchy");
    assert_eq!(
        (plain.params, plain.cyclomatic, plain.cognitive, plain.npath),
        (1, 1, 0, 1)
    );
    assert_eq!(
        (
            branchy.params,
            branchy.cyclomatic,
            branchy.cognitive,
            branchy.npath
        ),
        (2, 3, 3, 4)
    );
    assert_eq!(branchy.fan_out, 1);
    assert_eq!(function(&metrics, "shout").fan_in, 1);
}

/// Calls inside a lambda in a prompt count, as they do in a function body.
#[test]
fn prompt_lambda_calls_count() {
    let source = r#"
client<llm> GPT4 {
    provider openai
    options {
        model "gpt-4o"
    }
}

function shout(x: string) -> string { x }

function Mapped(xs: string[]) -> string {
    client: GPT4
    prompt: `${xs.map((x) -> { shout(x) }).join(", ")}`
}
"#;
    let metrics = project(source);
    assert_eq!(function(&metrics, "shout").fan_in, 1);
}

/// A virtual call reaches every implementation (class hierarchy analysis), so
/// a tree evaluated through an interface is recursive and its implementations
/// have callers.
#[test]
fn virtual_calls_link_to_implementations() {
    let source = r#"
interface Node {
    function eval(self) -> int;
}

class Leaf {
    v: int

    implements Node {
        function eval(self) -> int { self.v }
    }
}

class Pair {
    left: Node
    right: Node

    implements Node {
        function eval(self) -> int { self.left.eval() + self.right.eval() }
    }
}

function run(n: Node) -> int { n.eval() }
"#;
    let metrics = project(source);
    // The recursion point is the only cognitive increment in either.
    assert_eq!(function(&metrics, "Pair.eval").cognitive, 1);
    assert_eq!(function(&metrics, "Leaf.eval").cognitive, 0);
    assert_eq!(
        function(&metrics, "Leaf.eval").fan_in,
        2,
        "run and Pair.eval"
    );
}
