//! Every case runs twice, on the interpreter and as a native executable, and
//! the two must end the same way: the same printed result, or the same
//! uncaught panic.
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

const SOURCE: &str = r"
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
function int_max() -> int { let p = pow2(61); (p - 1) + p }
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
function remainder_takes_dividend_sign() -> int { -count(7) % count(3) }
function bit_operators() -> int { (count(12) & count(10)) | (count(1) ^ count(3)) }
function int_extremes() -> int { int_max() + int_min() }
function largest_product() -> int { pow2(31) * pow2(30) }
function shift_left_into_sign() -> int { count(1) << count(62) }
function shift_left_past_width() -> int { count(1) << count(63) }
function shift_right_past_width() -> int { -count(8) >> count(70) }
function shift_right() -> int { -count(9) >> count(1) }
function match_on_int() -> int {
  pick(count(0)) + pick(count(1)) * 100 + pick(count(7)) * 10000 + pick(count(9)) * 1000000
}
function mixed_arguments() -> float { mix(count(3), half(1), count(1) == 1) }

function add_overflows() -> int { int_max() + count(1) }
function subtract_overflows() -> int { int_min() - count(1) }
function multiply_overflows_63_bits() -> int { pow2(31) * pow2(31) }
function multiply_overflows_64_bits() -> int { pow2(40) * pow2(40) }
function negate_overflows() -> int { -int_min() }
function divide_overflows() -> int { int_min() / -count(1) }
function divide_by_zero() -> int { count(5) / count(0) }
function remainder_by_zero() -> int { count(5) % count(0) }
function negative_shift() -> int { count(1) << -count(1) }

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
";

/// Each case's entry function and how it must end on both sides.
const CASES: &[(&str, Outcome<'static>)] = &[
    ("sum_of_squares", Outcome::Printed("332833500")),
    ("recursion", Outcome::Printed("6765")),
    ("division_truncates", Outcome::Printed("-3")),
    ("remainder_takes_dividend_sign", Outcome::Printed("-1")),
    ("bit_operators", Outcome::Printed("10")),
    ("int_extremes", Outcome::Printed("-1")),
    ("largest_product", Outcome::Printed("2305843009213693952")),
    (
        "shift_left_into_sign",
        Outcome::Printed("-4611686018427387904"),
    ),
    ("shift_left_past_width", Outcome::Printed("0")),
    ("shift_right_past_width", Outcome::Printed("-1")),
    ("shift_right", Outcome::Printed("-5")),
    ("match_on_int", Outcome::Printed("40302010")),
    ("mixed_arguments", Outcome::Printed("3.5")),
    (
        "add_overflows",
        Outcome::Panicked(
            "baml.panics.IntegerOverflow {message: \"4611686018427387903 + 1 overflows int\"}",
        ),
    ),
    (
        "subtract_overflows",
        Outcome::Panicked(
            "baml.panics.IntegerOverflow {message: \"-4611686018427387904 - 1 overflows int\"}",
        ),
    ),
    (
        "multiply_overflows_63_bits",
        Outcome::Panicked(
            "baml.panics.IntegerOverflow {message: \"2147483648 * 2147483648 overflows int\"}",
        ),
    ),
    (
        "multiply_overflows_64_bits",
        Outcome::Panicked(
            "baml.panics.IntegerOverflow {message: \"1099511627776 * 1099511627776 overflows int\"}",
        ),
    ),
    (
        "negate_overflows",
        Outcome::Panicked(
            "baml.panics.IntegerOverflow {message: \"-(-4611686018427387904) overflows int\"}",
        ),
    ),
    (
        "divide_overflows",
        Outcome::Panicked(
            "baml.panics.IntegerOverflow {message: \"-4611686018427387904 / -1 overflows int\"}",
        ),
    ),
    (
        "divide_by_zero",
        Outcome::Panicked("baml.panics.DivisionByZero {dividend: 5}"),
    ),
    (
        "remainder_by_zero",
        Outcome::Panicked("baml.panics.DivisionByZero {dividend: 5}"),
    ),
    (
        "negative_shift",
        Outcome::Panicked(
            "baml.panics.NegativeBitShift {message: \"bit shift count is negative: -1\"}",
        ),
    ),
    ("int_times_float", Outcome::Printed("6.5")),
    (
        "float_shortest_digits",
        Outcome::Printed("0.30000000000000004"),
    ),
    (
        "float_without_exponent",
        Outcome::Printed("10000000000000000000000.0"),
    ),
    ("float_small", Outcome::Printed("0.00000003333333333333333")),
    ("float_divide_by_zero", Outcome::Printed("-inf")),
    ("float_nan", Outcome::Printed("NaN")),
    ("float_remainder", Outcome::Printed("-1.5")),
    ("nan_orders_above_numbers", Outcome::Printed("true")),
    ("negative_zero_equals_zero", Outcome::Printed("true")),
    ("or_skips_right_side", Outcome::Printed("true")),
    ("and_skips_right_side", Outcome::Printed("false")),
    ("not_of_bool", Outcome::Printed("false")),
    ("returns_null", Outcome::Printed("")),
];

/// How a run ended. `baml run` prints a result on stdout and exits zero, or
/// reports an uncaught panic on stderr and exits one.
#[derive(Debug, PartialEq, Eq)]
enum Outcome<'a> {
    Printed(&'a str),
    Panicked(&'a str),
}

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

fn run_native(
    db: &ProjectDatabase,
    function: &str,
    directory: &std::path::Path,
) -> Result<String, String> {
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
    let output = Command::new(&executable)
        .output()
        .unwrap_or_else(|error| panic!("`{function}` does not start: {error}"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    match output.status.code() {
        Some(0) => {
            assert_eq!(stderr, "", "`{function}` wrote to stderr");
            Ok(stdout.trim_end_matches('\n').to_owned())
        }
        Some(1) => {
            assert_eq!(stdout, "", "`{function}` printed before panicking");
            Err(thrown(&stderr).to_owned())
        }
        _ => panic!("`{function}` ended with {}: {stderr}", output.status),
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
    for (function, expected) in CASES {
        let interpreted = interpret(&engine, function).await;
        let native = run_native(&db, function, directory.path());
        let as_outcome = |result: &'_ Result<String, String>| match result {
            Ok(text) => format!("{:?}", Outcome::Printed(text)),
            Err(text) => format!("{:?}", Outcome::Panicked(text)),
        };
        let expected = format!("{expected:?}");
        let (interpreted, native) = (as_outcome(&interpreted), as_outcome(&native));
        if interpreted != expected || native != expected {
            failures.push(format!(
                "{function}:\n  expected    {expected}\n  interpreter {interpreted}\n  native      {native}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
