use std::path::{Path, PathBuf};

use baml_base::SourceRootKind;
use baml_db::{ProjectDatabase, SourceRootSpec};

use crate::{FnKind, FunctionMetrics, Halstead, ProjectMetrics, TypeMetrics, project_metrics};

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

fn type_named<'a>(metrics: &'a ProjectMetrics, name: &str) -> &'a TypeMetrics {
    metrics
        .types
        .iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("no type {name}"))
}

/// `(cyclomatic, cognitive, max_nesting)` of the one function named `f`.
fn scores(source: &str) -> (u32, u32, u32) {
    let metrics = project(source);
    let f = function(&metrics, "f");
    (f.cyclomatic, f.cognitive, f.max_nesting)
}

fn npath(source: &str) -> u64 {
    function(&project(source), "f").npath
}

#[test]
fn straight_line() {
    assert_eq!(scores("function f(a: int) -> int { a + 1 }"), (1, 0, 0));
}

/// The `sumOfPrimes` example from the Sonar cognitive complexity white paper
/// (cyclomatic 4, cognitive 7), with `continue OUTER` spelled as a plain
/// `continue`.
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
    assert_eq!(scores(source), (4, 7, 3));
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
        let f = function(&metrics, name);
        assert!(f.calls.recursive, "{name}");
        assert_eq!(f.cognitive, 3, "{name}: if + else + recursion");
    }
    let plain = function(&metrics, "plain");
    assert!(!plain.calls.recursive);
    assert_eq!(plain.cognitive, 2);
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
    let calls = |name| function(&metrics, name).calls;
    assert_eq!((calls("leaf").fan_in, calls("leaf").fan_out), (2, 0));
    assert_eq!((calls("a").fan_in, calls("a").fan_out), (1, 1));
    assert_eq!((calls("b").fan_in, calls("b").fan_out), (0, 2));
    assert_eq!(
        (calls("c").fan_in, calls("c").fan_out),
        (0, 0),
        "self-calls are recursion, not fan"
    );
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
fn abc_counts() {
    let source = r#"
function g(x: int) -> int { x }
function f(flag: bool) -> int {
    let x = 1;
    x = g(x);
    if (flag) {
        x += 1;
    } else if (x > 2) {
        x = 0;
    }
    x
}
"#;
    let abc = function(&project(source), "f").abc;
    // `let x = 1`, `x = g(x)`, `x += 1`, `x = 0`.
    assert_eq!(abc.assignments, 4);
    assert_eq!(abc.branches, 1);
    // `flag` as a bare condition, the `else`, and `x > 2`.
    assert_eq!(abc.conditions, 3);
}

/// Tokens: `function f ( a : int ) -> int { a + 1 }`. Operators are
/// `function ( : -> { +` (closers do not count); operands are `f a int int a 1`.
#[test]
fn halstead_counts() {
    let f = function(&project("function f(a: int) -> int { a + 1 }"), "f").halstead;
    assert_eq!(
        f,
        Halstead {
            distinct_operators: 6,
            operators: 6,
            distinct_operands: 4,
            operands: 6,
        }
    );
}

#[test]
fn halstead_derived_measures() {
    let h = Halstead {
        distinct_operators: 6,
        operators: 6,
        distinct_operands: 4,
        operands: 6,
    };
    let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
    assert!(close(h.vocabulary(), 10.0));
    assert!(close(h.length(), 12.0));
    assert!(close(h.volume(), 12.0 * 10f64.log2()));
    assert!(close(h.difficulty(), 3.0 * 1.5));
    assert!(close(h.effort(), 4.5 * 12.0 * 10f64.log2()));
    assert!(close(Halstead::default().difficulty(), 0.0));
}

#[test]
fn string_literals_are_one_operand() {
    let f = function(
        &project(r#"function f() -> string { "two words" + "two words" }"#),
        "f",
    )
    .halstead;
    // `f`, `string`, then the literal twice.
    assert_eq!((f.distinct_operands, f.operands), (3, 4));
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
    let loc = function(&project(source), "f").loc;
    assert_eq!(
        (loc.sloc, loc.ploc, loc.cloc, loc.blank, loc.lloc),
        (6, 4, 2, 1, 2)
    );
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

#[test]
fn class_metrics() {
    let source = r#"
class Other {
    v: int
}

interface Named {
    function name(self) -> string;
}

class Account {
    balance: int
    owner: string
    other: Other

    function deposit(self, n: int) -> int { self.balance + n }
    function withdraw(self, n: int) -> int { if (n > 0) { self.balance - n } else { 0 } }
    function label(self) -> string { self.owner }
    function peek(self) -> int { self.other.v }

    implements Named {
        function name(self) -> string { self.label() }
    }
}
"#;
    let metrics = project(source);
    let account = type_named(&metrics, "Account");
    assert_eq!(account.fields, 3);
    assert_eq!(account.nom, 5);
    assert_eq!(account.wmc, 1 + 2 + 1 + 1 + 1);
    // Five own methods; `label` is the only callee, and it is one of them.
    assert_eq!(account.rfc, 5);
    // Couples to `Other` through `self.other.v`.
    assert_eq!(account.cbo, 1);
    // {deposit, withdraw} share `balance`; {label, name} share a call;
    // `peek` reads only `other`, which no other method touches.
    assert_eq!(account.lcom4, Some(3));
    assert_eq!(account.interfaces_implemented, 1);
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
    assert_eq!(plain.kind, FnKind::Llm);
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
    assert_eq!(branchy.calls.fan_out, 1);
    assert_eq!(function(&metrics, "shout").calls.fan_in, 1);
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
    let pair = function(&metrics, "Pair.eval");
    assert!(pair.calls.recursive);
    assert!(!function(&metrics, "Leaf.eval").calls.recursive);
    assert_eq!(
        function(&metrics, "Leaf.eval").calls.fan_in,
        2,
        "run and Pair.eval"
    );
}
