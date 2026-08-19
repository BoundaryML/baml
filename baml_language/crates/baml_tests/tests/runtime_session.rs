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

function host_dispatch_session_value(value: baml.ToString) -> string throws never {
  value.to_string()
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
    client NeverContacted = openai.ResponsesClient.new(
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

function session_declaration_mints_are_generative() -> bool throws unknown {
  let left = reflect.Session.new()
  let right = reflect.Session.new()
  left.eval(#"class SameName { value string }"#)
  right.eval(#"class SameName { value string }"#)
  let left_type = left.eval<type>(#"type.of<SameName>()"#)
  let right_type = right.eval<type>(#"type.of<SameName>()"#)
  left_type != right_type
}

function host_dispatch_recovers_session_class_provenance() -> bool throws unknown {
  let s = reflect.Session.new()
  s.eval(#"
    class SessionValue {
      value string
      implements baml.ToString {
        function to_string(self) -> string throws never {
          self.value
        }
      }
    }
  "#)
  let value = s.eval<baml.ToString>(#"SessionValue { value: "from session" }"#)
  host_dispatch_session_value(value) == "from session"
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
async fn identical_declarations_in_two_sessions_have_distinct_mints() {
    let output = baml_test!(
        baml: SCENARIO_7,
        entry: "session_declaration_mints_are_generative"
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn host_interface_dispatch_uses_session_class_provenance() {
    let output = baml_test!(
        baml: SCENARIO_7,
        entry: "host_dispatch_recovers_session_class_provenance"
    );
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
async fn escaped_session_type_retains_provenance_only_while_handle_is_live() {
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
        owner_is_session,
        "a session declaration Type must retain its owning Session for nominal identity"
    );
    assert!(
        session_history >= 1 && retained_globals >= 1 && retained_dependencies == 1,
        "the retained Session provenance graph is incomplete"
    );
    assert!(
        definition_classes == 1 && retained_objects >= 1 && retained_dependency_objects >= 1,
        "the Type must retain its declaration and mounted dependency provenance"
    );
    assert!(
        without_escape_gc.live_count < with_escape_gc.live_count && escaped_graph_objects > 0,
        "dropping the escaped handle must release the retained Session provenance graph"
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

/// A Session's top-level `let` is a binding site, so a fresh literal
/// initializer widens to its base type exactly as `let` in a body does. It used
/// to bind the literal type itself — `let n = 5` gave `n` the type `5`.
///
/// The contract is the clean oracle: `5` accepts a `5`-typed submission and
/// refuses an `int`-typed one, so this flips from accepted to refused at the
/// change and cannot pass for an unrelated reason.
const SESSION_LET_WIDENING: &str = r####"
function main() -> string throws unknown {
  let s = reflect.Session.new()
  s.eval(#"let n = 5"#)
  let _ = s.eval<5>(#"n"#) catch (e) {
    baml.reflect.errors.CompilationError => return e.diagnostics[0].message,
    _ => return "wrong error",
  }
  "accepted"
}
"####;

/// The same widening seen through member resolution, for all three literal
/// kinds at once.
///
/// NOTE ON THE ORACLE: this pins the *type* each message names, which is the
/// binding's and is what changed. It does **not** endorse the message itself —
/// `to_string` is universally available through the sugar road but is not
/// reachable through the member walk these paths use, so "has no member
/// `to_string`" is a second, unrelated gap (filed in MIG_BRIEF). If that gap is
/// fixed, this test should be re-pointed at a genuinely absent member rather
/// than deleted.
const SESSION_LET_WIDENING_MEMBERS: &str = r####"
function probe(s: reflect.Session, source: string) -> string throws unknown {
  let _ = s.eval(source) catch (e) {
    baml.reflect.errors.CompilationError => return e.diagnostics[0].message,
    _ => return "wrong error",
  }
  "unexpected success"
}

function main() -> string throws unknown {
  let s = reflect.Session.new()
  s.eval(#"let n = 5"#)
  s.eval(#"let text = "hi""#)
  s.eval(#"let flag = true"#)
  `${probe(s, #"n.to_string()"#)}
${probe(s, #"text.to_string()"#)}
${probe(s, #"flag.to_string()"#)}`
}
"####;

/// A consequence of widening worth pinning rather than discovering: a match
/// that covered the literal exhaustively no longer covers the widened base.
/// The mirror also holds — a two-arm `true`/`false` match on a `bool` binding
/// is exhaustive where it used to be a match on the literal `true` alone.
const SESSION_LET_WIDENING_EXHAUSTIVENESS: &str = r####"
function probe(s: reflect.Session, source: string) -> string throws unknown {
  let _ = s.eval(source) catch (e) {
    baml.reflect.errors.CompilationError => return e.diagnostics[0].message,
    _ => return "wrong error",
  }
  "accepted"
}

function main() -> string throws unknown {
  let s = reflect.Session.new()
  s.eval(#"let n = 5"#)
  s.eval(#"let flag = true"#)
  `${probe(s, #"match (n) { 5 => "five" }"#)}
${probe(s, #"match (flag) { true => "t", false => "f" }"#)}`
}
"####;

/// Widening changes the binding, not the values that flow through it: the
/// committed value is unchanged, rebinding across submissions still works, and
/// a later submission still reads what the last one wrote.
const SESSION_LET_REBINDING: &str = r####"
function main() -> string throws unknown {
  let s = reflect.Session.new()
  s.eval(#"let n = 5"#)
  s.eval(#"let text = "hi""#)
  s.eval(#"let flag = true"#)
  s.eval(#"n = 7"#)
  s.eval(#"text = "there""#)
  s.eval(#"flag = false"#)
  s.eval<string>(#"`${n}|${text}|${flag}`"#)
}
"####;

/// Widening is unconditional because a Session binding cannot opt out of it: an
/// annotation is not a legal session spelling at all. Ordinary code keeps the
/// precise type through `let n: 5 = 5`; a submission is refused before it gets
/// that far, and that refusal is what this pins.
const SESSION_LET_ANNOTATION_REJECTED: &str = r####"
function main() -> string throws unknown {
  let s = reflect.Session.new()
  let _ = s.eval(#"let n: int = 5"#) catch (e) {
    baml.reflect.errors.CompilationError => return e.diagnostics[0].message,
    _ => return "wrong error",
  }
  "unexpected success"
}
"####;

/// Widening removes literal specificity from the BINDING, not from the values
/// flowing through it: narrowing a widened session binding still works.
const SESSION_LET_NARROWING: &str = r####"
function main() -> string throws unknown {
  let s = reflect.Session.new()
  s.eval(#"let n = 5"#)
  s.eval<string>(#"if (n is 5) { "narrowed" } else { "wide" }"#)
}
"####;

/// MIG_BRIEF 4(b): a method call on a Session `let` binding reached the VM with
/// a NULL receiver. A session binding is an initialized global, not a lexical
/// local, so MIR's "place for this name" lookup correctly found nothing — and
/// several roads turned that into "no receiver". Field access and indexing were
/// unaffected, because those roads already loaded the global.
///
/// The receivers below cover every road the fix touches: a primitive companion
/// method, a container method that lowers to a `Call`, the one that lowers to
/// `Rvalue::Len`, a user class declared in the session, and a reflection handle.
const SESSION_BINDING_METHOD_CALLS: &str = r####"
function main() -> string throws unknown {
  let s = reflect.Session.new()
  s.eval(#"
    class P {
      name string
      function greet(self) -> string throws never {
        "hi " + self.name
      }
    }
  "#)
  s.eval(#"let n = 5"#)
  s.eval(#"let text = "hi""#)
  s.eval(#"let v = ["a", "b"]"#)
  s.eval(#"let m = { "k": 1, "j": 2 }"#)
  s.eval(#"let p = P { name: "ada" }"#)
  s.eval(#"let cls = reflect.class.new("R", { "a": type.of<string>() })"#)
  s.eval<string>(
    #"`${n.abs()}|${text.to_upper_case()}|${v.length()}|${m.length()}|${v.join("-")}|${m.keys().join("-")}|${p.greet()}|${cls.fields()[0].name}`"#
  )
}
"####;

/// The same call inside the submission that introduces the binding — the defect
/// never needed two submissions, so neither does its regression.
const SESSION_BINDING_METHOD_CALL_SAME_SUBMISSION: &str = r####"
function main() -> string throws unknown {
  let s = reflect.Session.new()
  s.eval<string>(#"let v = ["a", "b"]
v.length().to_string()"#)
}
"####;

/// The controls that always worked, kept so a future change cannot fix method
/// dispatch by breaking them: field access on a binding, and indexing one.
const SESSION_BINDING_FIELD_AND_INDEX: &str = r####"
function main() -> string throws unknown {
  let s = reflect.Session.new()
  s.eval(#"class Draft { title string }"#)
  s.eval(#"let draft = Draft { title: "titled" }"#)
  s.eval(#"let items = [7, 8]"#)
  s.eval<string>(#"`${draft.title}|${items[0]}`"#)
}
"####;

/// A `client` declaration lowers to a top-level `let` too, so it took the same
/// defect — in ORDINARY BAML, with no Session anywhere. `MyClient.id()` was a VM
/// internal error (`expected instance, got any`) on canary.
const CLIENT_DECLARATION_METHOD: &str = r####"
client MyClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

function main() -> string throws unknown {
  MyClient.id()
}
"####;

#[tokio::test]
async fn session_top_level_lets_widen_literal_initializers() {
    let output = baml_test!(SESSION_LET_WIDENING);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "submission result has type `int`, which is not a subtype of requested contract `5`"
                .into()
        ))
    );
}

#[tokio::test]
async fn session_let_widening_is_visible_through_member_resolution() {
    let output = baml_test!(SESSION_LET_WIDENING_MEMBERS);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "type `int` has no member `to_string`\n\
             type `string` has no member `to_string`\n\
             type `bool` has no member `to_string`"
                .into()
        ))
    );
}

#[tokio::test]
async fn session_let_widening_moves_match_exhaustiveness_to_the_base_type() {
    let output = baml_test!(SESSION_LET_WIDENING_EXHAUSTIVENESS);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "non-exhaustive match on type int; missing: _\naccepted".into()
        ))
    );
}

#[tokio::test]
async fn session_let_rebinding_across_submissions_is_unaffected() {
    let output = baml_test!(SESSION_LET_REBINDING);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("7|there|false".into()))
    );
}

#[tokio::test]
async fn session_let_annotations_are_still_rejected() {
    let output = baml_test!(SESSION_LET_ANNOTATION_REJECTED);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "invalid pattern type ascription: session bindings do not support patterns or `else`: \
             type ascription not allowed here"
                .into()
        ))
    );
}

#[tokio::test]
async fn session_let_narrowing_still_sees_the_literal() {
    let output = baml_test!(SESSION_LET_NARROWING);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("narrowed".into()))
    );
}

#[tokio::test]
async fn method_calls_on_session_let_bindings_dispatch() {
    let output = baml_test!(SESSION_BINDING_METHOD_CALLS);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("5|HI|2|2|a-b|k-j|hi ada|a".into()))
    );
}

#[tokio::test]
async fn method_calls_on_a_session_binding_work_in_its_own_submission() {
    let output = baml_test!(SESSION_BINDING_METHOD_CALL_SAME_SUBMISSION);
    assert_eq!(output.result, Ok(BexExternalValue::String("2".into())));
}

#[tokio::test]
async fn session_binding_field_access_and_indexing_still_work() {
    let output = baml_test!(SESSION_BINDING_FIELD_AND_INDEX);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("titled|7".into()))
    );
}

#[tokio::test]
async fn client_declaration_methods_dispatch() {
    let output = baml_test!(CLIENT_DECLARATION_METHOD);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("openai/gpt-4o-mini".into()))
    );
}
