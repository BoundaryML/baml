//! Every case runs on the interpreter and as a native executable, and the two
//! must end the same way: the same printed result, or the same uncaught
//! panic. The native executable then runs again collecting on every
//! allocation, which must change nothing.
//!
//! Operands come out of loops (`count`, `half`, `pow2`) so that the
//! optimizer cannot fold a case into a constant and the generated arithmetic
//! is what runs.

use std::{process::Command, sync::Arc};

use baml_compiler2_native::{CompileOptions, compile, link};
use baml_db::{
    CheckedWorld, ProjectDatabase,
    baml_compiler2_hir::item_data::{file_functions, function_data},
    testing::{OptLevel, assert_no_diagnostic_errors, setup_test_db},
};
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
function count(n: int) -> int {
  let x = 0;
  for (let i = 0; i < n; i += 1) { x += 1; };
  x
}
function half(n: int) -> float {
  let x = 0.0;
  for (let i = 0; i < n; i += 1) { x += 0.5; };
  x
}
function pow2(n: int) -> int {
  let x = 1;
  for (let i = 0; i < n; i += 1) { x = x * 2; };
  x
}
function int_max() -> int { let p = pow2(62); (p - 1) + p }
function int_min() -> int { -int_max() - 1 }
function fib(n: int) -> int { if n <= 1 { n } else { fib(n - 1) + fib(n - 2) } }
function pick(x: int) -> int {
  match (x) { 0 => 10, 1 => 20, 7 => 30, _ => 40 }
}
function mix(a: int, b: float, flag: bool) -> float { if flag { a + b } else { b } }

function sum_of_squares() -> int {
  let s = 0;
  for (let i = 0; i < 1000; i += 1) { s += i * i; };
  s
}
function recursion() -> int { fib(20) }
function division_truncates() -> int { -count(7) / count(2) }
function division_by_minus_one() -> int { count(7) / -count(1) }
function remainder_takes_dividend_sign() -> int { -count(7) % count(3) }
function bit_operators() -> int { (count(12) & count(10)) | (count(1) ^ count(3)) }
function product() -> int { pow2(31) * pow2(30) }
function shift_left() -> int { count(3) << count(4) }
function shift_left_past_width() -> int { count(1) << count(64) }
function shift_right_past_width() -> int { -count(8) >> count(70) }
function shift_right() -> int { -count(9) >> count(1) }
function match_on_int() -> int {
  pick(count(0)) + pick(count(1)) * 100 + pick(count(7)) * 10000 + pick(count(9)) * 1000000
}
function mixed_arguments() -> float { mix(count(3), half(1), count(1) == 1) }
function divide_by_zero() -> int { count(5) / count(0) }
function remainder_by_zero() -> int { count(5) % count(0) }
function negative_shift() -> int { count(1) << -count(1) }

function largest_int() -> int { int_max() }
function int_extremes() -> int { int_max() + int_min() }
function shift_into_sign() -> int { count(1) << count(63) }
function add_wraps() -> int { int_max() + count(1) }
function increment_wraps() -> int { int_max() + 1 }
function subtract_wraps() -> int { int_min() - count(1) }
function multiply_wraps() -> int { pow2(32) * pow2(31) }
function multiply_wraps_to_zero() -> int { pow2(40) * pow2(40) }
function negate_wraps() -> int { -int_min() }
function smallest_divided_by_minus_one() -> int { int_min() / -count(1) }
function smallest_remainder_by_minus_one() -> int { int_min() % -count(1) }

function int_times_float() -> float { count(3) * 1.5 + 2 }
function float_shortest_digits() -> float { half(1) / 5.0 + half(1) / 2.5 }
function float_without_exponent() -> float { half(2) * 10000000000000000000000.0 }
function float_small() -> float { half(2) / 3.0 * 0.0000001 }
function float_divide_by_zero() -> float { -half(2) / half(0) }
function float_nan() -> float { half(0) / half(0) }
function float_remainder() -> float { -half(15) % half(4) }
function nan_orders_above_numbers() -> bool {
  let nan = half(0) / half(0);
  let one = half(2);
  nan == nan && nan > one && one < nan && nan >= nan && nan <= nan
    && !(nan < one) && !(one > nan) && nan != one && one <= nan && !(one >= nan)
}
function negative_zero_equals_zero() -> bool { -half(0) == half(0) }

function or_skips_right_side() -> bool { count(3) > count(2) || count(1) / count(0) == 1 }
function and_skips_right_side() -> bool { count(1) > count(2) && count(1) / count(0) == 1 }
function not_of_bool() -> bool { !(count(1) == count(1)) }
function returns_null() -> null { let ignored = count(3); null }

class Point {
  x: int
  y: int
}
class Line {
  from: Point
  to: Point
  label: string
}
class Node {
  value: int
  next: Node?
}
class Tree {
  left: Tree?
  right: Tree?
}
class Mixed {
  ratio: float
  on: bool
  name: string
  items: int[]
}

function instance_fields() -> int {
  let p = Point { x: count(3), y: count(4) };
  p.x * 10 + p.y
}
function field_writes() -> int {
  let p = Point { x: count(1), y: count(2) };
  p.x = count(5);
  p.y = p.x + p.y;
  p.x * 10 + p.y
}
function nested_fields() -> int {
  let l = Line { from: Point { x: count(1), y: 2 }, to: Point { x: 3, y: count(4) }, label: "ab" };
  l.to.x = l.from.y + 10;
  l.from.x + l.to.x * 10 + l.to.y * 100 + l.label.length() * 1000
}
function fields_of_every_kind() -> float {
  let m = Mixed { ratio: half(3), on: count(1) == 1, name: "n", items: [count(4)] };
  m.items.push(6);
  if m.on && m.name == "n" { m.ratio + m.items[0] + m.items[1] } else { 0.0 }
}
function build_list(n: int) -> Node? {
  let head: Node? = null;
  for (let i = 0; i < n; i += 1) { head = Node { value: i, next: head }; };
  head
}
function sum_list(node: Node?) -> int {
  let at = node;
  if (at == null) { return 0; }
  at.value + sum_list(at.next)
}
function linked_list() -> int { sum_list(build_list(count(100))) }
function make_tree(depth: int) -> Tree {
  if (depth <= 0) { return Tree { left: null, right: null }; }
  Tree { left: make_tree(depth - 1), right: make_tree(depth - 1) }
}
function count_tree(node: Tree) -> int {
  let l = node.left;
  let r = node.right;
  if (l == null || r == null) { return 1; }
  1 + count_tree(l) + count_tree(r)
}
function small_trees() -> int {
  let total = 0;
  for (let i = 0; i < 20; i += 1) { total += count_tree(make_tree(4)); };
  total
}
function garbage_under_a_live_tree() -> int {
  let keep = make_tree(14);
  let total = 0;
  for (let i = 0; i < 40; i += 1) { total += count_tree(make_tree(13)); };
  total + count_tree(keep)
}
function growing_live_heap() -> int {
  let all: Point[] = [];
  for (let i = 0; i < 300000; i += 1) { all.push(Point { x: i, y: 1 }); };
  let s = 0;
  for (let p in all) { s += p.y; };
  s + all[299999].x
}

class Reading {
  value: int?
  scale: float?
  ok: bool?
}
function maybe(n: int) -> int? { if n > 2 { n } else { null } }
function optional_ints() -> int {
  let a = maybe(count(5));
  let b = maybe(count(1));
  (a ?? 100) + (b ?? 1000)
}
function optional_null_tests() -> bool {
  let a = maybe(count(5));
  let b = maybe(count(1));
  a != null && b == null
}
function optional_fields() -> float {
  let r = Reading { value: maybe(count(7)), scale: null, ok: count(1) == 1 };
  r.scale = half(3);
  let v = r.value ?? 0;
  let s = r.scale ?? 0.0;
  let ok = r.ok ?? false;
  if ok { v + s } else { 0.0 }
}
function optional_narrowing() -> int {
  let a = maybe(count(9));
  if (a == null) { return 0; }
  a + 1
}
function coalesce_reference() -> int {
  let n = build_list(count(0));
  let m = n ?? Node { value: 42, next: null };
  m.value
}

function array_literal() -> int {
  let a = [count(1), 2, count(3)];
  a[0] + a[1] * 10 + a[2] * 100
}
function array_writes() -> int {
  let a = [1, 2, 3];
  a[1] = count(7);
  a[2] = a[1] + a[0];
  a[0] + a[1] * 10 + a[2] * 100
}
function array_push_and_length() -> int {
  let a: int[] = [];
  for (let i = 0; i < 100; i += 1) { a.push(i * i); };
  a.length() * 1000000 + a[99] + a[50]
}
function array_for_in() -> int {
  let a: int[] = [];
  for (let i = 0; i < 10; i += 1) { a.push(i); };
  let s = 0;
  for (let x in a) { s += x; };
  s
}
function for_in_sees_appends() -> int {
  let a = [1, 2, 3];
  let n = 0;
  for (let x in a) {
    if a.length() < 6 { a.push(x); };
    n += x;
  };
  n
}
function array_of_instances() -> int {
  let ps: Point[] = [];
  for (let i = 0; i < 10; i += 1) { ps.push(Point { x: i, y: i * 2 }); };
  let s = 0;
  for (let p in ps) { s += p.x + p.y; };
  ps[3].x = 100;
  s + ps[3].x
}
function array_of_strings() -> int {
  let ws = ["a", "bc", "def"];
  let tail = "gh";
  for (let i = 0; i < 1; i += 1) { tail = tail + "ij"; };
  ws.push(tail);
  let n = 0;
  for (let w in ws) { n = n * 10 + w.length(); };
  n
}
function nested_arrays() -> int {
  let g: int[][] = [];
  for (let i = 0; i < 3; i += 1) {
    let row: int[] = [];
    for (let j = 0; j < 3; j += 1) { row.push(i * 3 + j); };
    g.push(row);
  };
  g[2][1] + g[1][2] * 10
}
function array_of_floats() -> float {
  let a = [half(1), half(3)];
  a.push(half(4));
  a[0] + a[1] + a[2]
}
function array_of_bools() -> bool {
  let a = [count(1) == 1, count(1) == 2];
  a.push(true);
  a[0] && !a[1] && a[2]
}
function index_past_end() -> int { let a = [1, 2, 3]; a[count(5)] }
function index_from_end() -> int { let a = [1, 2, 3]; a[-count(1)] * 10 + a[-count(3)] }
function index_before_start() -> int { let a = [1, 2, 3]; a[-count(4)] }
function index_from_end_written() -> int {
  let a = [1, 2, 3];
  a[-count(1)] = 7;
  a[2]
}
function index_of_empty() -> int { let a: int[] = []; a[count(0)] }

function string_concat_loop() -> int {
  let s = "";
  for (let i = 0; i < 50; i += 1) { s = s + "xy"; };
  s.length()
}
function string_result() -> string {
  let s = "q";
  for (let i = 0; i < 2; i += 1) { s = s + "\"z"; };
  s
}
function many(depth: int, a: Node, b: int, c: Node, d: int, e: Node, f: int, g: Node, h: int, i: Node, j: int, k: Node, l: Node) -> int {
  if depth > 0 {
    return many(depth - 1, a, b, c, d, e, f, g, h, i, j, k, l);
  };
  let fresh = Node { value: b + d + f + h + j, next: l };
  a.value + c.value + e.value + g.value + i.value + k.value + fresh.value + l.value
}
function arguments_on_the_stack() -> int {
  let total = 0;
  for (let n = 0; n < 20; n += 1) {
    let x = Node { value: n, next: null };
    total += many(3, x, 1, Node { value: 2, next: null }, 3, x, 4, Node { value: 5, next: null }, 6, x, 7, x, Node { value: 8, next: x });
  };
  total
}
function string_length_counts_characters() -> int {
  let s = "é";
  for (let i = 0; i < 3; i += 1) { s = s + "日本"; };
  s.length()
}
function string_equality() -> bool {
  let a = "ab";
  let b = "a";
  for (let i = 0; i < 1; i += 1) { b = b + "b"; };
  a == b && !(a != b) && a != "ac"
}
function string_ordering() -> bool {
  let a = "abc";
  let b = "ab";
  for (let i = 0; i < 1; i += 1) { b = b + "d"; };
  a < b && b > a && a <= a && !(b <= a) && "ab" < a && a >= "abc"
}
function string_of_three_parts() -> string {
  let n = "x";
  for (let i = 0; i < 1; i += 1) { n = n + "y"; };
  n + "-" + n + "!"
}
function int_to_string() -> string { count(42).to_string() + "/" + (-count(7)).to_string() }
function template_holes() -> string {
  let n = count(3);
  let ok = n == 3;
  let name = "n";
  `${name}=${n} ok=${ok} half=${half(1)} inf=${half(1) / half(0)}`
}
function strings_survive_collection() -> int {
  let words: string[] = [];
  for (let i = 0; i < 200; i += 1) {
    let w = "w";
    for (let j = 0; j < i % 5; j += 1) { w = w + "o"; };
    words.push(w);
  };
  let n = 0;
  for (let w in words) { n += w.length(); };
  n
}
"#;

/// How a run ended. `baml run` prints a result on stdout and exits zero, or
/// reports an uncaught panic on stderr and exits one.
#[derive(Debug, PartialEq, Eq)]
enum Outcome<'a> {
    Printed(&'a str),
    Panicked(&'a str),
}
use Outcome::{Panicked, Printed};

struct Case {
    function: &'static str,
    /// How native code must end.
    expected: Outcome<'static>,
    /// Whether the interpreter must end the same way. It does not where the
    /// answer depends on `int` being 64 bits wide and wrapping: the
    /// interpreter's `int` is still 63 bits wide and panics on overflow.
    interpreter_agrees: bool,
    /// Whether the case is small enough to run collecting on every
    /// allocation.
    stressed: bool,
}

const fn case(function: &'static str, expected: Outcome<'static>) -> Case {
    Case {
        function,
        expected,
        interpreter_agrees: true,
        stressed: true,
    }
}

/// A case that allocates enough to collect and to grow the heap on its own.
const fn heavy(function: &'static str, expected: Outcome<'static>) -> Case {
    Case {
        function,
        expected,
        interpreter_agrees: true,
        stressed: false,
    }
}

/// A case the interpreter is expected to diverge on until its `int` wraps
/// at 64 bits too.
const fn wide(function: &'static str, expected: Outcome<'static>) -> Case {
    Case {
        function,
        expected,
        interpreter_agrees: false,
        stressed: true,
    }
}

const CASES: &[Case] = &[
    case("sum_of_squares", Printed("332833500")),
    case("recursion", Printed("6765")),
    case("division_truncates", Printed("-3")),
    case("division_by_minus_one", Printed("-7")),
    case("remainder_takes_dividend_sign", Printed("-1")),
    case("bit_operators", Printed("10")),
    case("product", Printed("2305843009213693952")),
    case("shift_left", Printed("48")),
    case("shift_left_past_width", Printed("0")),
    case("shift_right_past_width", Printed("-1")),
    case("shift_right", Printed("-5")),
    case("match_on_int", Printed("40302010")),
    case("mixed_arguments", Printed("3.5")),
    case(
        "divide_by_zero",
        Panicked("baml.panics.DivisionByZero {dividend: 5}"),
    ),
    case(
        "remainder_by_zero",
        Panicked("baml.panics.DivisionByZero {dividend: 5}"),
    ),
    case(
        "negative_shift",
        Panicked("baml.panics.NegativeBitShift {message: \"bit shift count is negative: -1\"}"),
    ),
    wide("largest_int", Printed("9223372036854775807")),
    wide("int_extremes", Printed("-1")),
    wide("shift_into_sign", Printed("-9223372036854775808")),
    wide("add_wraps", Printed("-9223372036854775808")),
    wide("increment_wraps", Printed("-9223372036854775808")),
    wide("subtract_wraps", Printed("9223372036854775807")),
    wide("multiply_wraps", Printed("-9223372036854775808")),
    wide("multiply_wraps_to_zero", Printed("0")),
    wide("negate_wraps", Printed("-9223372036854775808")),
    wide(
        "smallest_divided_by_minus_one",
        Printed("-9223372036854775808"),
    ),
    wide("smallest_remainder_by_minus_one", Printed("0")),
    case("int_times_float", Printed("6.5")),
    case("float_shortest_digits", Printed("0.30000000000000004")),
    case(
        "float_without_exponent",
        Printed("10000000000000000000000.0"),
    ),
    case("float_small", Printed("0.00000003333333333333333")),
    case("float_divide_by_zero", Printed("-inf")),
    case("float_nan", Printed("NaN")),
    case("float_remainder", Printed("-1.5")),
    case("nan_orders_above_numbers", Printed("true")),
    case("negative_zero_equals_zero", Printed("true")),
    case("or_skips_right_side", Printed("true")),
    case("and_skips_right_side", Printed("false")),
    case("not_of_bool", Printed("false")),
    case("returns_null", Printed("")),
    case("instance_fields", Printed("34")),
    case("field_writes", Printed("57")),
    case("nested_fields", Printed("2521")),
    case("fields_of_every_kind", Printed("11.5")),
    case("linked_list", Printed("4950")),
    case("small_trees", Printed("620")),
    // More arguments than registers: the collector finds a caller's frame
    // from its callee's whatever the arguments take on the stack.
    case("arguments_on_the_stack", Printed("1480")),
    heavy("garbage_under_a_live_tree", Printed("688087")),
    heavy("growing_live_heap", Printed("599999")),
    case("optional_ints", Printed("1005")),
    case("optional_null_tests", Printed("true")),
    case("optional_fields", Printed("8.5")),
    case("optional_narrowing", Printed("10")),
    case("coalesce_reference", Printed("42")),
    case("array_literal", Printed("321")),
    case("array_writes", Printed("871")),
    case("array_push_and_length", Printed("100012301")),
    case("array_for_in", Printed("45")),
    case("for_in_sees_appends", Printed("12")),
    case("array_of_instances", Printed("235")),
    case("array_of_strings", Printed("1234")),
    case("nested_arrays", Printed("57")),
    case("array_of_floats", Printed("4.0")),
    case("array_of_bools", Printed("true")),
    case(
        "index_past_end",
        Panicked("baml.panics.IndexOutOfBounds {index: 5, length: 3}"),
    ),
    case("index_from_end", Printed("31")),
    case(
        "index_before_start",
        Panicked("baml.panics.IndexOutOfBounds {index: -4, length: 3}"),
    ),
    case("index_from_end_written", Printed("7")),
    case(
        "index_of_empty",
        Panicked("baml.panics.IndexOutOfBounds {index: 0, length: 0}"),
    ),
    case("string_concat_loop", Printed("100")),
    case("string_result", Printed("\"q\\\"z\\\"z\"")),
    case("string_length_counts_characters", Printed("7")),
    case("string_equality", Printed("true")),
    case("string_ordering", Printed("true")),
    case("string_of_three_parts", Printed("\"xy-xy!\"")),
    case("int_to_string", Printed("\"42/-7\"")),
    case(
        "template_holes",
        Printed("\"n=3 ok=true half=0.5 inf=Infinity\""),
    ),
    case("strings_survive_collection", Printed("600")),
];

/// The text of the thrown value in an uncaught-panic report.
fn thrown(report: &str) -> &str {
    let marker = "uncaught throw: ";
    let start = report
        .find(marker)
        .unwrap_or_else(|| panic!("no uncaught throw in: {report}"));
    report[start + marker.len()..].lines().next().unwrap_or("")
}

async fn interpret(engine: &Arc<BexEngine>, function: &str) -> Result<String, String> {
    engine
        .call_function_bound_args(
            &format!("user.{function}"),
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .map(|value| match value {
            // `baml run` prints nothing for a null result.
            BexExternalValue::Null => String::new(),
            other => other.render_readable(),
        })
        .map_err(|error| thrown(&error.to_string()).to_owned())
}

/// Compile and link `function` as the entry of an executable.
fn build(db: &ProjectDatabase, function: &str, directory: &std::path::Path) -> std::path::PathBuf {
    let entry = db
        .workspace_files()
        .into_iter()
        .flat_map(|file| file_functions(db, file).iter().copied())
        .find(|candidate| function_data(db, *candidate).name.as_str() == function)
        .unwrap_or_else(|| panic!("no function `{function}`"));
    let compiled = compile(db, entry, CompileOptions::default())
        .unwrap_or_else(|error| panic!("`{function}` does not compile: {error}"));
    let executable = directory.join(function);
    link(&compiled.object, &executable)
        .unwrap_or_else(|error| panic!("`{function}` does not link: {error}"));
    executable
}

/// Run an executable, collecting on every allocation if `stress`.
fn run(executable: &std::path::Path, stress: bool) -> Result<String, String> {
    let mut command = Command::new(executable);
    if stress {
        command.env("BAML_GC_STRESS", "1");
    }
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("`{}` does not start: {error}", executable.display()));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    match output.status.code() {
        Some(0) if stderr.is_empty() => Ok(stdout.trim_end_matches('\n').to_owned()),
        Some(1) if stdout.is_empty() => Err(thrown(&stderr).to_owned()),
        // Anything else is a failure of the executable itself; report it as
        // an outcome no case expects.
        _ => Ok(format!(
            "ended with {}; stdout: {stdout}; stderr: {stderr}",
            output.status
        )),
    }
}

fn outcome(result: &Result<String, String>) -> String {
    match result {
        Ok(text) => format!("{:?}", Printed(text)),
        Err(text) => format!("{:?}", Panicked(text)),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn native_code_ends_as_the_interpreter_does() {
    let db = setup_test_db(SOURCE);
    assert_no_diagnostic_errors(&db);
    let program = CheckedWorld::collect(&db, db.workspace_root().expect("a workspace root"))
        .expect("the program checks")
        .compile(OptLevel::Two)
        .expect("the program compiles to bytecode");
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_ops::SysOps::native()), Vec::new())
            .expect("the engine starts"),
    );
    let directory = tempfile::tempdir().expect("a temporary directory");

    let mut failures = Vec::new();
    for case in CASES {
        let expected = format!("{:?}", case.expected);
        let executable = build(&db, case.function, directory.path());
        let native = outcome(&run(&executable, false));
        let stressed = if case.stressed {
            outcome(&run(&executable, true))
        } else {
            expected.clone()
        };
        let interpreted = outcome(&interpret(&engine, case.function).await);
        // A case marked as diverging that the interpreter starts agreeing on
        // fails too, so that the mark is removed when it stops being true.
        if native != expected
            || stressed != expected
            || (interpreted == expected) != case.interpreter_agrees
        {
            failures.push(format!(
                "{}:\n  expected          {expected}\n  native            {native}\n  collecting always {stressed}\n  interpreter       {interpreted}{}",
                case.function,
                if case.interpreter_agrees { "" } else { " (expected to differ)" },
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
