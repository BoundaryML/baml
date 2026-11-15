//! The MIR inliner: which calls it replaces with their callee's body, which
//! it leaves, and what a stack trace shows for an error raised in inlined
//! code.

use baml_compiler2_hir::{
    item_data::{file_functions, function_data},
    loc::DeclRef,
};
use baml_compiler2_mir::{
    Constant, MirFunctionKind, Operand, OptLevel, Terminator, lower_function,
    pretty::display_function,
};
use baml_db::{ProjectDatabase, testing::assert_no_diagnostic_errors};
use baml_tests::engine::{TestDbExt, run_compiled};
use bex_external_types::BexExternalValue;
use indexmap::IndexMap;

fn database(files: &[(&str, &str)]) -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.workspace(std::path::Path::new("."));
    for (path, source) in files {
        db.file(path, source);
    }
    assert_no_diagnostic_errors(&db);
    db
}

/// `function`'s MIR at `opt`: its rendering, and how many calls of the
/// program's own functions it makes.
fn lowered(source: &str, function: &str, opt: OptLevel) -> (String, usize) {
    let db = database(&[("test.baml", source)]);
    let file = db.workspace_files()[0];
    let loc = *file_functions(&db, file)
        .iter()
        .find(|&&loc| function_data(&db, loc).name.as_str() == function)
        .unwrap_or_else(|| panic!("no function `{function}`"));
    let mir = lower_function(&db, loc, opt)
        .as_ref()
        .unwrap_or_else(|error| panic!("MIR lowering failed: {error}"));
    let MirFunctionKind::Bytecode(body) = &mir.kind else {
        panic!("`{function}` has no bytecode body")
    };
    let calls = body
        .blocks
        .iter()
        .filter(|block| {
            matches!(
                &block.terminator,
                Some(Terminator::Call {
                    callee: Operand::Constant(
                        Constant::Function(DeclRef::Source(callee))
                        | Constant::GenericFunction { func: DeclRef::Source(callee), .. }
                    ),
                    ..
                }) if callee.file(&db) == file
            )
        })
        .count();
    (display_function(&db, mir), calls)
}

/// How many calls of the program's own functions `function` still makes
/// once optimized.
fn calls(source: &str, function: &str) -> usize {
    lowered(source, function, OptLevel::Two).1
}

/// Run `main` of a program compiled at `opt`, returning its value or its
/// error's traceback.
async fn run_at(files: &[(&str, &str)], opt: OptLevel) -> Result<BexExternalValue, String> {
    let db = database(files);
    let program = baml_db::CheckedWorld::collect(&db, db.workspace_root().unwrap())
        .expect("the world checks")
        .compile(opt)
        .expect("the program compiles");
    run_compiled(program, "main", IndexMap::new(), false)
        .await
        .result
        .map_err(|error| error.to_string())
}

/// The outcome of `main`, which must be the same with and without inlining.
async fn run(source: &str) -> Result<BexExternalValue, String> {
    let files = [("test.baml", source)];
    let optimized = run_at(&files, OptLevel::Two).await;
    assert_eq!(optimized, run_at(&files, OptLevel::One).await);
    optimized
}

#[tokio::test]
async fn a_call_becomes_the_callee_body() {
    let source = r#"
function add(a: int, b: int) -> int { a + b }
function main() -> int {
  let total = add(40, 1)
  add(total, 1)
}
"#;
    let (mir, calls) = lowered(source, "main", OptLevel::Two);
    assert_eq!(calls, 0, "{mir}");
    assert!(mir.contains("[inlined user.add]"), "{mir}");
    assert_eq!(run(source).await, Ok(BexExternalValue::Int(42)));
}

#[test]
fn lower_levels_keep_the_call() {
    let source = r#"
function add(a: int, b: int) -> int { a + b }
function main(x: int) -> int { add(x, 1) }
"#;
    for opt in [OptLevel::Zero, OptLevel::One] {
        let (mir, calls) = lowered(source, "main", opt);
        assert_eq!(calls, 1, "{opt:?}: {mir}");
    }
}

#[tokio::test]
async fn arguments_bind_by_position_name_and_default() {
    let source = r#"
function scale(value: int, by: int = 10, offset: int = 1) -> int { value * by + offset }
function swap(value: int) -> int {
  // The callee writes its parameter; the caller's variable must not change.
  value = value + 100
  value
}
function main() -> int {
  let seed = 2
  let moved = swap(seed)
  scale(seed) + scale(seed, by = 3) + scale(seed, offset = 5) + scale(seed, offset = 7, by = 4) + moved
}
"#;
    assert_eq!(calls(source, "main"), 0);
    // 21 + 7 + 25 + 15 + 102
    assert_eq!(run(source).await, Ok(BexExternalValue::Int(170)));
}

#[tokio::test]
async fn every_return_of_the_callee_reaches_the_call_site() {
    let source = r#"
class Account {
  balance: int
  function tier(self) -> string {
    if (self.balance < 0) { return "overdrawn" }
    if (self.balance < 100) { return "basic" }
    "premium"
  }
}
function main() -> string {
  let tiers = ""
  for (let balance in [-5, 50, 500]) {
    tiers = tiers + Account { balance: balance }.tier() + " "
  }
  tiers
}
"#;
    assert_eq!(calls(source, "main"), 0);
    assert_eq!(
        run(source).await,
        Ok(BexExternalValue::String("overdrawn basic premium ".into()))
    );
}

#[tokio::test]
async fn calls_exposed_by_inlining_are_inlined_in_turn() {
    let source = r#"
function inner(x: int) -> int { x + 1 }
function middle(x: int) -> int { inner(x) * 2 }
function outer(x: int) -> int { middle(x) - 3 }
function entry(x: int) -> int { outer(x) }
function main() -> int { entry(4) }
"#;
    let (mir, calls) = lowered(source, "entry", OptLevel::Two);
    assert_eq!(calls, 0, "{mir}");
    assert!(
        mir.contains("[inlined user.outer > user.middle > user.inner]"),
        "{mir}"
    );
    assert_eq!(run(source).await, Ok(BexExternalValue::Int(7)));
}

#[tokio::test]
async fn recursion_is_never_unrolled() {
    let source = r#"
function fib(n: int) -> int { if (n < 2) { n } else { fib(n - 1) + fib(n - 2) } }
function is_even(n: int) -> bool { if (n == 0) { true } else { is_odd(n - 1) } }
function is_odd(n: int) -> bool { if (n == 0) { false } else { is_even(n - 1) } }
function main() -> int { if (is_even(10)) { fib(10) } else { 0 } }
"#;
    // A function that calls itself keeps both calls and is copied nowhere.
    assert_eq!(calls(source, "fib"), 2);
    // `is_odd` is copied into `is_even` once; the call back stays a call.
    let (mir, calls) = lowered(source, "is_even", OptLevel::Two);
    assert_eq!(calls, 1, "{mir}");
    assert!(mir.contains("[inlined user.is_odd]"), "{mir}");
    assert!(!mir.contains("user.is_odd > "), "{mir}");
    assert_eq!(run(source).await, Ok(BexExternalValue::Int(55)));
}

#[tokio::test]
async fn an_inlined_throw_lands_in_the_callers_handler() {
    let source = r#"
function checked(divisor: int) -> int {
  if (divisor == 0) { throw "zero" }
  100 / divisor
}
function main() -> int {
  let total = 0
  for (let divisor in [5, 0]) {
    total += checked(divisor) catch (e) { _ => -1 }
  }
  total
}
"#;
    let (mir, calls) = lowered(source, "main", OptLevel::Two);
    assert_eq!(calls, 0, "{mir}");
    // The copied blocks unwind to the handler the call did.
    assert!(
        mir.lines()
            .any(|line| line.contains("[unwind ") && line.contains("[inlined user.checked]")),
        "{mir}"
    );
    assert_eq!(run(source).await, Ok(BexExternalValue::Int(19)));
}

/// `main` calls `target` once; the call must survive optimization.
fn assert_not_inlined(source: &str) {
    let (mir, calls) = lowered(source, "main", OptLevel::Two);
    assert_eq!(calls, 1, "{mir}");
    assert!(
        !mir.lines()
            .any(|line| line.contains("[inlined") && line.contains("user.target")),
        "{mir}"
    );
}

#[test]
fn a_traced_function_keeps_its_call() {
    assert_not_inlined(
        r#"
/// baml:$trace=trace.timing
function target(x: int) -> int { x + 1 }
function main() -> int { target(1) }
"#,
    );
}

#[test]
fn a_call_with_a_trace_attachment_stays_a_call() {
    assert_not_inlined(
        r#"
function target(x: int) -> int { x + 1 }
function main() -> int { target(1, $trace = trace.span()) }
"#,
    );
}

#[test]
fn a_function_with_a_handler_keeps_its_call() {
    assert_not_inlined(
        r#"
function target(x: int) -> int { (100 / x) catch (e) { _ => 0 } }
function main() -> int { target(1) }
"#,
    );
}

#[test]
fn a_function_with_a_defer_keeps_its_call() {
    assert_not_inlined(
        r#"
function target(x: int) -> int {
  defer { log.info("done") }
  x + 1
}
function main() -> int { target(1) }
"#,
    );
}

#[test]
fn a_function_with_a_closure_keeps_its_call() {
    assert_not_inlined(
        r#"
function target(x: int) -> int {
  let add = (y: int) -> int { x + y }
  add(1)
}
function main() -> int { target(1) }
"#,
    );
}

#[test]
fn a_generic_call_with_types_from_the_callers_frame_stays_a_call() {
    // `U` is only known when `main` runs.
    assert_not_inlined(
        r#"
function target<T>(x: T) -> T[] { [x] }
function main<U>(x: U) -> U[] { target(x) }
"#,
    );
}

#[test]
fn a_function_that_suspends_keeps_its_call() {
    assert_not_inlined(
        r#"
function target(pending: baml.future.Future<int, never>) -> int { await pending }
function main() -> int { target(spawn { 1 }) }
"#,
    );
}

#[test]
fn a_function_that_logs_keeps_its_call() {
    assert_not_inlined(
        r#"
function target(x: int) -> int {
  log.info(x)
  x + 1
}
function main() -> int { target(1) }
"#,
    );
}

#[test]
fn a_large_function_keeps_its_call() {
    let steps: String = (0..40)
        .map(|step| format!("  total = total * 3 + {step}\n"))
        .collect();
    assert_not_inlined(&format!(
        "function target(x: int) -> int {{\n  let total = x\n{steps}  total\n}}\n\
         function main() -> int {{ target(1) }}\n"
    ));
}

#[tokio::test]
async fn an_inlined_error_reports_every_frame() {
    // The same program as `errors.rs`'s `error_stack_trace`, whose calls are
    // all inlined at this level.
    let source = r#"
function three() -> int {
  throw "boom"
}

function two() -> int {
  three()
}

function one() -> int {
  two()
}

function main() -> int {
  one()
}
"#;
    assert_eq!(calls(source, "main"), 0);
    let error = run(source).await.unwrap_err();
    insta::assert_snapshot!(error, @r#"
    Traceback (most recent call last):
      File "test.baml", line 15, in user.main
      File "test.baml", line 11, in user.one
      File "test.baml", line 7, in user.two
      File "test.baml", line 3, in user.three
    uncaught throw: "boom"
    "#);
}

#[tokio::test]
async fn an_inlined_panic_reports_the_callee_line() {
    let source = r#"
class Ratio {
  top: int
  bottom: int
  function value(self) -> int {
    let scaled = self.top * 10
    scaled / self.bottom
  }
}

function describe(ratio: Ratio) -> int {
  let value = ratio.value()
  value + 1
}

function main() -> int {
  let fine = describe(Ratio { top: 1, bottom: 1 })
  fine + describe(Ratio { top: 1, bottom: 0 })
}
"#;
    assert_eq!(calls(source, "main"), 0);
    let error = run(source).await.unwrap_err();
    insta::assert_snapshot!(error, @r#"
    Traceback (most recent call last):
      File "test.baml", line 18, in user.main
      File "test.baml", line 12, in user.describe
      File "test.baml", line 7, in user.Ratio.value
    uncaught throw: baml.panics.DivisionByZero {dividend: 10}
    "#);
}

#[tokio::test]
async fn a_call_made_from_inlined_code_reports_its_inlined_callers() {
    // `fail` calls itself, so it keeps its frame; the call to it sits in
    // code inlined two calls deep.
    let source = r#"
function fail(depth: int) -> int {
  if (depth == 0) { throw "bottom" }
  fail(depth - 1)
}

function wrap(depth: int) -> int {
  fail(depth) + 1
}

function main() -> int {
  let first = wrap(0) catch (e) { _ => 0 }
  first + wrap(1)
}
"#;
    assert_eq!(calls(source, "wrap"), 1);
    assert_eq!(calls(source, "main"), 2);
    let error = run(source).await.unwrap_err();
    insta::assert_snapshot!(error, @r#"
    Traceback (most recent call last):
      File "test.baml", line 13, in user.main
      File "test.baml", line 8, in user.wrap
      File "test.baml", line 4, in user.fail
      File "test.baml", line 3, in user.fail
    uncaught throw: "bottom"
    "#);
}

#[tokio::test]
async fn a_frame_inlined_from_another_file_reports_its_own_file_and_line() {
    let files = [
        (
            "main.baml",
            "function main() -> int {\n  let base = 1\n  unwrap_value(base - 1)\n}\n",
        ),
        (
            "helpers.baml",
            "// Padding so the helper's lines differ from the caller's.\n//\n//\n//\n//\n\
             function unwrap_value(value: int) -> int {\n  if (value == 0) {\n    throw \"empty\"\n  }\n  value\n}\n",
        ),
    ];
    let optimized = run_at(&files, OptLevel::Two).await.unwrap_err();
    assert_eq!(optimized, run_at(&files, OptLevel::One).await.unwrap_err());
    insta::assert_snapshot!(optimized, @r#"
    Traceback (most recent call last):
      File "main.baml", line 3, in user.main
      File "helpers.baml", line 8, in user.unwrap_value
    uncaught throw: "empty"
    "#);
}

#[test]
fn an_inlined_finalizable_instance_is_still_built() {
    // Nothing reads the handle, but the collector will run its `cleanup`.
    let source = r#"
class Handle {
  log string[]
  function cleanup(self) -> void { self.log.push("closed"); }
}
function open(log: string[]) -> Handle { Handle { log: log } }
function main(log: string[]) -> void { let handle = open(log); }
"#;
    let (mir, calls) = lowered(source, "main", OptLevel::Two);
    assert_eq!(calls, 0, "{mir}");
    assert!(mir.contains("user.Handle {"), "{mir}");
}

#[tokio::test]
async fn an_inlined_standard_library_frame_stays_out_of_the_traceback() {
    // `baml.json.stringify` is a small function written in BAML.
    let source = r#"
function main() -> string {
  let text = baml.json.stringify(1)
  throw text
}
"#;
    let db = database(&[("test.baml", source)]);
    let program = baml_db::CheckedWorld::collect(&db, db.workspace_root().unwrap())
        .expect("the world checks")
        .compile(OptLevel::Two)
        .expect("the program compiles");
    let index = baml_tests::engine::function_index(&program, "user.main").unwrap();
    let Some(bex_vm_types::Object::Function(main)) = program.objects.get(index) else {
        panic!("expected function");
    };
    // The inlined frame names its own package and its `<builtin>` file, which
    // is what keeps it out of a user-facing traceback.
    let frames: Vec<(String, &str)> = main
        .bytecode
        .inline_frames
        .iter()
        .map(|frame| (main.inlined_name(frame), frame.source_file.as_str()))
        .collect();
    assert!(
        frames
            .iter()
            .any(|(name, file)| name == "baml.json.stringify"
                && file.starts_with(bex_vm_types::errors::BUILTIN_SOURCE_PREFIX)),
        "{frames:?}"
    );
    let error = run(source).await.unwrap_err();
    insta::assert_snapshot!(error, @r#"
    Traceback (most recent call last):
      File "test.baml", line 4, in user.main
    uncaught throw: "1"
    "#);
}

#[tokio::test]
async fn a_hook_that_raises_falls_back_to_the_target() {
    // The hook raises in its own frame, with no call in between.
    let source = r#"
class Counts { hooks: int, bodies: int }
function failing_hook(counts: Counts, settings: trace.Settings) -> trace.Options? throws never {
    counts.hooks += 1;
    throw baml.panics.UserPanic { message: "hook failure" };
}
/// baml:$trace=failing_hook
function target(counts: Counts) -> int { counts.bodies += 1; 11 }
function main() -> int {
    let counts = Counts { hooks: 0, bodies: 0 };
    target(counts) + counts.hooks * 100 + counts.bodies * 1000
}
"#;
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(60), run(source))
        .await
        .expect("the program finishes");
    assert_eq!(outcome, Ok(BexExternalValue::Int(1111)));
}

#[tokio::test]
async fn a_callee_loop_is_copied_as_it_is() {
    // `count_up` reaches the caller with its loop already rotated: the test
    // once at the top and once at the bottom. Cleaning up the caller must
    // not rotate it again, and both copies of the test stay inside the
    // inlined call.
    let source = r#"
function count_up(n: int) -> int {
  let total = 0
  let i = 0
  while (i < n) {
    total += i
    i += 1
  }
  total
}
function entry(n: int) -> int { count_up(n) + 1 }
function main() -> int { entry(5) }
"#;
    let tests = |mir: &str| mir.lines().filter(|line| line.contains(" < ")).count();
    let (callee, _) = lowered(source, "count_up", OptLevel::Two);
    let (caller, calls) = lowered(source, "entry", OptLevel::Two);
    assert_eq!(calls, 0, "{caller}");
    assert_eq!(tests(&callee), 2, "{callee}");
    assert_eq!(tests(&caller), 2, "{caller}");
    let mut header = "";
    for line in caller.lines() {
        if line.trim_start().starts_with("bb") {
            header = line;
        }
        if line.contains(" < ") {
            assert!(header.contains("[inlined user.count_up]"), "{caller}");
        }
    }
    assert_eq!(run(source).await, Ok(BexExternalValue::Int(11)));
}

#[tokio::test]
async fn an_instance_passed_to_an_inlined_method_is_never_built() {
    // The shape of the `method-call-100k` workload.
    let source = r#"
class Vec {
  x: int
  y: int
  function norm2(self) -> int {
    return self.x * self.x + self.y * self.y
  }
}
function main() -> int {
  let acc = 0;
  for (let i = 0; i < 10; i += 1) {
    let v = Vec { x: i, y: i + 1 };
    acc += v.norm2();
  };
  return acc;
}
"#;
    let (mir, calls) = lowered(source, "main", OptLevel::Two);
    assert_eq!(calls, 0, "{mir}");
    assert!(!mir.contains("user.Vec {"), "{mir}");
    assert_eq!(run(source).await, Ok(BexExternalValue::Int(670)));
}

#[tokio::test]
async fn a_generic_callee_reads_the_types_the_call_names() {
    // `T` is `int` at one call and `string` at the other; the arrays the
    // copies build must say so.
    let source = r#"
class Box<T> {
  value: T
  function get(self) -> T { self.value }
  function listed(self) -> T[] { [self.value] }
}
function wrap<T>(x: T) -> T[] { [x] }
function describe(xs: unknown) -> string {
  if (xs is int[]) { "ints" } else if (xs is string[]) { "strings" } else { "other" }
}
function entry(n: int, s: string) -> string {
  let numbers = wrap(n)
  let words = wrap(s)
  let boxed = Box<int> { value: n }
  let sum = boxed.get() + numbers[0]
  `${describe(numbers)} ${describe(words)} ${describe(boxed.listed())} ${sum}`
}
function main() -> string { entry(4, "four") }
"#;
    let (mir, calls) = lowered(source, "entry", OptLevel::Two);
    assert_eq!(calls, 0, "{mir}");
    assert!(mir.contains("[inlined user.wrap]"), "{mir}");
    assert_eq!(
        run(source).await,
        Ok(BexExternalValue::String("ints strings ints 8".into()))
    );
}

#[tokio::test]
async fn an_error_in_an_inlined_generic_callee_reports_its_frame() {
    let source = r#"
function first<T>(xs: T[]) -> T {
  if (xs.length() == 0) {
    throw "empty"
  }
  xs[0]
}

function main() -> int {
  let none: int[] = []
  first(none)
}
"#;
    let (mir, _) = lowered(source, "main", OptLevel::Two);
    assert!(mir.contains("[inlined user.first]"), "{mir}");
    let error = run(source).await.unwrap_err();
    insta::assert_snapshot!(error, @r#"
    Traceback (most recent call last):
      File "test.baml", line 11, in user.main
      File "test.baml", line 4, in user.first
    uncaught throw: "empty"
    "#);
}

#[tokio::test]
async fn a_call_through_a_closure_that_captures_nothing_is_inlined() {
    // The shape of the `closure-apply-1m` workload: `apply` is inlined, which
    // leaves a call of the closure where it was made.
    let source = r#"
function apply(f: (int) -> int, x: int) -> int { f(x) }
function entry(n: int) -> int {
  let double = (x: int) -> int { x * 2 };
  let total = 0;
  for (let i = 0; i < n; i += 1) { total += apply(double, i); };
  total
}
function main() -> int { entry(5) }
"#;
    let (mir, calls) = lowered(source, "entry", OptLevel::Two);
    assert_eq!(calls, 0, "{mir}");
    assert!(
        mir.contains("[inlined user.apply > .<lambda(entry, 0)>]"),
        "{mir}"
    );
    assert!(!mir.contains("= call "), "{mir}");
    assert_eq!(run(source).await, Ok(BexExternalValue::Int(20)));
}

#[tokio::test]
async fn a_call_through_a_closure_over_locals_reads_and_writes_them() {
    // `shift` reads `offset` and `bump` writes `count`, both through the
    // cells the closures captured.
    let source = r#"
function entry(n: int) -> int {
  let offset = n + 1;
  let count = 0;
  let shift = (x: int) -> int { x + offset };
  let bump = () -> int { count += 1; count };
  let total = 0;
  for (let i = 0; i < n; i += 1) {
    total += shift(i);
    bump();
    offset += 1;
  };
  total * 100 + count
}
function main() -> int { entry(3) }
"#;
    let (mir, _) = lowered(source, "entry", OptLevel::Two);
    assert!(mir.contains("[inlined .<lambda(entry, 0)>]"), "{mir}");
    assert!(mir.contains("[inlined .<lambda(entry, 1)>]"), "{mir}");
    assert!(!mir.contains("= call copy"), "{mir}");
    // With both closures gone, `offset` and `count` are plain locals again.
    assert!(!mir.contains("*_"), "{mir}");
    // shift: (0 + 4) + (1 + 5) + (2 + 6) = 18; count = 3.
    assert_eq!(run(source).await, Ok(BexExternalValue::Int(1803)));
}

#[tokio::test]
async fn a_closure_made_each_iteration_sees_that_iterations_binding() {
    let source = r#"
function entry(n: int) -> int {
  let total = 0;
  for (let i = 0; i < n; i += 1) {
    let square = i * i;
    let get = () -> int { square + i };
    total += get();
  };
  total
}
function main() -> int { entry(4) }
"#;
    // 0 + 2 + 6 + 12
    assert_eq!(run(source).await, Ok(BexExternalValue::Int(20)));
}

#[tokio::test]
async fn an_error_in_an_inlined_closure_reports_its_frame() {
    let source = r#"
function apply(f: (int) -> int, x: int) -> int {
  f(x)
}

function main() -> int {
  let fail = (x: int) -> int {
    throw "boom"
  };
  apply(fail, 1)
}
"#;
    let (mir, _) = lowered(source, "main", OptLevel::Two);
    assert!(mir.contains("<lambda(main, 0)>]"), "{mir}");
    let error = run(source).await.unwrap_err();
    insta::assert_snapshot!(error, @r#"
    Traceback (most recent call last):
      File "test.baml", line 10, in user.main
      File "test.baml", line 3, in user.apply
      File "test.baml", line 8, in .<lambda(main, 0)>
    uncaught throw: "boom"
    "#);
}

#[tokio::test]
async fn a_parameter_defaulting_to_a_closure_calls_what_it_was_passed() {
    // `f` has one assignment in the body, its default, but the caller's
    // closure is what it holds when one is passed.
    let source = r#"
function twice(x: int, f: (int) -> int throws never = (q: int) -> int { q * 2 }) -> int {
  f(x)
}
function main() -> int {
  twice(5) * 100 + twice(5, f = (q: int) -> int { q + 1 })
}
"#;
    assert_eq!(run(source).await, Ok(BexExternalValue::Int(1006)));
}

#[tokio::test]
async fn an_error_in_an_inlined_capturing_closure_reports_its_frame() {
    let source = r#"
function main() -> int {
  let limit = 2
  let check = (x: int) -> int {
    if (x > limit) {
      throw "over"
    }
    x
  };
  check(1) + check(5)
}
"#;
    let (mir, _) = lowered(source, "main", OptLevel::Two);
    assert!(mir.contains("[inlined .<lambda(main, 0)>]"), "{mir}");
    let error = run(source).await.unwrap_err();
    insta::assert_snapshot!(error, @r#"
    Traceback (most recent call last):
      File "test.baml", line 10, in user.main
      File "test.baml", line 6, in .<lambda(main, 0)>
    uncaught throw: "over"
    "#);
}

const SHAPES: &str = r#"
interface Shape {
  function area(self) -> int throws never
}
class Square {
  side: int
  implements Shape {
    function area(self) -> int { return self.side * self.side }
  }
}
class Rect {
  w: int
  h: int
  implements Shape {
    function area(self) -> int { return self.w * self.h }
  }
}
function pick(i: int) -> Shape {
  if i % 2 == 0 { return Square { side: i }; };
  return Rect { w: i, h: 3 };
}
function area_of(s: Shape) -> int {
  return s.area()
}
"#;

#[tokio::test]
async fn an_interface_call_on_a_known_class_becomes_its_implementation() {
    // The shape of `polymorphic-dispatch-100k`: `pick` returns one class
    // from each `return`, so each way into the call knows its class.
    let source = format!(
        "{SHAPES}
function entry(n: int) -> int {{
  let acc = 0;
  for (let i = 0; i < n; i += 1) {{
    acc += area_of(pick(i));
  }};
  return acc;
}}
function main() -> int {{ entry(5) }}
"
    );
    let (mir, calls) = lowered(&source, "entry", OptLevel::Two);
    assert_eq!(calls, 0, "{mir}");
    assert!(!mir.contains("virtual_call"), "{mir}");
    assert!(
        mir.contains("user.Square.area") || mir.contains("Square"),
        "{mir}"
    );
    // Each way has its own instance, which is then never built.
    assert!(!mir.contains("user.Square {"), "{mir}");
    assert!(!mir.contains("user.Rect {"), "{mir}");
    // 0 + 1*3 + 4 + 3*3 + 16
    assert_eq!(run(&source).await, Ok(BexExternalValue::Int(32)));
}

#[test]
fn an_interface_call_on_an_unknown_receiver_stays_one() {
    let (mir, _) = lowered(SHAPES, "area_of", OptLevel::Two);
    assert!(mir.contains("virtual_call"), "{mir}");
    assert!(!mir.contains("[inlined"), "{mir}");
}

#[tokio::test]
async fn an_interface_default_body_and_field_are_read_for_the_known_class() {
    // `total` is the interface's own body, `step` each class's, and `base`
    // a field the classes keep at different slots.
    let source = r#"
interface Counter {
  base: int
  function step(self) -> int throws never
  function total(self) -> int throws never {
    return self.base + self.step()
  }
}
class Inc {
  pad: int
  base: int
  implements Counter {
    function step(self) -> int { return 1 }
  }
}
class Dbl {
  start: int
  implements Counter {
    base as start
    function step(self) -> int { return self.start }
  }
}
function entry(i: int) -> int {
  let a = Inc { pad: 100, base: i };
  let b = Dbl { start: i };
  a.total() * 1000 + b.total() * 10 + a.base
}
function main() -> int { entry(4) }
"#;
    let (mir, _) = lowered(source, "entry", OptLevel::Two);
    assert!(!mir.contains("virtual_call"), "{mir}");
    assert!(!mir.contains("virtual_field"), "{mir}");
    // (4 + 1) * 1000 + (4 + 4) * 10 + 4
    assert_eq!(run(source).await, Ok(BexExternalValue::Int(5084)));
}

#[tokio::test]
async fn an_error_in_an_inlined_implementation_reports_its_frame() {
    let source = r#"
interface Checked {
  function check(self) -> int throws string
}

class Limit {
  max: int
  implements Checked {
    function check(self) -> int {
      if (self.max < 0) {
        throw "negative"
      }
      self.max
    }
  }
}

function run_check(c: Checked) -> int {
  c.check()
}

function main() -> int {
  run_check(Limit { max: 3 }) + run_check(Limit { max: -1 })
}
"#;
    let (mir, _) = lowered(source, "main", OptLevel::Two);
    assert!(!mir.contains("virtual_call"), "{mir}");
    let error = run(source).await.unwrap_err();
    insta::assert_snapshot!(error, @r#"
    Traceback (most recent call last):
      File "test.baml", line 23, in user.main
      File "test.baml", line 19, in user.run_check
      File "test.baml", line 11, in user.<(user.Limit as user.Checked)>.check
    uncaught throw: "negative"
    "#);
}

#[tokio::test]
async fn calls_through_function_and_bound_method_values_are_direct() {
    let source = r#"
class Scale {
  by: int
  function apply(self, x: int) -> int { x * self.by }
}
function add_one(x: int) -> int { x + 1 }
function entry(n: int) -> int {
  let step = add_one;
  let scale = Scale { by: 3 };
  let triple = scale.apply;
  step(n) * 100 + triple(n)
}
function main() -> int { entry(4) }
"#;
    let (mir, calls) = lowered(source, "entry", OptLevel::Two);
    assert_eq!(calls, 0, "{mir}");
    assert!(!mir.contains("= call "), "{mir}");
    assert_eq!(run(source).await, Ok(BexExternalValue::Int(512)));
}

#[tokio::test]
async fn a_bound_method_keeps_the_receiver_it_was_bound_to() {
    // `box` is rebound after `get` is made; `get` still reads the first one.
    let source = r#"
class Box {
  value: int
  function get(self) -> int { self.value }
}
function entry(n: int) -> int {
  let box = Box { value: n };
  let get = box.get;
  box = Box { value: n + 100 };
  get() * 1000 + box.value
}
function main() -> int { entry(4) }
"#;
    assert_eq!(run(source).await, Ok(BexExternalValue::Int(4104)));
}

#[tokio::test]
async fn a_loop_test_is_not_hoisted_past_an_inlined_write() {
    // `grow` is inlined into the loop, so its `push` is a write of the array
    // the loop's test reads the length of: the length must be read again on
    // every trip.
    let source = r#"
class Bag {
  items: int[]
}
function grow(bag: Bag, n: int) -> int {
  if (bag.items.length() < n) {
    bag.items.push(1);
  }
  1
}
function main() -> int {
  let bag = Bag { items: [1] };
  let trips = 0;
  let i = 0;
  while (i < bag.items.length()) {
    trips += grow(bag, 5);
    i += 1;
  }
  trips * 10 + bag.items.length()
}
"#;
    assert_eq!(run(source).await, Ok(BexExternalValue::Int(55)));
}

#[tokio::test]
async fn a_panic_in_a_fused_instruction_of_an_inlined_callee_reports_both_frames() {
    // `scaled / bottom` is one fused instruction inside the copy of `ratio`.
    let source = r#"
function ratio(top: int, bottom: int) -> int {
  let scaled = top * 10
  scaled / bottom
}

function main() -> int {
  let total = 0
  for (let i = 2; i >= 0; i -= 1) {
    total += ratio(8, i)
  }
  total
}
"#;
    let db = database(&[("test.baml", source)]);
    let program = baml_db::CheckedWorld::collect(&db, db.workspace_root().unwrap())
        .expect("the world checks")
        .compile(OptLevel::Two)
        .expect("the program compiles");
    let index = baml_tests::engine::function_index(&program, "user.main").unwrap();
    let Some(bex_vm_types::Object::Function(main)) = program.objects.get(index) else {
        panic!("expected function");
    };
    assert!(
        !main.bytecode.inline_frames.is_empty(),
        "`ratio` is inlined"
    );
    let divides_fused = main
        .bytecode
        .instructions
        .iter()
        .enumerate()
        .any(|(pc, op)| {
            matches!(op, bex_vm_types::Instruction::IntArith { .. })
                && main.bytecode.inline_frame_for_pc(pc).is_some()
        });
    assert!(
        divides_fused,
        "no fused instruction inside the inlined call: {:?}",
        main.bytecode.instructions
    );
    let error = run(source).await.unwrap_err();
    insta::assert_snapshot!(error, @"
    Traceback (most recent call last):
      File \"test.baml\", line 10, in user.main
      File \"test.baml\", line 4, in user.ratio
    uncaught throw: baml.panics.DivisionByZero {dividend: 80}
    ");
}
