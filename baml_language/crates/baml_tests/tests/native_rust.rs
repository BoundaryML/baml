//! MIR-to-Rust backend: golden and differential tests.
//!
//! Every fixture under `tests/native/fixtures/*.baml` is compiled by the
//! native backend and the emitted Rust is committed next to it under
//! `tests/native/generated/*.rs`. Two things are checked:
//!
//! 1. The committed source is exactly what the backend emits today
//!    (`generated_sources_are_current`). Regenerate with
//!    `UPDATE_NATIVE_FIXTURES=1 cargo test -p baml_tests --test native_rust`.
//! 2. The committed source is `include!`d into this test binary, linked against
//!    `bex_lang`, and executed for the same inputs as the bytecode VM. Values
//!    and panics must agree (`*_matches_vm` tests).
//!
//! Keeping the generated code in the tree means a reviewer reads real backend
//! output, the default CI lane never spawns `cargo` on generated code, and the
//! semantics are still exercised, not only the text.
//!
//! `UPDATE_NATIVE_FIXTURES` is a developer regeneration knob like insta's
//! `INSTA_UPDATE`, read directly rather than registered in `baml_env`.
#![allow(clippy::disallowed_methods)]

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use baml_db::{baml_compiler2_hir::item_data::file_functions, baml_compiler2_rust as native};
use baml_test_support::{compile_source_with_opt, setup_test_db};
use baml_tests::engine::OptLevel;
use bex_engine::{BexCallArg, BexEngine, BexExternalValue, FunctionCallContextBuilder};
use bex_lang::{Int63, Panic, Str, Thrown};
use sys_native::SysOpsExt;

const FIXTURES: &[&str] = &[
    "arith",
    "loops",
    "matching",
    "calls",
    "arrays",
    "strings",
    "classes",
    "benchmarks",
];

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/native")
}

fn fixture_source(name: &str) -> String {
    let path = fixture_dir().join("fixtures").join(format!("{name}.baml"));
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

/// Compile every function of a fixture file into one native module.
///
/// The module borrows the database, so the database is leaked; four fixtures
/// per test process is a bounded cost.
fn emit_fixture(name: &str) -> native::NativeModule<'static> {
    let db: &'static _ = Box::leak(Box::new(setup_test_db(&fixture_source(name))));
    let roots: Vec<_> = db
        .workspace_files()
        .into_iter()
        .flat_map(|file| file_functions(db, file).iter().copied())
        .collect();
    native::compile_many(db, &roots)
        .unwrap_or_else(|rejection| panic!("fixture {name} was rejected: {rejection}"))
}

#[test]
fn generated_sources_are_current() {
    let update = std::env::var_os("UPDATE_NATIVE_FIXTURES").is_some();
    let mut stale = Vec::new();
    for name in FIXTURES {
        let module = emit_fixture(name);
        let path = fixture_dir().join("generated").join(format!("{name}.rs"));
        let committed = std::fs::read_to_string(&path).unwrap_or_default();
        if committed == module.rust_source {
            continue;
        }
        if update {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &module.rust_source).unwrap();
        } else {
            let diff = similar::TextDiff::from_lines(&committed, &module.rust_source)
                .unified_diff()
                .header("committed", "emitted")
                .to_string();
            stale.push(format!("{}:\n{diff}", path.display()));
        }
    }
    assert!(
        stale.is_empty(),
        "generated native sources are stale; rerun with UPDATE_NATIVE_FIXTURES=1\n{}",
        stale.join("\n")
    );
}

#[test]
fn every_fixture_is_structured() {
    for name in FIXTURES {
        let module = emit_fixture(name);
        assert!(
            !module.rust_source.contains("loop {\n        match"),
            "{name}: generated code uses a block dispatcher instead of structured control flow"
        );
        assert!(
            !module.rust_source.contains("bex_vm"),
            "{name}: generated code must not mention the VM"
        );
    }
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
generated_module!(classes, "native/generated/classes.rs");
generated_module!(benchmarks, "native/generated/benchmarks.rs");

/// What the VM produced, with an uncaught panic reduced to its readable form
/// (the text after `uncaught throw: `), which is also what `bex_lang::Panic`
/// renders.
#[derive(Debug, PartialEq)]
enum Observed {
    Int(i64),
    Bool(bool),
    Str(String),
    Void,
    Panic(String),
}

impl From<Result<Int63, Panic>> for Observed {
    fn from(value: Result<Int63, Panic>) -> Self {
        match value {
            Ok(v) => Self::Int(v.get()),
            Err(p) => Self::Panic(p.render_readable()),
        }
    }
}

impl From<Result<bool, Panic>> for Observed {
    fn from(value: Result<bool, Panic>) -> Self {
        match value {
            Ok(v) => Self::Bool(v),
            Err(p) => Self::Panic(p.render_readable()),
        }
    }
}

impl From<Result<Str, Thrown>> for Observed {
    fn from(value: Result<Str, Thrown>) -> Self {
        match value {
            Ok(v) => Self::Str(v.to_string()),
            Err(t) => Self::Panic(t.render_readable()),
        }
    }
}

impl From<Result<Int63, Thrown>> for Observed {
    fn from(value: Result<Int63, Thrown>) -> Self {
        match value {
            Ok(v) => Self::Int(v.get()),
            Err(t) => Self::Panic(t.render_readable()),
        }
    }
}

impl From<Result<bool, Thrown>> for Observed {
    fn from(value: Result<bool, Thrown>) -> Self {
        match value {
            Ok(v) => Self::Bool(v),
            Err(t) => Self::Panic(t.render_readable()),
        }
    }
}

impl From<Result<(), Thrown>> for Observed {
    fn from(value: Result<(), Thrown>) -> Self {
        match value {
            Ok(()) => Self::Void,
            Err(t) => Self::Panic(t.render_readable()),
        }
    }
}

impl From<Result<(), Panic>> for Observed {
    fn from(value: Result<(), Panic>) -> Self {
        match value {
            Ok(()) => Self::Void,
            Err(p) => Self::Panic(p.render_readable()),
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
            Ok(BexExternalValue::String(v)) => Observed::Str(v.to_string()),
            Ok(BexExternalValue::Null) => Observed::Void,
            Ok(other) => panic!("{entry}: unexpected VM value {other:?}"),
            Err(err) => {
                let rendered = err.to_string();
                let (_, tail) = rendered
                    .rsplit_once("uncaught throw: ")
                    .unwrap_or_else(|| panic!("{entry}: VM error is not a throw: {rendered}"));
                Observed::Panic(tail.trim_end().to_owned())
            }
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
            vec![$( format!("{:?}", $val) ),*].join(", ")
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
