//! BEP-066 slice 9: live Sessions and Scenario 7.

use std::sync::Arc;

use baml_tests::baml_test;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use sys_native::SysOpsExt;

const BASIC_SESSION: &str = r####"
function main() -> int throws unknown {
  let s = reflect.Session.new()
  s.eval(#"let x = 10"#)
  s.eval<int>(#"x + 1"#)
}
"####;

const SESSION_IMPORTED_FIELD: &str = r####"
function LoadNotes() -> string {
  "raw notes"
}

function inspect() -> unknown throws unknown {
  let s = reflect.Session.new(packages = { "app": reflect.Package.current() })
  s.eval(#"class Draft { title string, body string }"#)
  s.eval(#"let draft = Draft { title: "title", body: app.LoadNotes() }"#)
  s.eval(#"draft.body"#)
}
"####;

const SCENARIO_7: &str = r#####"
class ValidationError {
  message string
}

function ImprovePost(text: string) -> string {
  text + " polished"
}

function LoadNotes() -> string {
  "raw notes"
}

function SaveDraft(title: string, body: string) -> string {
  log.info("SAVE:" + title + ":" + body)
  "draft-1"
}

function ValidateOrThrow(title: string, body: string) -> null throws ValidationError {
  throw ValidationError { message: title + ":" + body }
}

function Validate(n: int) -> int throws ValidationError {
  if (n < 0) {
    throw ValidationError { message: "negative" }
  }
  n
}

function Wait() -> int throws unknown {
  baml.sys.sleep(baml.time.Duration.from_milliseconds(150))
  1
}

function LongWait() -> int throws unknown {
  baml.sys.sleep(baml.time.Duration.from_milliseconds(5000))
  1
}

function scenario_7() -> bool throws unknown {
  let s = reflect.Session.new(packages = { "app": reflect.Package.current() })

  // Submission 1: declarations hoist and execute nothing.
  s.eval(#"
    class Draft {
      title: string,
      body: string,
    }

    function Polish(d: Draft) -> Draft {
      Draft { title: d.title, body: app.ImprovePost(d.body) }
    }
  "#)

  // Submission 2: the binding becomes visible only after its initializer.
  s.eval(#"let draft = Draft { title: "Eval in BAML", body: app.LoadNotes() }"#)
  let title = s.eval<string>(#"Polish(draft).title"#)

  // Containment is a committed prefix, not rollback.
  let _ = s.eval(#"
    let saved = app.SaveDraft(draft.title, draft.body)
    app.ValidateOrThrow(draft.title, draft.body)
    let approved = true
  "#) catch (e) {
    baml.reflect.errors.EvaluationError => null,
    _ => throw e,
  }
  let saved = s.eval<string>(#"saved"#)
  let approved_missing = s.eval<bool>(#"approved"#) catch (_) {
    baml.reflect.errors.CompilationError => true,
    _ => false,
  }

  let _ = s.eval(#"
    let x = 10
    let checked = app.Validate(-1)
    let y = "hi"
  "#) catch (e) {
    baml.reflect.errors.EvaluationError => null,
    _ => throw e,
  }
  let x = s.eval<int>(#"x"#)
  let y_missing = s.eval<string>(#"y"#) catch (_) {
    baml.reflect.errors.CompilationError => true,
    _ => false,
  }

  // Assignment updates the old cell; shadowing allocates a new one.
  s.eval(#"let greeting = "hello""#)
  s.eval(#"let shout = (name: string) -> { `${greeting}, ${name}!` }"#)
  s.eval(#"greeting = "howdy""#)
  let a = s.eval<string>(#"shout("Ada")"#)
  s.eval(#"let greeting = "goodbye""#)
  let b = s.eval<string>(#"shout("Ada")"#)

  // A failed compile never poisons the Session.
  let compile_failed = s.eval(#"let broken: MissingType = null"#) catch (_) {
    baml.reflect.errors.CompilationError => true,
    _ => false,
  }
  let continued = s.eval<int>(#"x + 1"#)

  title == "Eval in BAML" &&
    saved == "draft-1" &&
    approved_missing &&
    x == 10 &&
    y_missing &&
    a == "howdy, Ada!" &&
    b == "howdy, Ada!" &&
    compile_failed &&
    continued == 11
}

function diagnostic_submission_name() -> string throws unknown {
  let s = reflect.Session.new()
  s.eval(#"let bad: MissingType = null"#) catch (_) {
    baml.reflect.errors.CompilationError { diagnostics } => {
      let span = diagnostics[0].span ?? throw "missing diagnostic span"
      span.file ?? ""
    },
    _ => "wrong error",
  }
}

function package_current_is_rejected() -> bool throws unknown {
  let s = reflect.Session.new()
  s.eval(#"reflect.Package.current()"#) catch (_) {
    baml.reflect.errors.CompilationError => true,
    _ => false,
  }
}

function runtime_and_failed_contracts() -> bool throws unknown {
  let s = reflect.Session.new()
  let string_t = type.of<string>()
  let value = s.eval<unreflect(string_t)>(#""ok""#)
  let rejected = s.eval<string>(#"
    let should_not_exist = 7
    42
  "#) catch (_) {
    baml.reflect.errors.CompilationError => true,
    _ => false,
  }
  let missing = s.eval<int>(#"should_not_exist"#) catch (_) {
    baml.reflect.errors.CompilationError => true,
    _ => false,
  }
  type.of_value(value) == string_t && rejected && missing
}

function concurrent_eval_is_busy() -> bool throws unknown {
  let s = reflect.Session.new(packages = { "app": reflect.Package.current() })
  let pending = spawn { s.eval<int>(#"app.Wait()"#) }
  baml.sys.sleep(baml.time.Duration.from_milliseconds(20))
  let busy = s.eval<int>(#"1"#) catch (_) {
    baml.reflect.errors.SessionBusy => true,
    _ => false,
  }
  let waited = await pending
  busy && waited == 1 && s.eval<int>(#"2"#) == 2
}

function cancelled_eval_releases_lease_and_preserves_prefix() -> bool throws unknown {
  let s = reflect.Session.new(packages = { "app": reflect.Package.current() })
  s.eval(#"let baseline = 40"#)
  let pending = spawn {
    s.eval<int>(#"
      let committed = baseline + 1
      app.LongWait()
    "#)
  }
  baml.sys.sleep(baml.time.Duration.from_milliseconds(500))
  pending.cancel()
  let cancelled = (await pending) catch (e) {
    baml.panics.Cancelled => true,
    _ => false,
  }

  cancelled && s.eval<int>(#"baseline + committed"#) == 81
}

function declaration_redefinition_keeps_earlier_resolution() -> bool throws unknown {
  let s = reflect.Session.new()
  s.eval(#"function Current() -> int { 1 }"#)
  s.eval(#"let old = () -> { Current() }"#)
  s.eval(#"function Current() -> int { 2 }"#)
  s.eval<int>(#"old()"#) == 1 && s.eval<int>(#"Current()"#) == 2
}

function client_declaration_is_lazy() -> bool throws unknown {
  let s = reflect.Session.new()
  s.eval(#"
    client<llm> NeverContacted {
      provider openai
      options {
        model "no-network-during-declaration"
        api_key "unused"
        base_url "http://127.0.0.1:1"
      }
    }
  "#)
  s.eval<int>(#"1"#) == 1
}

function runtime_type_binding_persists() -> bool throws unknown {
  let s = reflect.Session.new()
  let first = s.eval<bool>(#"
    type T = unreflect(type.of<string>());
    type.of<T>() == type.of<string>()
  "#)
  let later = s.eval<bool>(#"type.of<T>() == type.of<string>()"#)
  s.eval(#"type T = unreflect(type.of<int>());"#)
  let rebound = s.eval<bool>(#"type.of<T>() == type.of<int>()"#)
  first && later && rebound
}

function perf_500() -> string throws unknown {
  let s = reflect.Session.new()
  let i = 0
  let tenth = 1n
  let five_hundredth = 1n
  while (i < 500) {
    let start = baml.time.Instant.now()
    let value = s.eval<int>(#"1"#)
    let elapsed = start.elapsed().to_nanoseconds()
    if (i == 9) {
      tenth = elapsed
    } else if (i == 499) {
      five_hundredth = elapsed
    }
    if (value != 1) { throw "bad Session perf value" } else { null }
    i = i + 1
  }
  tenth.to_string() + "," + five_hundredth.to_string()
}
"#####;

#[tokio::test]
async fn session_commits_a_binding_and_returns_a_contracted_value() {
    let output = baml_test!(BASIC_SESSION);
    assert_eq!(output.result, Ok(BexExternalValue::Int(11)));
}

#[tokio::test]
async fn session_reimports_instance_fields_without_changing_the_value() {
    let output = baml_test!(baml: SESSION_IMPORTED_FIELD, entry: "inspect");
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("raw notes".into()))
    );
}

#[tokio::test]
async fn scenario_7_verbatim_semantics_and_containment() {
    let output = baml_test!(baml: SCENARIO_7, entry: "scenario_7");
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn compilation_diagnostics_use_stable_submission_names() {
    let output = baml_test!(baml: SCENARIO_7, entry: "diagnostic_submission_name");
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("$submission_0.baml".into()))
    );
}

#[tokio::test]
async fn package_current_is_not_available_in_session_source() {
    let output = baml_test!(baml: SCENARIO_7, entry: "package_current_is_rejected");
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn runtime_contracts_check_before_execution() {
    let output = baml_test!(baml: SCENARIO_7, entry: "runtime_and_failed_contracts");
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn concurrent_session_eval_throws_busy_and_recovers() {
    let output = baml_test!(baml: SCENARIO_7, entry: "concurrent_eval_is_busy");
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn cancelled_session_eval_releases_lease_and_preserves_committed_prefix() {
    let output = baml_test!(
        baml: SCENARIO_7,
        entry: "cancelled_eval_releases_lease_and_preserves_prefix"
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn declaration_redefinition_is_newest_wins_without_relinking_old_code() {
    let output = baml_test!(
        baml: SCENARIO_7,
        entry: "declaration_redefinition_keeps_earlier_resolution"
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn session_client_declarations_do_not_perform_network_io() {
    let output = baml_test!(baml: SCENARIO_7, entry: "client_declaration_is_lazy");
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn scoped_runtime_type_bindings_persist_and_rebind_between_submissions() {
    let output = baml_test!(baml: SCENARIO_7, entry: "runtime_type_binding_persists");
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

fn resident_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                line.strip_prefix("VmRSS:")?
                    .split_whitespace()
                    .next()?
                    .parse()
                    .ok()
            })
        })
        .unwrap_or(0)
}

#[cfg(target_os = "linux")]
fn trim_allocator() {
    // Fresh Salsa databases are intentionally dropped after every eval, but
    // glibc may retain their freed arenas in the process RSS. `malloc_trim`
    // makes this measurement distinguish reachable Session state from the
    // allocator's reusable high-water cache.
    unsafe {
        libc::malloc_trim(0);
    }
}

#[cfg(not(target_os = "linux"))]
fn trim_allocator() {}

#[tokio::test]
async fn five_hundred_evals_have_flat_latency_and_bounded_artifacts() {
    let program = baml_project::testing::compile_source(SCENARIO_7);
    let engine = Arc::new(
        BexEngine::new_with_runtime_compiler(
            program,
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            bex_project::runtime_compiler(),
        )
        .expect("Session perf engine"),
    );
    let before_objects = engine.heap_stats().runtime_objects;
    trim_allocator();
    let before_rss = resident_kib();
    let result = engine
        .call_function(
            "user.perf_500",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("500 Session evals");
    let after_objects = engine.heap_stats().runtime_objects;
    trim_allocator();
    let after_rss = resident_kib();
    let BexExternalValue::String(timing) = result else {
        panic!("unexpected perf result: {result:?}")
    };
    let (tenth, five_hundredth) = timing
        .split_once(',')
        .map(|(a, b)| (a.parse::<u128>().unwrap(), b.parse::<u128>().unwrap()))
        .expect("two timing samples");
    let object_growth = after_objects.saturating_sub(before_objects);
    let rss_growth = after_rss.saturating_sub(before_rss);
    eprintln!(
        "SESSION_PERF eval10_ns={tenth} eval500_ns={five_hundredth} \
         runtime_object_growth={object_growth} rss_growth_kib={rss_growth}"
    );

    // A 100ms floor keeps the wall-clock pin stable on loaded CI machines;
    // above it, the specified 3x relationship is the controlling bound.
    assert!(
        five_hundredth <= (tenth.saturating_mul(3)).max(100_000_000),
        "eval #500 regressed against eval #10: {timing}"
    );
    assert!(
        object_growth < 10_000,
        "500 constant-size submissions retained too many VM objects: {object_growth}"
    );
    assert!(
        rss_growth < 256 * 1024,
        "fresh compiler contexts were retained: RSS grew {rss_growth} KiB"
    );
}
