//! BEP-066 Scenario 7: live Sessions.

use std::sync::Arc;

use baml_tests::{baml_test, engine::TestOutput};
use bex_engine::{BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder};
use bex_heap::{CollectionLevel, HeapPermit};
use bex_vm_types::Object;
use sys_native::SysOpsExt;

fn evaluation_error_value(output: TestOutput) -> BexExternalValue {
    match output.result.unwrap_err() {
        EngineError::UnhandledThrow { value, .. } => {
            let value = match *value {
                BexExternalValue::Union { value, .. } => *value,
                value => value,
            };
            match &value {
                BexExternalValue::Instance { class_name, .. }
                    if class_name == "reflect.errors.EvaluationError" =>
                {
                    value
                }
                _ => panic!("expected EvaluationError, got {value:?}"),
            }
        }
        other => panic!("expected an unhandled EvaluationError, got {other:?}"),
    }
}

#[tokio::test]
async fn session_evaluation_error_preserves_structured_cause_for_host() {
    let output = baml_test!(
        r####"
class AppError {
  code string
}

function fail_cell() -> null throws AppError {
  throw AppError { code: "E42" }
}

function main() -> null {
  let session = reflect.Session.new(packages = { "app": reflect.Package.current() })
  session.eval<null>(`app.fail_cell()`)
}
"####
    );
    let error = evaluation_error_value(output);
    let BexExternalValue::Instance { fields, .. } = error else {
        unreachable!()
    };

    assert!(
        fields
            .get("message")
            .and_then(BexExternalValue::as_string)
            .is_some_and(|message| message.contains("AppError") && message.contains("E42")),
        "EvaluationError message did not describe its cause: {fields:?}"
    );
    let Some(BexExternalValue::Instance {
        class_name,
        fields: cause_fields,
        ..
    }) = fields.get("cause")
    else {
        panic!("EvaluationError did not carry the structured cause: {fields:?}")
    };
    assert_eq!(class_name, "user.AppError");
    assert_eq!(
        cause_fields
            .get("code")
            .and_then(BexExternalValue::as_string),
        Some("E42".into())
    );
}

#[tokio::test]
async fn session_evaluation_error_preserves_string_cause_for_host() {
    let output = baml_test!(
        r####"
function main() -> unknown {
  let session = reflect.Session.new()
  session.eval<unknown>(`throw "cell failed"`)
}
"####
    );
    let error = evaluation_error_value(output);
    let BexExternalValue::Instance { fields, .. } = error else {
        unreachable!()
    };

    assert_eq!(
        fields.get("cause").and_then(BexExternalValue::as_string),
        Some("cell failed".into())
    );
    assert!(
        fields
            .get("message")
            .and_then(BexExternalValue::as_string)
            .is_some_and(|message| message.contains("cell failed")),
        "EvaluationError message did not describe its string cause: {fields:?}"
    );
}

const S11_LIVENESS_PROBE: &str = r#####"
function escape_one_session_value() -> reflect.Type {
  let dependency = reflect.Package.compile({
    "dep.baml": `
      class Mounted {
        value int
      }
    `,
  })
  let s = reflect.Session.new(packages = { "dep": dependency })
  s.eval(`class Escaped { value string }`)
  s.eval(`let first = Escaped { value: "first" }`)
  s.eval(`let count = 2`)
  s.eval(`let note = "history"`)
  s.eval<reflect.Type>(`reflect.Type.of<Escaped>()`)
}
"#####;

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

function Wait() -> int {
  baml.sys.sleep(baml.time.Duration.from_milliseconds(150))
  1
}

function LongWait() -> int {
  baml.sys.sleep(baml.time.Duration.from_milliseconds(5000))
  1
}

function scenario_7() -> bool {
  let s = reflect.Session.new(packages = { "app": reflect.Package.current() })

  // Submission 1: declarations hoist and execute nothing.
  s.eval(`
    class Draft {
      title: string,
      body: string,
    }

    function Polish(d: Draft) -> Draft {
      Draft { title: d.title, body: app.ImprovePost(d.body) }
    }
  `)

  // Submission 2: the binding becomes visible only after its initializer.
  s.eval(`let draft = Draft { title: "Eval in BAML", body: app.LoadNotes() }`)
  let title = s.eval<string>(`Polish(draft).title`)

  // Containment is a committed prefix, not rollback.
  let _ = s.eval(`
    let saved = app.SaveDraft(draft.title, draft.body)
    app.ValidateOrThrow(draft.title, draft.body)
    let approved = true
  `) catch (e) {
    reflect.errors.EvaluationError => null,
    _ => throw e,
  }
  let saved = s.eval<string>(`saved`)
  let approved_missing = s.eval<bool>(`approved`) catch (_) {
    reflect.errors.CompilationError => true,
    _ => false,
  }

  let _ = s.eval(`
    let x = 10
    let checked = app.Validate(-1)
    let y = "hi"
  `) catch (e) {
    reflect.errors.EvaluationError => null,
    _ => throw e,
  }
  let x = s.eval<int>(`x`)
  let y_missing = (s.eval<string>(`y`) == "") catch (_) {
    reflect.errors.CompilationError => true,
    _ => false,
  }

  // Assignment updates the old cell; shadowing allocates a new one.
  s.eval(`let greeting = "hello"`)
  s.eval(``let shout = (name: string) -> { `\${greeting}, \${name}!` }``)
  s.eval(`greeting = "howdy"`)
  let a = s.eval<string>(`shout("Ada")`)
  s.eval(`let greeting = "goodbye"`)
  let b = s.eval<string>(`shout("Ada")`)

  // A failed compile never poisons the Session.
  let compile_failed = false
  let _ = s.eval(`let broken: MissingType = null`) catch (_) {
    reflect.errors.CompilationError => { compile_failed = true },
    _ => null,
  }
  let continued = s.eval<int>(`x + 1`)

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

function diagnostic_submission_name() -> string {
  let s = reflect.Session.new()
  let _ = s.eval(`let bad: MissingType = null`) catch (e) {
    reflect.errors.CompilationError => {
      let span = e.diagnostics[0].span ?? throw "missing diagnostic span"
      return span.file ?? ""
    },
    _ => return "wrong error",
  }
  "unexpected success"
}

function package_current_is_rejected() -> bool {
  let s = reflect.Session.new()
  let _ = s.eval(`reflect.Package.current()`) catch (_) {
    reflect.errors.CompilationError => return true,
    _ => return false,
  }
  false
}

function runtime_and_failed_contracts() -> bool {
  let s = reflect.Session.new()
  let string_t = reflect.Type.of<string>()
  type StringT = unreflect(string_t)
  let value = s.eval<StringT>(`"ok"`)
  let rejected = (s.eval<string>(`
    let should_not_exist = 7
    42
  `) == "") catch (_) {
    reflect.errors.CompilationError => true,
    _ => false,
  }
  let missing = (s.eval<int>(`should_not_exist`) == 0) catch (_) {
    reflect.errors.CompilationError => true,
    _ => false,
  }
  reflect.Type.of_value(value) == string_t && rejected && missing
}

function concurrent_eval_is_busy() -> bool {
  let s = reflect.Session.new(packages = { "app": reflect.Package.current() })
  let pending = spawn { s.eval<int>(`app.Wait()`) }
  baml.sys.sleep(baml.time.Duration.from_milliseconds(20))
  let busy = (s.eval<int>(`1`) == 0) catch (_) {
    reflect.errors.SessionBusy => true,
    _ => false,
  }
  let waited = await pending
  busy && waited == 1 && s.eval<int>(`2`) == 2
}

function cancelled_eval_releases_lease_and_preserves_prefix() -> bool {
  let s = reflect.Session.new(packages = { "app": reflect.Package.current() })
  s.eval(`let baseline = 40`)
  let pending = spawn {
    s.eval<int>(`app.LongWait()`)
  }
  baml.sys.sleep(baml.time.Duration.from_milliseconds(20))
  pending.cancel()
  let cancelled = ((await pending) == 0) catch (e) {
    baml.panics.Cancelled => true,
    _ => false,
  }

  cancelled && s.eval<int>(`baseline + 2`) == 42
}

function declaration_redefinition_keeps_earlier_resolution() -> bool {
  let s = reflect.Session.new()
  s.eval(`function Current() -> int { 1 }`)
  s.eval(`let old = () -> { Current() }`)
  s.eval(`function Current() -> int { 2 }`)
  s.eval<int>(`old()`) == 1 && s.eval<int>(`Current()`) == 2
}

function client_declaration_is_lazy() -> bool {
  let s = reflect.Session.new()
  s.eval(`
    client NeverContacted = openai.ResponsesClient.new(
    model = "no-network-during-declaration",
    api_key = "unused",
    base_url = "http://127.0.0.1:1",
);
  `)
  s.eval<int>(`1`) == 1
}

function runtime_type_binding_persists() -> bool {
  let s = reflect.Session.new()
  let first = s.eval<bool>(`
    type T = unreflect(reflect.Type.of<string>());
    reflect.Type.of<T>() == reflect.Type.of<string>()
  `)
  let later = s.eval<bool>(`reflect.Type.of<T>() == reflect.Type.of<string>()`)
  s.eval(`type T = unreflect(reflect.Type.of<int>());`)
  let rebound = s.eval<bool>(`reflect.Type.of<T>() == reflect.Type.of<int>()`)
  first && later && rebound
}

// A step whose value is typed by a session binding publishes as `unknown`:
// the binding lives only inside the step's own block, and later submissions
// read the value through a global. Narrowing recovers it.
function binding_typed_step_value_leaves_as_unknown() -> bool {
  let s = reflect.Session.new()
  s.eval(`type T = unreflect(reflect.Type.of<int>());`)
  s.eval(`let v = match (1) { let x: T => x, _ => 0 }`)
  let narrowed = s.eval<int>(`match (v) { let n: int => n, _ => -1 }`)
  let unrelated = s.eval<string>(`"plain"`)
  narrowed == 1 && unrelated == "plain"
}

function session_declarations_are_generative() -> bool {
  let left = reflect.Session.new()
  let right = reflect.Session.new()
  left.eval(`class SameName { value string }`)
  right.eval(`class SameName { value string }`)
  let left_type = left.eval<reflect.Type>(`reflect.Type.of<SameName>()`)
  let right_type = right.eval<reflect.Type>(`reflect.Type.of<SameName>()`)
  left_type != right_type
}

function host_dispatch_recovers_session_class_provenance() -> bool {
  let s = reflect.Session.new()
  s.eval(`
    class SessionValue {
      value string
      implements baml.ToString {
        function to_string(self) -> string throws never {
          self.value
        }
      }
    }
  `)
  let value = s.eval<baml.ToString>(`SessionValue { value: "from session" }`)
  host_dispatch_session_value(value) == "from session"
}

function perf_500() -> string {
  let s = reflect.Session.new()
  let i = 0
  let tenth = 1n
  let five_hundredth = 1n
  while (i < 500) {
    let start = baml.time.Instant.now()
    let value = s.eval<int>(`1`)
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

/// Two sessions and a submission compiled on one of them, finished on the
/// other: the assignment commits into a cell only the first owns, so the
/// second refuses the submission. `refused` returns that second session;
/// `control` returns one that never saw a foreign submission.
const REFUSED_SUBMISSION_PROBE: &str = r#####"
function refused() -> reflect.Session {
  let a = reflect.Session.new();
  a.eval(`let n = 5`);
  let b = reflect.Session.new();
  b.eval(`let m = 1`);
  let artifact = a._compile<unknown>(`n = 9`);
  let _ = b._finish<unknown>(artifact) catch (e) {
    baml.errors.InvalidArgument => { null },
  };
  b
}

function control() -> reflect.Session {
  let b = reflect.Session.new();
  b.eval(`let m = 1`);
  b
}
"#####;

/// Everything a load writes into a package, rendered for comparison.
#[derive(Debug, PartialEq, Eq)]
struct PackageShape {
    classes: Vec<String>,
    enums: Vec<String>,
    interfaces: Vec<String>,
    type_aliases: Vec<String>,
    globals: Vec<(String, u32)>,
    impl_rules: usize,
    cells: usize,
    objects: usize,
    history: Vec<String>,
    visible: Vec<String>,
}

async fn session_shape(engine: &Arc<BexEngine>, name: &str) -> PackageShape {
    let value = engine
        .call_function(
            name,
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            false,
        )
        .await
        .expect("the probe returns a session");
    let BexExternalValue::Handle(handle) = value else {
        panic!("expected a session handle, got {value:?}")
    };
    let inactive = engine.heap_permit_manager().new_permit(()).await;
    let active = inactive.acquire().await;
    let wrapper_ptr = engine
        .resolve_handle(active.proof(), &handle)
        .expect("the handle roots the session wrapper");
    // SAFETY: the active permit excludes collection; the handle roots the wrapper.
    let Object::Instance(wrapper) = (unsafe { wrapper_ptr.get() }) else {
        panic!("a Session handle wraps an instance")
    };
    let package_ptr = wrapper
        .load_field(0)
        .as_object_ptr()
        .expect("the wrapper holds its package");
    // SAFETY: the wrapper roots its package payload under the same permit.
    let Object::Package(package) = (unsafe { package_ptr.get() }) else {
        panic!("a Session's payload is a package")
    };
    let names = |table: &indexmap::IndexMap<bex_vm_types::types::LocalName, _>| {
        table.keys().map(ToString::to_string).collect::<Vec<_>>()
    };
    let state = package.session().expect("the payload is a session");
    PackageShape {
        classes: names(&package.classes),
        enums: names(&package.enums),
        interfaces: names(&package.interfaces),
        type_aliases: names(&package.type_aliases),
        globals: package
            .globals
            .iter()
            .map(|(path, slot)| (path.to_string(), *slot))
            .collect(),
        impl_rules: package.impl_rules.values().map(Vec::len).sum(),
        cells: package.slots.own().map_or(0, <[_]>::len),
        objects: package.objects.own().map_or(0, <[_]>::len),
        history: state.history.keys().cloned().collect(),
        visible: state.visible.keys().cloned().collect(),
    }
}

/// A refused submission publishes nothing. The loader builds a submission's
/// whole image before writing any of it, so a session that refuses one is,
/// table for table, a session that never saw it — not one holding the
/// submission's declarations, slots, or objects with its bindings and
/// history unchanged.
#[tokio::test]
async fn a_refused_submission_publishes_nothing() {
    let program = baml_db::testing::compile_source(REFUSED_SUBMISSION_PROBE);
    let engine = Arc::new(
        BexEngine::new_with_runtime_compiler(
            program,
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            bex_project::runtime_compiler(),
        )
        .expect("refused-submission probe engine"),
    );
    let refused = session_shape(&engine, "user.refused").await;
    let control = session_shape(&engine, "user.control").await;
    assert_eq!(refused, control);
    assert!(
        control.globals.iter().any(|(path, _)| path.ends_with("_m")),
        "the control session owns its own binding: {control:?}"
    );
}

#[tokio::test]
async fn escaped_session_type_retains_provenance_only_while_handle_is_live() {
    let program = baml_db::testing::compile_source(S11_LIVENESS_PROBE);
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
    // A type value's edges are its heads, so its declarations — and through
    // their owner back-edges, the session that keeps them alive — are reached
    // by walking them. There is no sidecar to consult.
    let mut declarations = Vec::new();
    escaped_type.ty.visit_heads(&mut |head| {
        if head.is_resolved() {
            declarations.push(head.ptr());
        }
    });
    let definition_classes = declarations
        .iter()
        .filter(|ptr| matches!(unsafe { ptr.get() }, Object::Class(_)))
        .count();
    let owner_ptr = declarations
        .iter()
        .find_map(|ptr| match unsafe { ptr.get() } {
            Object::Class(class) => class.owner.package(),
            Object::Enum(enm) => enm.owner.package(),
            _ => None,
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
        owner_is_session = owner.session().is_some();
        session_history = owner.session().map_or(0, |state| state.history.len());
        retained_globals = owner.slots.own().map_or(0, <[_]>::len);
        retained_objects = owner.objects.own().map_or(0, <[_]>::len);
        retained_dependencies = owner.declared().count();
        retained_dependency_objects = owner
            .declared()
            .map(|dependency| match unsafe { dependency.get() } {
                Object::Package(package) => package.objects.own().map_or(0, <[_]>::len),
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
    let program = baml_db::testing::compile_source(SCENARIO_7);
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
