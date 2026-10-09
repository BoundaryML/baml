//! MIR-to-Rust backend: differential execution against the VM.
//!
//! Every fixture under `tests/native/fixtures/*.baml` is compiled by the
//! backend and the emitted Rust is committed under
//! `tests/native/generated/*.rs` (`native_backend.rs` keeps it current). Here
//! the committed source is compiled into this test binary, linked against
//! `bex_aot`, and executed for the same inputs as the bytecode VM. Values
//! and thrown objects must agree (`*_match_vm` tests).
//!
//! Keeping the generated code in the tree means a reviewer reads real backend
//! output, the default CI lane never spawns `cargo` on generated code, and the
//! semantics are still exercised, not only the text.

use std::{path::Path, sync::Arc};

use baml_test_support::compile_source_with_opt;
use baml_tests::engine::OptLevel;
use bex_aot::{Int63, Str, Thrown};
use bex_engine::{
    BexCallArg, BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder,
};
use sys_native::SysOpsExt;

/// The fixtures, one `generated_module!` each below.
const FIXTURES: &[&str] = &[
    "arith",
    "arrays",
    "benchmarks",
    "bigint",
    "bitwise",
    "calls",
    "classes",
    "defaults",
    "enums",
    "floats",
    "loops",
    "maps",
    "matching",
    "methods",
    "nullable",
    "recursion",
    "strings",
    "switches",
];

fn fixture_source(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/native/fixtures")
        .join(format!("{name}.baml"));
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

/// Every fixture file has a module below, and nothing else is in the
/// directory.
#[test]
fn fixture_list_matches_the_directory() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/native/fixtures");
    let mut found: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter_map(|name| name.strip_suffix(".baml").map(str::to_owned))
        .collect();
    found.sort();
    assert_eq!(
        found, FIXTURES,
        "add the fixture to FIXTURES and a generated_module!"
    );
}

// ---------------------------------------------------------------------------
// Differential execution against the VM
// ---------------------------------------------------------------------------

/// The committed backend output, compiled into this test binary. The
/// generated files carry their own `#![allow]` set; the extra allowances here
/// cover `pub` functions the test never calls.
macro_rules! generated_module {
    ($name:ident, $file:literal) => {
        #[allow(unreachable_pub, dead_code)]
        #[path = $file]
        mod $name;
    };
}

generated_module!(arith, "native/generated/arith.rs");
generated_module!(calls, "native/generated/calls.rs");
generated_module!(loops, "native/generated/loops.rs");
generated_module!(matching, "native/generated/matching.rs");
generated_module!(arrays, "native/generated/arrays.rs");
generated_module!(strings, "native/generated/strings.rs");
generated_module!(switches, "native/generated/switches.rs");
generated_module!(floats, "native/generated/floats.rs");
generated_module!(bitwise, "native/generated/bitwise.rs");
generated_module!(nullable, "native/generated/nullable.rs");
generated_module!(methods, "native/generated/methods.rs");
generated_module!(recursion, "native/generated/recursion.rs");
generated_module!(classes, "native/generated/classes.rs");
generated_module!(benchmarks, "native/generated/benchmarks.rs");
generated_module!(defaults, "native/generated/defaults.rs");
generated_module!(enums, "native/generated/enums.rs");
generated_module!(maps, "native/generated/maps.rs");
generated_module!(bigint, "native/generated/bigint.rs");

/// What a call produced, on either backend. A thrown object is compared by
/// class and by every field but `message`: the classes and their data are
/// part of the language, while the wording of a message is shared only where
/// the code producing it is (`bex_lang`), and otherwise each backend's own.
#[derive(Debug, PartialEq)]
enum Observed {
    Int(i64),
    Bool(bool),
    /// The bits, with every NaN as the one `f64::NAN`: BAML has one NaN, and
    /// a hardware NaN's payload is not part of the language.
    Float(u64),
    Str(String),
    Void,
    Thrown {
        class: String,
        fields: Vec<(String, String)>,
    },
}

impl Observed {
    fn float(value: f64) -> Self {
        Self::Float(if value.is_nan() {
            f64::NAN.to_bits()
        } else {
            value.to_bits()
        })
    }

    fn thrown(class: &str, fields: impl IntoIterator<Item = (String, String)>) -> Self {
        Self::Thrown {
            class: class.to_owned(),
            fields: fields
                .into_iter()
                .filter(|(name, _)| name != "message")
                .collect(),
        }
    }
}

/// A native return value, as [`Observed`] sees it.
trait NativeValue {
    fn observe(self) -> Observed;
}

impl NativeValue for Int63 {
    fn observe(self) -> Observed {
        Observed::Int(self.get())
    }
}

impl NativeValue for bool {
    fn observe(self) -> Observed {
        Observed::Bool(self)
    }
}

impl NativeValue for f64 {
    fn observe(self) -> Observed {
        Observed::float(self)
    }
}

impl NativeValue for Str {
    fn observe(self) -> Observed {
        Observed::Str(self.to_string())
    }
}

impl NativeValue for () {
    fn observe(self) -> Observed {
        Observed::Void
    }
}

impl<T: NativeValue> From<Result<T, Thrown>> for Observed {
    fn from(value: Result<T, Thrown>) -> Self {
        match value {
            Ok(v) => v.observe(),
            Err(thrown) => Self::thrown(
                thrown.class_fqn(),
                thrown
                    .fields()
                    .into_iter()
                    .map(|(name, value)| (name.to_owned(), value)),
            ),
        }
    }
}

/// One VM engine per fixture, reused for every call: engine construction is
/// the expensive part of a VM call, and repeatedly constructing engines inside
/// one test has been observed to block.
struct Oracle {
    engine: Arc<BexEngine>,
}

impl Oracle {
    fn new(fixture: &str) -> Self {
        let program = compile_source_with_opt(&fixture_source(fixture), OptLevel::One);
        let engine = BexEngine::new_with_runtime_compiler(
            program,
            Arc::new(sys_ops::SysOps::native()),
            Vec::new(),
            bex_project::runtime_compiler(),
        )
        .expect("engine for fixture");
        Self {
            engine: Arc::new(engine),
        }
    }

    /// Call `user.<entry>` with positional arguments and reduce the outcome.
    async fn call(&self, entry: &str, args: Vec<BexExternalValue>) -> Observed {
        let args = args
            .into_iter()
            .map(|value| BexCallArg::Provided(Box::new(value)))
            .collect();
        let result = self
            .engine
            .call_function_bound_args(
                &format!("user.{entry}"),
                args,
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true,
            )
            .await;
        match result {
            Ok(BexExternalValue::Int(v)) => Observed::Int(v),
            Ok(BexExternalValue::Bool(v)) => Observed::Bool(v),
            Ok(BexExternalValue::Float(v)) => Observed::float(v),
            Ok(BexExternalValue::String(v)) => Observed::Str(v.to_string()),
            Ok(BexExternalValue::Null) => Observed::Void,
            Ok(other) => panic!("{entry}: unexpected VM value {other:?}"),
            Err(EngineError::UnhandledThrow { value, .. }) => {
                // A `throws A | B` function wraps the object in its union.
                let mut thrown = *value;
                while let BexExternalValue::Union { value, .. } = thrown {
                    thrown = *value;
                }
                match thrown {
                    BexExternalValue::Instance {
                        class_name, fields, ..
                    } => Observed::thrown(
                        &class_name,
                        fields
                            .into_iter()
                            .map(|(name, value)| (name, value.render_readable())),
                    ),
                    other => panic!("{entry}: VM threw a non-class value {other:?}"),
                }
            }
            Err(err) => panic!("{entry}: VM error is not a throw: {err}"),
        }
    }
}

fn int(v: i64) -> Int63 {
    Int63::new(v).expect("test literal in range")
}

const MIN: i64 = -(1 << 62);
const MAX: i64 = (1 << 62) - 1;

trait IntoExternal {
    fn into_external(self) -> BexExternalValue;
}
impl IntoExternal for i64 {
    fn into_external(self) -> BexExternalValue {
        BexExternalValue::Int(self)
    }
}
impl IntoExternal for &str {
    fn into_external(self) -> BexExternalValue {
        BexExternalValue::String(self.into())
    }
}
impl IntoExternal for bool {
    fn into_external(self) -> BexExternalValue {
        BexExternalValue::Bool(self)
    }
}
impl IntoExternal for f64 {
    fn into_external(self) -> BexExternalValue {
        BexExternalValue::Float(self)
    }
}
impl IntoExternal for num_bigint::BigInt {
    fn into_external(self) -> BexExternalValue {
        BexExternalValue::Bigint(self)
    }
}

/// A `bigint` argument for the VM, from its decimal digits.
fn vm_big(digits: &str) -> num_bigint::BigInt {
    digits.parse().expect("decimal digits")
}

/// The same `bigint` argument for native code.
fn big(digits: &str) -> bex_aot::BigInt {
    bex_aot::BigInt::from(vm_big(digits))
}

/// Run one case natively and on the VM and compare. Arguments are positional
/// and listed in the fixture's declaration order.
macro_rules! check {
    ($oracle:ident, $f:ident ( $( $val:expr ),* ) => $native:expr) => {{
        let observed_native: Observed = Observed::from($native);
        let observed_vm = $oracle
            .call(stringify!($f), vec![$( $val.into_external() ),*])
            .await;
        assert_eq!(
            observed_native,
            observed_vm,
            "{}({}) native (left) vs VM (right)",
            stringify!($f),
            Vec::<String>::from([$( format!("{:?}", $val) ),*]).join(", ")
        );
    }};
}

#[tokio::test]
async fn arith_matches_vm() {
    use arith::*;
    let o = Oracle::new("arith");
    for (a, b) in [(7, 3), (-5, 12), (100, -100), (0, 0), (MAX, 1), (MIN, 2)] {
        check!(o, arith(a, b) => user_arith(int(a), int(b)));
        check!(o, compare(a, b) => user_compare(int(a), int(b)));
    }
    for (a, b) in [(7, 2), (-7, 2), (7, -2), (1, 0), (MIN, -1), (0, 5)] {
        check!(o, divide(a, b) => user_divide(int(a), int(b)));
        check!(o, remainder(a, b) => user_remainder(int(a), int(b)));
    }
    for a in [0, 1, -1, MIN, MAX] {
        check!(o, add_to_max(a) => user_add_to_max(int(a)));
        check!(o, negate(a) => user_negate(int(a)));
    }
    for (x, lo, hi) in [(5, 1, 10), (1, 1, 10), (10, 1, 10), (0, 1, 10), (11, 1, 10)] {
        check!(o, in_range(x, lo, hi) => user_in_range(int(x), int(lo), int(hi)));
        check!(o, is_edge(x, lo, hi) => user_is_edge(int(x), int(lo), int(hi)));
    }
}

#[tokio::test]
async fn loops_match_vm() {
    use loops::*;
    let o = Oracle::new("loops");
    for n in [1, 2, 6, 27, 97] {
        check!(o, collatz_steps(n) => user_collatz_steps(int(n)));
    }
    for limit in [0, 1, 10, 1000, 123_456] {
        check!(o, sum_odd_squares(limit) => user_sum_odd_squares(int(limit)));
    }
    for (a, b) in [(48, 18), (18, 48), (7, 0), (0, 7), (1, 1)] {
        check!(o, gcd(a, b) => user_gcd(int(a), int(b)));
    }
    for n in [1, 2, 7, 91, 97, 1_000_003] {
        check!(o, first_factor(n) => user_first_factor(int(n)));
    }
    for n in [0, 1, 5, 10] {
        check!(o, nested(n) => user_nested(int(n)));
    }
}

#[tokio::test]
async fn matching_matches_vm() {
    use matching::*;
    let o = Oracle::new("matching");
    for x in [0, 1, 2, 3, 7, -1] {
        check!(o, classify(x) => user_classify(int(x)));
        check!(o, match_returns(x) => user_match_returns(int(x)));
    }
    for n in [0, 2, 3, 5, 6, 10] {
        check!(o, match_loop(n) => user_match_loop(int(n)));
    }
    for b in [true, false] {
        check!(o, bool_match(b) => user_bool_match(b));
    }
}

#[tokio::test]
async fn calls_match_vm() {
    use calls::*;
    let o = Oracle::new("calls");
    for n in [0, 1, 10, 100] {
        check!(o, sum_of_squares(n) => user_sum_of_squares(int(n)));
        check!(o, count_even(n) => user_count_even(int(n)));
    }
    for x in [5, 0, -1, -100, MAX] {
        check!(o, step_twice(x) => user_step_twice(int(x)));
        check!(o, checked_step(x) => user_checked_step(int(x)));
    }
    for x in [0, 3, 10] {
        check!(o, is_even(x) => user_is_even(int(x)));
        check!(o, noop(x) => user_noop(int(x)));
    }
}

fn text(v: &str) -> Str {
    Str::from(v)
}

#[tokio::test]
async fn arrays_match_vm() {
    use arrays::*;
    let o = Oracle::new("arrays");
    for n in [0, 1, 5, 40] {
        check!(o, build(n) => user_build(int(n)));
        check!(o, bubble(n) => user_bubble(int(n)));
        check!(o, alias_mutation(n) => user_alias_mutation(int(n)));
        check!(o, push_during_iteration(n) => user_push_during_iteration(int(n)));
    }
    for n in [0, 2, -1, -3, 3, -4, 7] {
        check!(o, negative_index(n) => user_negative_index(int(n)));
    }
}

#[tokio::test]
async fn strings_match_vm() {
    use strings::*;
    let o = Oracle::new("strings");
    for n in [0, 1, 7, -12, MAX] {
        check!(o, greet(n) => user_greet(int(n)));
        check!(o, same(n) => user_same(int(n)));
        check!(o, ordered(n) => user_ordered(int(n)));
    }
    for n in [0, 1, 3] {
        check!(o, repeat_len(n) => user_repeat_len(int(n)));
        check!(o, ascii_check(n) => user_ascii_check(int(n)));
    }
}

#[tokio::test]
async fn classes_match_vm() {
    use classes::*;
    let o = Oracle::new("classes");
    for n in [0, 3, -2, MAX] {
        check!(o, make(n) => user_make(int(n)));
        check!(o, render(n) => user_render(int(n)));
        check!(o, nullable(n) => user_nullable(int(n)));
    }
    for raw in [
        r#"{"x": 3, "y": 4}"#,
        r#"{"y": 4, "x": -3, "extra": true}"#,
        r#"{"x": 4611686018427387903, "y": 2}"#,
        r#"{"x": 1.5, "y": 2}"#,
        r#"{"x": 1}"#,
        r#"not json"#,
    ] {
        check!(o, roundtrip(raw) => user_roundtrip(text(raw)));
    }
}

#[tokio::test]
async fn benchmarks_match_vm() {
    use benchmarks::*;
    let o = Oracle::new("benchmarks");
    for raw in [
        r#"{"count": 128, "seed": 1729}"#,
        r#"{"count": 0, "seed": 5}"#,
    ] {
        check!(o, pipeline(raw) => user_pipeline(text(raw)));
    }
    for n in [0, 1, 2, 17, 300] {
        check!(o, sorted_checksum(n) => user_sorted_checksum(int(n)));
    }
}

#[tokio::test]
async fn switches_match_vm() {
    use switches::*;
    let o = Oracle::new("switches");
    for x in -1..10 {
        check!(o, sw_empty(x) => user_sw_empty(int(x)));
        check!(o, sw_same(x) => user_sw_same(int(x)));
        check!(o, sw_fall(x) => user_sw_fall(int(x)));
        check!(o, chain(x) => user_chain(int(x)));
        check!(o, partial(x) => user_partial(int(x)));
    }
}

#[tokio::test]
async fn floats_match_vm() {
    use floats::*;
    let o = Oracle::new("floats");
    let specials = [
        0.0,
        -0.0,
        1.5,
        -2.25,
        1e300,
        f64::MIN_POSITIVE,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
    ];
    for a in specials {
        for b in [0.0, -0.0, 1.5, f64::INFINITY, f64::NAN] {
            check!(o, fsum(a, b) => user_fsum(a, b));
            check!(o, fcmp(a, b) => user_fcmp(a, b));
            check!(o, fdiv(a, b) => user_fdiv(a, b));
        }
        check!(o, fmix(a) => user_fmix(a));
        check!(o, ffloor(a) => user_ffloor(a));
        check!(o, fitrunc(a) => user_fitrunc(a));
        check!(o, nan_order(a) => user_nan_order(a));
        check!(o, fneg(a) => user_fneg(a));
    }
    for a in [
        2.5,
        -2.5,
        4611686018427387904.0,
        -4611686018427387904.0,
        1e19,
    ] {
        check!(o, fitrunc(a) => user_fitrunc(a));
    }
}

#[tokio::test]
async fn bitwise_matches_vm() {
    use bitwise::*;
    let o = Oracle::new("bitwise");
    let values = [0, 1, -1, 0b1100, 0b1010, 1 << 40, MIN, MAX];
    for a in values {
        for b in values {
            check!(o, band(a, b) => user_band(int(a), int(b)));
            check!(o, bor(a, b) => user_bor(int(a), int(b)));
            check!(o, bxor(a, b) => user_bxor(int(a), int(b)));
        }
        for n in [0, 1, 3, 62, 63, 64, 100, -1, MIN] {
            check!(o, shl(a, n) => user_shl(int(a), int(n)));
            check!(o, shr(a, n) => user_shr(int(a), int(n)));
            check!(o, mix(a, n) => user_mix(int(a), int(n)));
        }
    }
}

#[tokio::test]
async fn nullable_matches_vm() {
    use nullable::*;
    let o = Oracle::new("nullable");
    for n in [-1, 0, 1, 7, MAX] {
        check!(o, or_zero(n) => user_or_zero(int(n)));
        check!(o, describe(n) => user_describe(int(n)));
    }
    for n in [0, 1, 3] {
        check!(o, chain(n) => user_chain(int(n)));
        check!(o, sparse(n) => user_sparse(int(n)));
    }
    check!(o, sparse(MAX) => user_sparse(int(MAX)));
}

#[tokio::test]
async fn methods_match_vm() {
    use methods::*;
    let o = Oracle::new("methods");
    for k in [0, 1, 5, 100] {
        check!(o, count(k) => user_count(int(k)));
        check!(o, alias(k) => user_alias(int(k)));
    }
    check!(o, alias(MAX) => user_alias(int(MAX)));
}

/// The depth limit is the VM's frame count (256). The VM counts every
/// frame, including the entry call, while native code counts only frames on
/// a cycle, so the two overflow a frame or two apart: the inputs stay clear
/// of the boundary on both sides.
#[tokio::test]
async fn recursion_matches_vm() {
    use recursion::*;
    let o = Oracle::new("recursion");
    for n in [0, 1, 5, 20, 21, -3] {
        check!(o, fact(n) => user_fact(int(n)));
    }
    for n in [0, 1, 7, 10, 200] {
        check!(o, is_even(n) => user_is_even(int(n)));
    }
    for n in [0, 100, 200, 300, 100_000] {
        check!(o, deep(n) => user_deep(int(n)));
    }
    for n in [0, 1, 2, 10, 20] {
        check!(o, fib(n) => user_fib(int(n)));
    }
}

/// Omitted arguments take the callee's constant default at the call site;
/// the VM fills them in the callee's prologue.
#[tokio::test]
async fn defaults_match_vm() {
    use defaults::*;
    let o = Oracle::new("defaults");
    for n in [0, 1, 7, -4, MAX] {
        check!(o, use_defaults(n) => user_use_defaults(int(n)));
        check!(o, use_named(n) => user_use_named(int(n)));
        check!(o, use_method(n) => user_use_method(int(n)));
    }
    for x in [0.0, 1.5, -8.0, f64::INFINITY, f64::NAN] {
        check!(o, use_float(x) => user_use_float(x));
    }
    // Explicit arguments still reach a defaulted parameter unchanged.
    check!(o, scale(5, 2, 1) => user_scale(int(5), int(2), int(1)));
    check!(o, label("x", "hey", true) => user_label(text("x"), text("hey"), true));
}

/// Enum values, matches on them, equality, rendering and JSON both ways.
#[tokio::test]
async fn enums_match_vm() {
    use enums::*;
    let o = Oracle::new("enums");
    for n in [0, 1, 2, 3, 7, -1, -2] {
        check!(o, render(n) => user_render(int(n)));
        check!(o, json_out(n) => user_json_out(int(n)));
        check!(o, count_red(n) => user_count_red(int(n)));
        check!(o, tinted(n) => user_tinted(int(n)));
        check!(o, nullable(n) => user_nullable(int(n)));
        for m in [0, 1, 2] {
            check!(o, same(n, m) => user_same(int(n), int(m)));
            check!(o, differ(n, m) => user_differ(int(n), int(m)));
        }
    }
    for raw in [
        r#"{"color": "Red", "weight": 1}"#,
        r#"{"weight": -3, "color": "Blue"}"#,
        r#"{"color": "Purple", "weight": 1}"#,
        r#"{"color": 2, "weight": 1}"#,
        r#"{"weight": 1}"#,
        r#"["Red"]"#,
    ] {
        check!(o, json_in(raw) => user_json_in(text(raw)));
    }
}

/// Maps keep insertion order through writes and deletes, share one table
/// between handles, panic on an absent subscript like the VM, and take the
/// VM's JSON rules (string keys only, by declared key type).
#[tokio::test]
async fn maps_match_vm() {
    use maps::*;
    let o = Oracle::new("maps");
    for n in [0, 1, 5, -7, MAX] {
        check!(o, tally(n) => user_tally(int(n)));
        check!(o, json_out(n) => user_json_out(int(n)));
        check!(o, get_or_insert(n) => user_get_or_insert(int(n)));
        check!(o, alias(n) => user_alias(int(n)));
        check!(o, json_int_keys(n) => user_json_int_keys(int(n)));
    }
    for n in [1, 2, -1, 0, 3] {
        check!(o, lookup(n) => user_lookup(int(n)));
    }
    for b in [true, false] {
        check!(o, flags(b) => user_flags(b));
    }
    for raw in [
        r#"{"stock": {"b": 2, "a": 1}, "name": "n"}"#,
        r#"{"name": "n", "stock": {}}"#,
        r#"{"name": "n", "stock": {"a": "1"}}"#,
        r#"{"name": "n", "stock": [1]}"#,
        r#"{"name": "n"}"#,
    ] {
        check!(o, json_in(raw) => user_json_in(text(raw)));
    }
    for raw in [r#"{"1": 1}"#, "{}", "[]"] {
        check!(o, json_int_keys_in(raw) => user_json_int_keys_in(text(raw)));
    }
}

/// Arbitrary-precision arithmetic, comparison, bit operations, `int`
/// widening and narrowing, the methods, and JSON, including every panic and
/// error class the operations raise.
#[tokio::test]
async fn bigint_matches_vm() {
    use bigint::*;
    let o = Oracle::new("bigint");
    let values = [
        "0",
        "1",
        "-1",
        "7",
        "-7",
        "4611686018427387904",
        "-4611686018427387905",
        "123456789012345678901234567890",
        "-99999999999999999999999999999999",
    ];
    for a in values {
        for b in values {
            check!(o, arith(vm_big(a), vm_big(b)) => user_arith(big(a), big(b)));
            check!(o, divide(vm_big(a), vm_big(b)) => user_divide(big(a), big(b)));
            check!(o, compare(vm_big(a), vm_big(b)) => user_compare(big(a), big(b)));
            check!(o, bits(vm_big(a), vm_big(b)) => user_bits(big(a), big(b)));
        }
        for n in [0, 1, -3, 62, MAX, MIN] {
            check!(o, mixed(vm_big(a), n) => user_mixed(big(a), int(n)));
        }
        check!(o, narrow(vm_big(a)) => user_narrow(big(a)));
        check!(o, negate(vm_big(a)) => user_negate(big(a)));
        check!(o, json_out(vm_big(a)) => user_json_out(big(a)));
    }
    for (a, b) in [
        ("2", "10"),
        ("2", "0"),
        ("0", "0"),
        ("-2", "3"),
        ("10", "-1"),
        ("1000", "10"),
        ("1024", "2"),
        ("1", "10"),
        ("0", "10"),
        ("10", "1"),
        ("-4", "2"),
        ("2", "300000000"),
        ("2", "99999999999999999999"),
    ] {
        check!(o, methods(vm_big(a), vm_big(b)) => user_methods(big(a), big(b)));
    }
    for n in [0, 1, -5, MAX, MIN] {
        check!(o, widen(n) => user_widen(int(n)));
    }
    for n in [0, 100, -1, 300_000_000] {
        check!(o, shift_wide(n) => user_shift_wide(int(n)));
    }
    check!(o, literal() => user_literal());
    for s in [
        "42",
        "-7",
        "+0",
        "99999999999999999999",
        "",
        "12a",
        "0x2a",
        "1_000",
        " 5 ",
    ] {
        check!(o, parse(s) => user_parse(text(s)));
    }
    for raw in [
        "42",
        "-42",
        "18446744073709551616",
        "-123456789012345678901234567890",
        "\"42\"",
        "\"-99999999999999999999\"",
        "12.5",
        "1e3",
        "1.0",
        "\"12.5\"",
        "\" 42\"",
        "true",
        "[1]",
        "nope",
    ] {
        check!(o, json_in(raw) => user_json_in(text(raw)));
    }
}
