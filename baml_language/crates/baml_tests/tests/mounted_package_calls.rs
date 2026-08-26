//! BEP-066 mounted-package linking: CALLS into MOUNTED (source-less) packages, end to end —
//! check → MIR → emit → link → run.
//!
//! # The two-database linking harness
//!
//! The LIBRARY database compiles the library's source under
//! `<builtin>/app/…` (a source-bearing `Dependency` root, so `file_package`
//! assigns the files the package name `app`). Two artifacts are captured:
//!
//!   1. the CHECK artifact — `borsh(PackageInterface)`, the typed surface a
//!      consumer checks against, and
//!   2. the RUN artifact — the library's independently emitted symbolic
//!      `CompilationUnit` set (its files ride the builtin emit group, so the
//!      linked image is `stdlib ++ app`, a user-independent prefix exactly
//!      like the stdlib slice).
//!
//! The CONSUMER database is a fresh `ProjectDatabase` with NO `app`
//! source anywhere. The blob is mounted via `set_mounted_packages` (checking
//! resolves `app.…` through the interface rows and records loc-free
//! `MemberResolution::External` callees), and bytecode is generated with
//! `generate_project_bytecode_with_mounted_units(consumer_db, library_units)`:
//! the public seam links the units and seeds emit from that prefix, so the
//! consumer's symbolic references (`app.add`, `app.Widget`, the interface
//! method slots) LINK against the library's already-compiled definitions by
//! fully-qualified name, matching the runtime linker's name-keyed resolution.
//!
//! The resulting single `Program` runs on the ordinary engine harness.

use baml_base::Name;
use baml_compiler2_emit::{OptLevel, emit_units, generate_project_bytecode_with_mounted_units};
use baml_compiler2_hir::package::PackageId;
use baml_compiler2_hir_ty::package_interface::package_interface;
use baml_db::{ProjectDatabase, collect_diagnostics, testing::assert_no_diagnostic_errors};
use baml_tests::engine::{TestDbExt, run_compiled};
use bex_engine::BexExternalValue;
use bex_vm_types::{CompilationUnit, Program};
use indexmap::IndexMap;

// ── The library fixture ─────────────────────────────────────────────────────

/// The mounted library: free functions (plain, generic, throwing), classes
/// with plain and `implements`-block methods, interfaces with concrete and
/// blanket blob impls, and an enum — everything the consumer tests call into.
const LIB: &str = r#"
interface Describable {
    function describe(self) -> string throws never
}

interface Mergeable {
    function merge(self, other: Self) -> Self throws never
}

interface Tagged {
    function tag(self) -> string throws never
}

class Widget {
    x int
    label string

    function get_x(self) -> int throws never {
        self.x
    }

    implements Describable {
        function describe(self) -> string throws never {
            self.label
        }
    }
}

class Bag {
    n int

    implements Mergeable {
        function merge(self, other: Self) -> Self throws never {
            Bag { n: self.n + other.n }
        }
    }
}

class Plain {
    text string
}

implement Describable for Plain {
    function describe(self) -> string throws never {
        self.text
    }
}

implement<T> Tagged for T {
    function tag(self) -> string throws never {
        "any"
    }
}

class ParseError {
    message string
}

function add(a: int, b: int) -> int throws never {
    a + b
}

function apply_callback(cb: (x: int) -> int) -> int {
    cb(41)
}

function widget_x(w: Widget) -> int throws never {
    w.x
}

function merge_both<T extends Mergeable>(a: T, b: T) -> T throws never {
    a.merge(b)
}

function tag_of<T extends Tagged>(value: T) -> string throws never {
    value.tag()
}

function parse_positive(n: int) -> int throws ParseError {
    if n < 0 {
        throw ParseError { message: "negative" }
    }
    n
}
"#;

const OPT: OptLevel = OptLevel::One;

/// Compile the library under `<builtin>/app/` and capture both
/// artifacts — the `borsh(PackageInterface)` blob (check surface) and the
/// symbolic `CompilationUnit` set (run surface; links to `stdlib ++ app`, a
/// user-independent prefix image).
fn compile_library() -> (Vec<u8>, Vec<CompilationUnit>) {
    let mut db = ProjectDatabase::new();
    db.workspace(std::path::Path::new("/mounted-calls"));
    db.dependency("app");
    db.file("<builtin>/app/lib.baml", LIB);
    assert_no_diagnostic_errors(&db);

    let iface = package_interface(&db, PackageId::new(&db, Name::new("app")));
    assert!(
        matches!(
            iface.lookup_type(&[], &Name::new("Widget$stream")),
            Some(baml_compiler2_hir_ty::package_interface::ExportedType::Class { .. })
        ),
        "canonical PPIR companions must be part of the mounted export surface"
    );
    let blob = borsh::to_vec(iface).expect("serialize app interface");

    let units = emit_units(&db, OPT).expect("library fixture emits units");
    (blob, units)
}

/// Build a fresh consumer database — blob mounted as `app`, NO `app`
/// source — checked clean, then spliced against the library image.
fn consumer_program(user_src: &str) -> Program {
    let (blob, lib_units) = compile_library();
    let mut db = ProjectDatabase::new();
    db.workspace(std::path::Path::new("/mounted-calls"));
    db.set_mounted_packages([("app".to_string(), blob)].into());
    db.file("main.baml", user_src);
    assert_no_diagnostic_errors(&db);

    generate_project_bytecode_with_mounted_units(&db, OPT, &lib_units)
        .expect("consumer compiles against the mounted blob")
}

/// Compile the consumer and run `entry` (no arguments), returning the result.
async fn run_consumer(user_src: &str, entry: &str) -> Result<BexExternalValue, String> {
    let program = consumer_program(user_src);
    let output = run_compiled(program, entry, IndexMap::new(), false).await;
    output.result.map_err(|e| format!("{e:?}"))
}

#[track_caller]
fn assert_ok(result: Result<BexExternalValue, String>, expected: BexExternalValue) {
    match result {
        Ok(value) => assert_eq!(value, expected),
        Err(e) => panic!("execution failed: {e}"),
    }
}

// ── Free functions ──────────────────────────────────────────────────────────

/// `app.add(1, 2)` — a mounted free function called directly, its result used.
#[tokio::test]
async fn mounted_free_function_call_runs() {
    let result = run_consumer(
        r#"
function main() -> int throws never {
    app.add(1, 2)
}
"#,
        "main",
    )
    .await;
    assert_ok(result, BexExternalValue::Int(3));
}

/// A mounted free function used as a value (`use_f(app.add)`) — the reference
/// links to the library's function object.
#[tokio::test]
async fn mounted_free_function_reference_runs() {
    let result = run_consumer(
        r#"
function apply(f: (a: int, b: int) -> int, a: int, b: int) -> int {
    f(a, b)
}

function main() -> int {
    apply(app.add, 20, 22)
}
"#,
        "main",
    )
    .await;
    assert_ok(result, BexExternalValue::Int(42));
}

/// Omitted callback throws elaborates a synthetic effect parameter into the
/// exported signature. It participates in call inference but is erased by
/// `RuntimeGenericLayout`, so the cross-image frame remains aligned.
#[tokio::test]
async fn mounted_callback_effect_param_does_not_shift_runtime_frame() {
    let result = run_consumer(
        r#"
function plus_one(x: int) -> int throws never {
    x + 1
}

function main() -> int throws never {
    app.apply_callback(plus_one)
}
"#,
        "main",
    )
    .await;
    assert_ok(result, BexExternalValue::Int(42));
}

// ── Mounted classes: construction, fields, methods ──────────────────────────

/// Construct a mounted class, read a field, pass the value back into a
/// mounted free function, and call a plain method on it.
#[tokio::test]
async fn mounted_class_construction_fields_and_methods_run() {
    let result = run_consumer(
        r#"
function main() -> int throws never {
    let w = app.Widget { x: 40, label: "w" }
    w.x + app.widget_x(w) + w.get_x() - 2 * 40 + 2
}
"#,
        "main",
    )
    .await;
    assert_ok(result, BexExternalValue::Int(42));
}

/// An `implements`-block method on a mounted class value — dispatched through
/// the interface slot, resolved by the VM against the library's registered
/// impl rule.
#[tokio::test]
async fn mounted_class_implements_method_dispatches() {
    let result = run_consumer(
        r#"
function main() -> string throws never {
    let w = app.Widget { x: 1, label: "hello" }
    w.describe()
}
"#,
        "main",
    )
    .await;
    assert_ok(result, BexExternalValue::String("hello".into()));
}

/// An out-of-body implementation exported only through `ExportedImpl.methods`
/// is a concrete member candidate in the source-less consumer.
#[tokio::test]
async fn mounted_out_of_body_impl_method_dispatches() {
    let result = run_consumer(
        r#"
function main() -> string throws never {
    app.Plain { text: "out-of-body" }.describe()
}
"#,
        "main",
    )
    .await;
    assert_ok(result, BexExternalValue::String("out-of-body".into()));
}

// ── Interface dispatch through blob impls ───────────────────────────────────

/// `x.merge(y)` where the impl row is mounted (`implements Mergeable` in the
/// blob) — virtual dispatch lands on the library's compiled override.
#[tokio::test]
async fn interface_dispatch_through_mounted_impl_runs() {
    let result = run_consumer(
        r#"
function main() -> int throws never {
    let a = app.Bag { n: 40 }
    let b = app.Bag { n: 2 }
    let merged = a.merge(b)
    merged.n
}
"#,
        "main",
    )
    .await;
    assert_ok(result, BexExternalValue::Int(42));
}

/// Dispatch through a mounted interface on an interface-typed (existential)
/// receiver.
#[tokio::test]
async fn mounted_interface_existential_dispatch_runs() {
    let result = run_consumer(
        r#"
function main() -> string throws never {
    let d: app.Describable = app.Widget { x: 1, label: "via-existential" }
    d.describe()
}
"#,
        "main",
    )
    .await;
    assert_ok(result, BexExternalValue::String("via-existential".into()));
}

// ── Generic mounted functions ───────────────────────────────────────────────

/// A generic mounted function instantiated at a mounted type, its bound
/// (`T extends Mergeable`) satisfied by a blob impl.
#[tokio::test]
async fn generic_mounted_function_with_blob_impl_bound_runs() {
    let result = run_consumer(
        r#"
function main() -> int throws never {
    let merged = app.merge_both(app.Bag { n: 2 }, app.Bag { n: 40 })
    merged.n
}
"#,
        "main",
    )
    .await;
    assert_ok(result, BexExternalValue::Int(42));
}

/// A generic mounted function instantiated at a USER type, the bound
/// satisfied by the blob's blanket impl (`implement<T> Tagged for T`).
#[tokio::test]
async fn generic_mounted_function_at_user_type_runs() {
    let result = run_consumer(
        r#"
class Mine {
    v int
}

function main() -> string throws never {
    app.tag_of(Mine { v: 1 })
}
"#,
        "main",
    )
    .await;
    assert_ok(result, BexExternalValue::String("any".into()));
}

/// A direct member call on a user type supplied by the mounted blanket impl.
/// This proves the TIR candidate path consumes loc-free impl method rows; the
/// generic mounted-function test above only proves bound satisfaction.
#[tokio::test]
async fn mounted_blanket_impl_method_on_user_type_dispatches() {
    let result = run_consumer(
        r#"
class Mine {
    v int
}

function main() -> string throws never {
    Mine { v: 1 }.tag()
}
"#,
        "main",
    )
    .await;
    assert_ok(result, BexExternalValue::String("any".into()));
}

// ── Canonical streaming companions ─────────────────────────────────────────

/// PPIR-generated `$stream` types are ordinary exported rows. A declarative
/// LLM function in the fresh consumer therefore synthesizes its return
/// companion through `app.Widget$stream` without dependency source.
#[test]
fn mounted_stream_companion_supports_consumer_llm_expansion() {
    let (blob, _) = compile_library();
    let mut db = ProjectDatabase::new();
    db.workspace(std::path::Path::new("/mounted-calls"));
    db.set_mounted_packages([("app".to_string(), blob)].into());
    db.file(
        "main.baml",
        r##"
client Dummy = openai.ResponsesClient.new(
    model = "gpt-4o",
    api_key = "test",
);

function Ask() -> app.Widget {
    client: Dummy
    prompt: `Return a widget`
}
"##,
    );
    assert_no_diagnostic_errors(&db);
}

// ── Throws across the mount boundary ────────────────────────────────────────

/// A mounted function that THROWS: the blob's throw-set feeds the caller's
/// checking, and the throw is caught in user code at runtime.
#[tokio::test]
async fn mounted_function_throw_caught_in_user_code() {
    let result = run_consumer(
        r#"
function classify(n: int) -> string {
    let v = app.parse_positive(n) catch (e) {
        let err: app.ParseError => {
            return err.message
        }
    }
    "ok"
}

function main() -> string {
    let fine = classify(7)
    let caught = classify(0 - 5)
    fine + ":" + caught
}
"#,
        "main",
    )
    .await;
    assert_ok(result, BexExternalValue::String("ok:negative".into()));
}

/// A USER generic function bounded by a mounted interface
/// (`T extends app.Mergeable`): the rigid-`Self` receiver resolves the member
/// through the mounted row, and the call dispatches virtually at runtime.
#[tokio::test]
async fn user_generic_bounded_by_mounted_interface_runs() {
    let result = run_consumer(
        r#"
function combine<T extends app.Mergeable>(a: T, b: T) -> T throws never {
    a.merge(b)
}

function main() -> int throws never {
    let merged = combine(app.Bag { n: 30 }, app.Bag { n: 12 })
    merged.n
}
"#,
        "main",
    )
    .await;
    assert_ok(result, BexExternalValue::Int(42));
}

// ── UFCS and the reserved residue ───────────────────────────────────────────

/// UFCS through an `implements`-block method lowers to a virtual call using
/// the explicit first argument as its receiver.
#[tokio::test]
async fn mounted_ufcs_interface_method_dispatches() {
    let result = run_consumer(
        r#"
function main() -> string throws never {
    app.Widget.describe(app.Widget { x: 1, label: "l" })
}
"#,
        "main",
    )
    .await;
    assert_ok(result, BexExternalValue::String("l".into()));
}

/// A mounted compiler/VM builtin has no ordinary bytecode-unit symbol. This
/// is the remaining E0158 residue; derive only the interface artifact because
/// the fixture intentionally has no native implementation to link and run.
#[test]
fn mounted_builtin_call_stays_reserved() {
    let mut lib_db = ProjectDatabase::new();
    lib_db.workspace(std::path::Path::new("/mounted-calls"));
    lib_db.dependency("app");
    lib_db.file(
        "<builtin>/app/native.baml",
        r#"
function native_value() -> int throws never {
    $rust_function
}

function intrinsic_type<T>() -> reflect.Type throws never {
    $compiler_intrinsic
}
"#,
    );
    assert_no_diagnostic_errors(&lib_db);
    let iface = package_interface(&lib_db, PackageId::new(&lib_db, Name::new("app")));
    let blob = borsh::to_vec(iface).expect("serialize builtin app interface");

    let mut db = ProjectDatabase::new();
    db.workspace(std::path::Path::new("/mounted-calls"));
    db.set_mounted_packages([("app".to_string(), blob)].into());
    db.file(
        "main.baml",
        r#"
function main() -> int throws never {
    let reference = app.native_value
    let intrinsic = app.intrinsic_type<int>()
    app.native_value()
}
"#,
    );
    let errors: Vec<String> = collect_diagnostics(&db)
        .iter()
        .filter(|d| matches!(d.severity, baml_compiler_diagnostics::Severity::Error))
        .map(|d| format!("[{}] {}", d.code(), d.message))
        .collect();
    let reserved = errors
        .iter()
        .filter(|error| error.contains("E0158") && error.contains("mounted"))
        .count();
    assert_eq!(
        reserved, 2,
        "ordinary mounts cannot claim native or compiler-intrinsic trust; got:\n{errors:#?}"
    );
}
