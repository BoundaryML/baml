//! BEP-066 Scenario 7: live Sessions.

use std::sync::Arc;

use baml_tests::baml_test;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use bex_heap::{CollectionLevel, HeapPermit};
use bex_vm_types::Object;
use sys_native::SysOpsExt;

const BASIC_SESSION: &str = r####"
function main() -> int throws unknown {
  let s = reflect.Session.new()
  s.eval(#"let x = 10"#)
  s.eval<int>(#"x + 1"#)
}
"####;

const S11_LIVENESS_PROBE: &str = r#####"
function escape_one_session_value() -> type throws unknown {
  let dependency = reflect.Package.compile({
    "dep.baml": #"
      class Mounted {
        value int
      }
    "#,
  })
  let s = reflect.Session.new(packages = { "dep": dependency })
  s.eval(#"class Escaped { value string }"#)
  s.eval(#"let first = Escaped { value: "first" }"#)
  s.eval(#"let count = 2"#)
  s.eval(#"let note = "history""#)
  s.eval<type>(#"type.of<Escaped>()"#)
}
"#####;

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
  let y_missing = (s.eval<string>(#"y"#) == "") catch (_) {
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
  let compile_failed = false
  let _ = s.eval(#"let broken: MissingType = null"#) catch (_) {
    baml.reflect.errors.CompilationError => { compile_failed = true },
    _ => null,
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
  let _ = s.eval(#"let bad: MissingType = null"#) catch (e) {
    baml.reflect.errors.CompilationError => {
      let span = e.diagnostics[0].span ?? throw "missing diagnostic span"
      return span.file ?? ""
    },
    _ => return "wrong error",
  }
  "unexpected success"
}

function package_current_is_rejected() -> bool throws unknown {
  let s = reflect.Session.new()
  let _ = s.eval(#"reflect.Package.current()"#) catch (_) {
    baml.reflect.errors.CompilationError => return true,
    _ => return false,
  }
  false
}

function runtime_and_failed_contracts() -> bool throws unknown {
  let s = reflect.Session.new()
  let string_t = type.of<string>()
  let value = s.eval<unreflect(string_t)>(#""ok""#)
  let rejected = (s.eval<string>(#"
    let should_not_exist = 7
    42
  "#) == "") catch (_) {
    baml.reflect.errors.CompilationError => true,
    _ => false,
  }
  let missing = (s.eval<int>(#"should_not_exist"#) == 0) catch (_) {
    baml.reflect.errors.CompilationError => true,
    _ => false,
  }
  type.of_value(value) == string_t && rejected && missing
}

function concurrent_eval_is_busy() -> bool throws unknown {
  let s = reflect.Session.new(packages = { "app": reflect.Package.current() })
  let pending = spawn { s.eval<int>(#"app.Wait()"#) }
  baml.sys.sleep(baml.time.Duration.from_milliseconds(20))
  let busy = (s.eval<int>(#"1"#) == 0) catch (_) {
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
    s.eval<int>(#"app.LongWait()"#)
  }
  baml.sys.sleep(baml.time.Duration.from_milliseconds(20))
  pending.cancel()
  let cancelled = ((await pending) == 0) catch (e) {
    baml.panics.Cancelled => true,
    _ => false,
  }

  cancelled && s.eval<int>(#"baseline + 2"#) == 42
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
    client NeverContacted = openai.OpenAiClient.new(
    model = "no-network-during-declaration",
    api_key = "unused",
    base_url = "http://127.0.0.1:1",
);
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

#[tokio::test]
async fn mutually_recursive_session_lets_diagnose_without_panicking() {
    let output = baml_test!(
        r##"
function main() -> bool throws unknown {
  let s = reflect.Session.new()
  let diagnosed = false
  let _ = s.eval(#"
    let a = b
    let b = a
  "#) catch (e) {
    baml.reflect.errors.CompilationError => {
      diagnosed = e.diagnostics.length() > 0
    },
    _ => null,
  }
  diagnosed && s.eval<int>(#"1"#) == 1
}
"##
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn session_let_named_json_does_not_shadow_json_package_paths() {
    let output = baml_test!(
        r##"
function main() -> bool throws unknown {
  let s = reflect.Session.new()
  s.eval(#"let json = "local""#)
  let local = s.eval<string>(#"json"#)
  let encoded = s.eval<string>(#"json.stringify(7)"#)
  local == "local" && encoded == "7"
}
"##
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn s11_heap_liveness_probe_for_one_escaped_session_value() {
    let program = baml_project::testing::compile_source(S11_LIVENESS_PROBE);
    let engine = Arc::new(
        BexEngine::new_with_runtime_compiler(
            program,
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            bex_project::runtime_compiler(),
        )
        .expect("S-11 liveness probe engine"),
    );
    let escaped = engine
        .call_function(
            "user.escape_one_session_value",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            false,
        )
        .await
        .expect("Session eval should return one escaped value");
    let BexExternalValue::Handle(handle) = escaped else {
        panic!("expected escaped type handle, got {escaped:?}")
    };

    let with_escape_gc = engine.collect_garbage(CollectionLevel::Major).await;
    let inactive = engine.heap_permit_manager().new_permit(()).await;
    let active = inactive.acquire().await;
    let escaped_ptr = engine
        .resolve_handle(active.proof(), &handle)
        .expect("escaped handle should remain live after major GC");
    let Object::Type(escaped_type) = (unsafe { escaped_ptr.get() }) else {
        panic!("escaped handle should point to Object::Type")
    };
    let definition_classes = escaped_type.defs().classes.len();
    let owner_ptr = (!escaped_type.owner.is_null())
        .then_some(escaped_type.owner)
        .or_else(|| {
            escaped_type.defs().classes.values().find_map(|class_ptr| {
                match unsafe { class_ptr.get() } {
                    Object::Class(class) => class
                        .runtime_type
                        .as_ref()
                        .map(|runtime| runtime.owner)
                        .filter(|owner| !owner.is_null()),
                    _ => None,
                }
            })
        });
    let mut owner_is_session = false;
    let mut session_history = 0;
    let mut retained_globals = 0;
    let mut retained_objects = 0;
    let mut retained_dependencies = 0;
    let mut retained_dependency_objects = 0;
    if let Some(owner_ptr) = owner_ptr {
        let Object::Package(owner) = (unsafe { owner_ptr.get() }) else {
            panic!("escaped definition owner should be a Package")
        };
        owner_is_session = owner.session.is_some();
        session_history = owner
            .session
            .as_ref()
            .map_or(0, |state| state.history.len());
        let runtime = owner.runtime.as_ref().expect("runtime owner image");
        retained_globals = runtime.globals.len();
        retained_objects = runtime.objects.len();
        retained_dependencies = runtime.dependencies.len();
        retained_dependency_objects = runtime
            .dependencies
            .iter()
            .map(|dependency| match unsafe { dependency.get() } {
                Object::Package(package) => package
                    .runtime
                    .as_ref()
                    .map_or(0, |runtime| runtime.objects.len()),
                _ => 0,
            })
            .sum::<usize>();
    }
    drop(active);
    drop(handle);
    let without_escape_gc = engine.collect_garbage(CollectionLevel::Major).await;
    let escaped_graph_objects = with_escape_gc
        .live_count
        .saturating_sub(without_escape_gc.live_count);
    eprintln!(
        "S11_LIVENESS with_escape_live={} with_escape_collected={} \
         without_escape_live={} escaped_graph_objects={} definition_classes={} \
         owner_is_session={} session_history={} retained_globals={} retained_objects={} \
         retained_dependencies={} retained_dependency_objects={}",
        with_escape_gc.live_count,
        with_escape_gc.collected_count,
        without_escape_gc.live_count,
        escaped_graph_objects,
        definition_classes,
        owner_is_session,
        session_history,
        retained_globals,
        retained_objects,
        retained_dependencies,
        retained_dependency_objects,
    );

    assert!(
        !owner_is_session,
        "an escaped eval value must not retain its Session owner"
    );
    assert_eq!(
        (
            session_history,
            retained_globals,
            retained_dependencies,
            retained_dependency_objects,
        ),
        (0, 0, 0, 0),
        "the escaped value retained Session history, globals, or mounted Package state"
    );
    assert!(
        definition_classes <= 1 && retained_objects <= 4 && escaped_graph_objects <= 4,
        "the escaped value retained more than its own bounded definition closure"
    );
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
