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
    "captures",
    "catches",
    "class_unions",
    "classes",
    "defaults",
    "defers",
    "enums",
    "floats",
    "generic_classes",
    "generics",
    "higher_order",
    "interfaces",
    "lambdas",
    "literal_unions",
    "loops",
    "maps",
    "matching",
    "methods",
    "nullable",
    "primitive_unions",
    "recursion",
    "strings",
    "switches",
    "throws",
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
/// cover `pub` functions the test never calls, and clippy's style lints,
/// which judge hand-written code: the goldens are reviewed as backend output,
/// not held to the workspace's style.
macro_rules! generated_module {
    ($name:ident, $file:literal) => {
        #[allow(unreachable_pub, dead_code, clippy::all)]
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
generated_module!(throws, "native/generated/throws.rs");
generated_module!(catches, "native/generated/catches.rs");
generated_module!(defers, "native/generated/defers.rs");
generated_module!(literal_unions, "native/generated/literal_unions.rs");
generated_module!(primitive_unions, "native/generated/primitive_unions.rs");
generated_module!(class_unions, "native/generated/class_unions.rs");
generated_module!(lambdas, "native/generated/lambdas.rs");
generated_module!(captures, "native/generated/captures.rs");
generated_module!(higher_order, "native/generated/higher_order.rs");
generated_module!(generics, "native/generated/generics.rs");
generated_module!(generic_classes, "native/generated/generic_classes.rs");
generated_module!(interfaces, "native/generated/interfaces.rs");

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
        // A union-typed return wraps the member value in its union.
        let result = result.map(|mut value| {
            while let BexExternalValue::Union { value: inner, .. } = value {
                value = *inner;
            }
            value
        });
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
            // An uncaught `baml.panics.Exit { code }` is the engine's clean
            // termination: observed as the thrown class it is.
            Err(EngineError::Exit { code }) => {
                Observed::thrown("baml.panics.Exit", [("code".to_string(), code.to_string())])
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
async fn literal_unions_match_vm() {
    use literal_unions::*;
    let o = Oracle::new("literal_unions");
    for n in [-3, 0, 4, 10, 25] {
        check!(o, pick(n) => user_pick(int(n)));
        check!(o, level(n) => user_level(int(n)));
        check!(o, describe(n) => user_describe(int(n)));
        check!(o, level_name(n) => user_level_name(int(n)));
        check!(o, tag_is_hi(n) => user_tag_is_hi(int(n)));
        check!(o, is_lo(n) => user_is_lo(int(n)));
        check!(o, maybe_text(n) => user_maybe_text(int(n)));
        check!(o, flag(n) => user_flag(int(n)));
        check!(o, width(n) => user_width(int(n)));
        check!(o, render(n) => user_render(int(n)));
        check!(o, json_out(n) => user_json_out(int(n)));
        check!(o, count_lo(n) => user_count_lo(int(n)));
    }
    for (a, b) in [(1, 2), (1, 20), (30, 40)] {
        check!(o, same_tag(a, b) => user_same_tag(int(a), int(b)));
    }
}

/// A union-typed native result observes as the member it holds, as the VM's
/// union value does once unwrapped.
impl NativeValue for primitive_unions::Union_int_or_float {
    fn observe(self) -> Observed {
        match self {
            Self::int(value) => Observed::Int(value.get()),
            Self::float(value) => Observed::float(value),
        }
    }
}

impl NativeValue for Option<primitive_unions::Union_int_or_float> {
    fn observe(self) -> Observed {
        match self {
            Some(value) => value.observe(),
            None => Observed::Void,
        }
    }
}

#[tokio::test]
async fn primitive_unions_match_vm() {
    use primitive_unions::*;
    let o = Oracle::new("primitive_unions");
    for n in [-3, 0, 1, 2, 5, 12] {
        check!(o, make(n) => user_make(int(n)));
        check!(o, make_null(n) => user_make_null(int(n)));
        check!(o, reassign(n) => user_reassign(int(n)));
        // A union parameter takes a member value on both sides.
        let member = Union_int_or_float::int(int(n));
        check!(o, kind(n) => user_kind(member));
        check!(o, bound(n) => user_bound(member));
        check!(o, tested(n) => user_tested(member));
        check!(o, narrowed(n) => user_narrowed(member));
        check!(o, four_of(n) => user_four_of(int(n)));
        check!(o, wide_match(n) => user_wide_match(int(n)));
        check!(o, nullable_kind(n) => user_nullable_kind(int(n)));
        check!(o, nullable_narrow(n) => user_nullable_narrow(int(n)));
        check!(o, is_three(n) => user_is_three(int(n)));
        check!(o, is_half(n) => user_is_half(int(n)));
        check!(o, render(n) => user_render(int(n)));
        check!(o, json_out(n) => user_json_out(int(n)));
        check!(o, sum(n) => user_sum(int(n)));
        check!(o, doubled(n) => user_doubled(int(n)));
        check!(o, coalesce(n) => user_coalesce(int(n)));
        check!(o, coalesce_union(n) => user_coalesce_union(int(n)));
        check!(o, big(n) => user_big(int(n)));
        check!(o, flag(n) => user_flag(int(n)));
        check!(o, plain_tests(n) => user_plain_tests(int(n)));
    }
    for x in [0.0, 1.5, -2.5, 1e18, f64::NAN, f64::INFINITY] {
        let member = Union_int_or_float::float(x);
        check!(o, kind(x) => user_kind(member));
        check!(o, bound(x) => user_bound(member));
        check!(o, tested(x) => user_tested(member));
        check!(o, narrowed(x) => user_narrowed(member));
    }
    for (a, b) in [(2, 2), (2, 4), (1, 1), (1, 3), (0, 1), (0, 0), (3, 6)] {
        check!(o, same(a, b) => user_same(int(a), int(b)));
        check!(o, differ(a, b) => user_differ(int(a), int(b)));
        check!(o, nullable_same(a, b) => user_nullable_same(int(a), int(b)));
    }
    for (value, expected) in [
        (Union_int_or_float::int(int(1)), true),
        (Union_int_or_float::float(2.5), true),
    ] {
        // `is_number` takes a three-member union; the two-member value is
        // widened natively as the checker would.
        let widened = match value {
            Union_int_or_float::int(v) => Union_int_or_float_or_string::int(v),
            Union_int_or_float::float(v) => Union_int_or_float_or_string::float(v),
        };
        assert_eq!(user_is_number(widened).unwrap(), expected);
    }
    assert!(!user_is_number(Union_int_or_float_or_string::string(text("s"))).unwrap());
    for n in [0, 7] {
        check!(o, is_number(n) => user_is_number(Union_int_or_float_or_string::int(int(n))));
    }
    for s in ["", "x"] {
        check!(o, is_number(s) => user_is_number(Union_int_or_float_or_string::string(text(s))));
    }
    for raw in [
        "1",
        "1.0",
        "2.5",
        "-7",
        "\"x\"",
        "4611686018427387904",
        "null",
        "true",
        "[1]",
        "nope",
    ] {
        check!(o, json_in(raw) => user_json_in(text(raw)));
    }
    for raw in [
        r#"[{"amount": 1, "note": null}, {"amount": 2.5, "note": "n"}]"#,
        r#"[{"amount": 1.0}]"#,
        r#"[]"#,
        r#"[{"amount": "x", "note": null}]"#,
        r#"[{"note": null}]"#,
        r#"{"amount": 1}"#,
    ] {
        check!(o, json_rows(raw) => user_json_rows(text(raw)));
    }
}

#[tokio::test]
async fn class_unions_match_vm() {
    use class_unions::*;
    let o = Oracle::new("class_unions");
    for n in [-4, -1, 0, 1, 2, 7] {
        check!(o, describe(n) => user_describe(int(n)));
        check!(o, is_ok(n) => user_is_ok(int(n)));
        check!(o, tested(n) => user_tested(int(n)));
        check!(o, classify(n) => user_classify(int(n)));
        check!(o, fields(n) => user_fields(int(n)));
        check!(o, narrowed_arg(n) => user_narrowed_arg(int(n)));
        check!(o, label_name(n) => user_label_name(int(n)));
        check!(o, is_red(n) => user_is_red(int(n)));
        check!(o, not_big(n) => user_not_big(int(n)));
        check!(o, variant_arm(n) => user_variant_arm(int(n)));
        check!(o, literal_arm(n) => user_literal_arm(int(n)));
        check!(o, render(n) => user_render(int(n)));
        check!(o, json_out(n) => user_json_out(int(n)));
        check!(o, carry(n) => user_carry(int(n)));
        check!(o, count_ok(n) => user_count_ok(int(n)));
        check!(o, thrown_payload(n) => user_thrown_payload(int(n)));
    }
    for (a, b) in [(-1, -2), (-1, 0), (0, 0), (1, 1), (1, 2), (2, 5)] {
        check!(o, same_label(a, b) => user_same_label(int(a), int(b)));
    }
    // A class union decodes as the first member, in the spelled order, the
    // object fits; an object with both shapes' fields goes to the first.
    for raw in [
        r#"{"value": 3}"#,
        r#"{"message": "m"}"#,
        r#"{"value": 3, "message": "m"}"#,
        r#"{"value": "x"}"#,
        r#"{}"#,
        r#"[]"#,
        r#"null"#,
        r#"nope"#,
    ] {
        check!(o, json_in(raw) => user_json_in(text(raw)));
        check!(o, json_in_reversed(raw) => user_json_in_reversed(text(raw)));
    }
    // The same union spelled `Err | Ok` decodes the ambiguous object as
    // the other member, at a decode site and in a class field alike.
    for raw in [
        r#"{"outcome": {"value": 3, "message": "m"}}"#,
        r#"{"outcome": {"value": 3}}"#,
        r#"{"outcome": {"message": "m"}}"#,
    ] {
        check!(o, json_reversed(raw) => user_json_reversed(text(raw)));
    }
    assert_eq!(
        user_json_in(text(r#"{"value": 3, "message": "m"}"#)).unwrap(),
        text("ok 3")
    );
    assert_eq!(
        user_json_in_reversed(text(r#"{"value": 3, "message": "m"}"#)).unwrap(),
        text("err m")
    );
    for raw in [
        r#"{"outcome": {"value": 3}, "label": "Red"}"#,
        r#"{"outcome": {"message": "m"}, "label": "Big"}"#,
        r#"{"outcome": {"value": 3, "message": "m"}, "label": "Small"}"#,
        r#"{"outcome": {"value": 3}, "label": "Purple"}"#,
        r#"{"outcome": {"value": 3}}"#,
        r#"{"outcome": {}, "label": "Red"}"#,
    ] {
        check!(o, json_report(raw) => user_json_report(text(raw)));
    }
    for raw in [
        r#"{"payload": null}"#,
        r#"{}"#,
        r#"{"payload": {"value": 1}}"#,
        r#"{"payload": {"message": "m"}}"#,
        r#"{"payload": 5}"#,
    ] {
        check!(o, json_carrier(raw) => user_json_carrier(text(raw)));
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

#[tokio::test]
async fn throws_match_vm() {
    use throws::*;
    let o = Oracle::new("throws");
    for x in [3, 0, -1, -2] {
        check!(o, throw_if_negative(x) => user_throw_if_negative(int(x)));
        check!(o, throw_coded(x) => user_throw_coded(int(x)));
        check!(o, throw_local(x) => user_throw_local(int(x)));
        check!(o, throw_stdlib(x) => user_throw_stdlib(int(x)));
        check!(o, throw_panic_class(x) => user_throw_panic_class(int(x)));
        check!(o, throw_deep(x) => user_throw_deep(int(x)));
    }
    for n in [0, 1, 2, 3] {
        check!(o, throw_in_loop(n) => user_throw_in_loop(int(n)));
    }
    for x in [0, 2, 3, -1, -4] {
        check!(o, panic_deep(x) => user_panic_deep(int(x)));
    }
}

#[tokio::test]
async fn catches_match_vm() {
    use catches::*;
    let o = Oracle::new("catches");
    for x in [5, 0, -1, -7, 2] {
        check!(o, catch_class(x) => user_catch_class(int(x)));
        check!(o, catch_two(x) => user_catch_two(int(x)));
        check!(o, catch_bind(x) => user_catch_bind(int(x)));
        check!(o, catch_wild(x) => user_catch_wild(int(x)));
        check!(o, catch_wild_panic(x) => user_catch_wild_panic(int(x)));
        check!(o, catch_all_panic(x) => user_catch_all_panic(int(x)));
        check!(o, catch_all_panics(x) => user_catch_all_panics(int(x)));
        check!(o, catch_let_wild(x) => user_catch_let_wild(int(x)));
        check!(o, catch_deep(x) => user_catch_deep(int(x)));
        check!(o, catch_rethrow(x) => user_catch_rethrow(int(x)));
        check!(o, catch_throw_in_arm(x) => user_catch_throw_in_arm(int(x)));
        check!(o, catch_throw_bound(x) => user_catch_throw_bound(int(x)));
        check!(o, catch_throw_caught(x) => user_catch_throw_caught(int(x)));
        check!(o, catch_identity(x) => user_catch_identity(int(x)));
        check!(o, catch_statement(x) => user_catch_statement(int(x)));
        check!(o, catch_stdlib(x) => user_catch_stdlib(int(x)));
        check!(o, catch_exit(x) => user_catch_exit(int(x)));
    }
    for x in [5, 1, 0, -1, 101, 200, -100] {
        check!(o, catch_four(x) => user_catch_four(int(x)));
    }
    for x in [0, 1, 2, 3, 5, -1, -3, 4] {
        check!(o, catch_panics(x) => user_catch_panics(int(x)));
    }
    for x in [5, 0, -1, -100, -93] {
        check!(o, catch_nested(x) => user_catch_nested(int(x)));
    }
    for n in [5, -1, 0, 2] {
        check!(o, catch_in_loop(n) => user_catch_in_loop(int(n)));
    }
    for n in [0, 2, 5, 10] {
        check!(o, catch_returns(n) => user_catch_returns(int(n)));
    }
}

#[tokio::test]
async fn defers_match_vm() {
    use defers::*;
    let o = Oracle::new("defers");
    for x in [1, 2] {
        check!(o, defer_order(x) => user_defer_order(int(x)));
        check!(o, defer_sees_final(x) => user_defer_sees_final(int(x)));
        check!(o, defer_nested_blocks(x) => user_defer_nested_blocks(int(x)));
    }
    for early in [true, false] {
        check!(o, defer_early_return(early) => user_defer_early_return(early));
    }
    for n in [0, 1, 2, 3, 4] {
        check!(o, defer_loop(n) => user_defer_loop(int(n)));
        check!(o, defer_while(n) => user_defer_while(int(n)));
    }
    for x in [5, 0, -1] {
        check!(o, defer_unwind(x) => user_defer_unwind(int(x)));
        check!(o, defer_throws(x) => user_defer_throws(int(x)));
        check!(o, defer_throw_uncaught(x) => user_defer_throw_uncaught(int(x)));
        check!(o, defer_in_handler(x) => user_defer_in_handler(int(x)));
    }
}

/// A function-typed native result is called natively; the VM's value is
/// observed through what it computes, as the fixture's `chosen` does.
#[tokio::test]
async fn lambdas_match_vm() {
    use lambdas::*;
    let o = Oracle::new("lambdas");
    for n in [-3, 0, 1, 7, 100, MAX] {
        check!(o, direct(n) => user_direct(int(n)));
        check!(o, named_value(n) => user_named_value(int(n)));
        check!(o, passed(n) => user_passed(int(n)));
        check!(o, void_lambda(n) => user_void_lambda(int(n)));
        check!(o, throwing(n) => user_throwing(int(n)));
        check!(o, caught(n) => user_caught(int(n)));
        check!(o, in_array(n) => user_in_array(int(n)));
        check!(o, recursion_through_value(n.min(50)) => user_recursion_through_value(int(n.min(50))));
    }
    for (which, a) in [(0, 3), (1, 3), (2, -4), (0, MAX)] {
        check!(o, chosen(which, a) => user_chosen(int(which), int(a)));
        check!(o, nullable_fn(which, a) => user_nullable_fn(int(which), int(a)));
    }
    for (a, b) in [(1, 2), (MIN, 1), (MAX, -1)] {
        check!(o, two_args(a, b) => user_two_args(int(a), int(b)));
    }
    check!(o, strings("hi") => user_strings(text("hi")));
    // A function value is called like any other: `returned` is observed
    // through `chosen`; its own result has no VM-side observation.
    assert_eq!((user_returned(int(1)).unwrap())(int(9)).unwrap(), int(81));
    // Runaway recursion through a function value throws `StackOverflow`
    // instead of overflowing the native stack.
    assert_eq!(
        Observed::from(user_recursion_through_value(int(1_000_000))),
        Observed::thrown("baml.panics.StackOverflow", [])
    );
}

/// Captures: one value behind every handle, a cell per iteration where the
/// VM gives one, and closures that outlive their creator.
#[tokio::test]
async fn captures_match_vm() {
    use captures::*;
    let o = Oracle::new("captures");
    for n in [0, 1, 3, 10, 1000] {
        check!(o, counter(n) => user_counter(int(n)));
        check!(o, two_counters(n) => user_two_counters(int(n)));
        check!(o, loop_capture(n) => user_loop_capture(int(n)));
        check!(o, cfor_capture(n) => user_cfor_capture(int(n)));
        check!(o, while_capture(n) => user_while_capture(int(n)));
        check!(o, shared_loop_var(n) => user_shared_loop_var(int(n)));
    }
    for a in [-7, 0, 1, 42, MAX, MIN] {
        check!(o, seen_both_ways(a) => user_seen_both_ways(int(a)));
        check!(o, escaped(a) => user_escaped(int(a)));
        check!(o, captured_param(a) => user_captured_param(int(a)));
        check!(o, nested(a) => user_nested(int(a)));
        check!(o, closure_captures_closure(a) => user_closure_captures_closure(int(a)));
        check!(o, box_field(a) => user_box_field(int(a)));
        check!(o, captured_array(a) => user_captured_array(int(a)));
        check!(o, rebound(a) => user_rebound(int(a)));
        check!(o, throwing_capture(a) => user_throwing_capture(int(a)));
        check!(o, with_default(a, 10) => user_with_default(int(a), int(10)));
    }
    check!(o, strings("ab") => user_strings(text("ab")));
    // A closure escaping its creator keeps the cell alive and sees its own
    // copy of the binding.
    let add = user_make_adder(int(5)).unwrap();
    assert_eq!(add(int(1)).unwrap(), int(6));
    let first = user_make_counter().unwrap();
    let second = user_make_counter().unwrap();
    assert_eq!(first().unwrap(), int(1));
    assert_eq!(first().unwrap(), int(2));
    assert_eq!(second().unwrap(), int(1));
    // The class handle a closure captures is the caller's object.
    let b = bex_aot::shared(user_Box { n: int(1) });
    assert_eq!(user_mutate_box(b.clone()).unwrap(), int(5));
    assert_eq!(b.borrow().n, int(5));
}

/// The array methods that call back into a function value: every one walks
/// a snapshot, `sort_by` is the VM's merge sort comparison for comparison
/// (the logged sequence agrees), a throwing comparator or key leaves the
/// array as it was, a mutating one has its changes overwritten.
#[tokio::test]
async fn higher_order_matches_vm() {
    use higher_order::*;
    let o = Oracle::new("higher_order");
    for n in [0, 1, 2, 3] {
        check!(o, suite(n) => user_suite(int(n)));
        check!(o, render_items(n) => user_render_items(int(n)));
    }
    for (a, b) in [(1, 2), (2, 2), (3, 2), (MIN, MAX)] {
        check!(o, cmp_ints(a, b) => user_cmp_ints(int(a), int(b)));
    }
    for (a, b) in [
        (1.0, 2.0),
        (2.0, 2.0),
        (f64::NAN, 1.0),
        (1.0, f64::NAN),
        (f64::NAN, f64::NAN),
        (-0.0, 0.0),
        (f64::NEG_INFINITY, -1e300),
    ] {
        check!(o, cmp_floats(a, b) => user_cmp_floats(a, b));
    }
    for (a, b) in [("a", "b"), ("b", "a"), ("", ""), ("é", "z"), ("Z", "a")] {
        check!(o, cmp_strings(a, b) => user_cmp_strings(text(a), text(b)));
    }
}

/// Generic functions: every instance the callers reach, compared through
/// the non-generic functions that call them (the VM erases the type
/// arguments; the native instances are named by them).
#[tokio::test]
async fn generics_match_vm() {
    use generics::*;
    let o = Oracle::new("generics");
    for n in [-3, 0, 1, 7, 100, MAX - 10] {
        check!(o, use_ints(n) => user_use_ints(int(n)));
        check!(o, use_classes(n) => user_use_classes(int(n)));
        check!(o, use_arrays(n) => user_use_arrays(int(n)));
        check!(o, use_nullable(n) => user_use_nullable(int(n)));
        check!(o, renders(n) => user_renders(int(n)));
        check!(o, type_tests(n) => user_type_tests(int(n)));
    }
    for s in ["", "a", "zz"] {
        check!(o, use_strings(s) => user_use_strings(text(s)));
    }
    for x in [0.0, -1.5, 2.0, f64::NAN, f64::INFINITY] {
        check!(o, use_floats(x) => user_use_floats(x));
    }
    // An instance is an ordinary function: the same call, natively.
    assert_eq!(user_identity__int(int(4)).unwrap(), int(4));
    assert_eq!(
        user_first__string(user_wrap__string(text("w")).unwrap()).unwrap(),
        text("w")
    );
}

/// Generic classes: one struct per instantiation, its methods instantiated
/// with it, compared through the functions that build them (the VM has one
/// class carrying its type arguments per instance).
#[tokio::test]
async fn generic_classes_match_vm() {
    use generic_classes::*;
    let o = Oracle::new("generic_classes");
    for n in [-2, 0, 1, 5, 40, MAX / 4] {
        check!(o, boxes(n) => user_boxes(int(n)));
        check!(o, stacks(n.min(50)) => user_stacks(int(n.min(50))));
        check!(o, nested(n) => user_nested(int(n)));
        check!(o, renders(n) => user_renders(int(n)));
        check!(o, type_tests(n) => user_type_tests(int(n)));
        check!(o, nullable_boxes(n) => user_nullable_boxes(int(n)));
    }
    for (n, s) in [(0, ""), (3, "x"), (-9, "long text")] {
        check!(o, pairs(n, s) => user_pairs(int(n), text(s)));
    }
    // An instance's methods are ordinary functions over its struct.
    let b = user_make_box__int(int(2)).unwrap();
    user_Box_set__int(b.clone(), int(7)).unwrap();
    assert_eq!(user_Box_get__int(b).unwrap(), int(7));
}

/// Interfaces: static calls on concrete receivers, a match over the
/// implementors on interface values, default bodies, interface fields,
/// primitive implementors, narrowing, rendering and JSON, all compared
/// with the VM's run-time resolution.
#[tokio::test]
async fn interfaces_match_vm() {
    use interfaces::*;
    let o = Oracle::new("interfaces");
    for n in [-3, 0, 1, 2, 5, 40] {
        check!(o, static_calls(n) => user_static_calls(int(n)));
        check!(o, shapes(n) => user_shapes(int(n)));
        check!(o, narrowing(n) => user_narrowing(int(n)));
        check!(o, switch_arms(n) => user_switch_arms(int(n)));
        check!(o, bounded_calls(n) => user_bounded_calls(int(n)));
        check!(o, fields(n) => user_fields(int(n)));
        check!(o, nullable(n) => user_nullable(int(n)));
        check!(o, renders(n) => user_renders(int(n)));
        check!(o, in_union(n) => user_in_union(int(n)));
        check!(o, in_field(n) => user_in_field(int(n)));
        check!(o, union_receivers(n) => user_union_receivers(int(n)));
    }
    for (n, s) in [(0, ""), (3, "abc"), (-2, "x")] {
        check!(o, primitives(n, s) => user_primitives(int(n), text(s)));
    }
    for (a, b) in [(1, 2), (2, 2), (3, -1), (0, 0), (MAX, MIN)] {
        check!(o, comparisons(a, b) => user_comparisons(int(a), int(b)));
    }
    // An interface value is its implementor's value in a variant; the
    // method on it is the implementor's.
    let shape = user_Shape::user_Square(bex_aot::handle::shared(user_Square { side: int(3) }));
    assert_eq!(
        user_total(bex_aot::array::new::<user_Shape>(vec![
            shape.clone(),
            shape
        ]))
        .unwrap(),
        int(18)
    );
}
