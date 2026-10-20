//! Global variables at the engine boundary — what BAML source cannot observe
//! about them, so these stay Rust tests: how a failing `$init` is reported to
//! the host (the engine never exists for a test block to run in), the runtime
//! check under the static initialization order, allocation pressure during
//! `$init`, init order across files independent of load order, and a
//! dependency package's globals. The language semantics live in the corpus
//! (`baml_src/ns_global_variables`).

use std::{path::Path, sync::Arc};

use baml_db::{OptLevel, ProjectDatabase, compile_program};
use baml_tests::engine::TestDbExt;
use bex_engine::{BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder};
use bex_vm_types::Program;
use sys_native::SysOpsExt;

fn engine(program: Program) -> Result<Arc<BexEngine>, EngineError> {
    BexEngine::new_with_runtime_compiler(
        program,
        Arc::new(sys_native::SysOps::native()),
        Vec::new(),
        bex_project::runtime_compiler(),
    )
    .map(Arc::new)
}

async fn call(engine: &Arc<BexEngine>, name: &str) -> BexExternalValue {
    engine
        .call_function(
            name,
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap_or_else(|error| panic!("`{name}` fails: {error}"))
}

fn init_failure(program: Program) -> String {
    match engine(program) {
        Ok(_) => panic!("the engine was created, but `$init` should have failed"),
        Err(EngineError::InitFailed(message)) => message,
        Err(other) => panic!("expected an `$init` failure, got {other:?}"),
    }
}

/// A throw nothing catches (E0183 is only a warning) stops engine creation;
/// the host sees which global failed and the thrown value rendered as BAML,
/// with the traceback through the initializer — not a raw heap pointer.
#[test]
fn an_uncaught_initializer_throw_names_the_global_and_the_value() {
    let program = baml_tests::engine::compile_source_with_opt(
        r#"
function load() -> int throws string {
    throw "config is missing"
}

let CONFIG = load();

function main() -> int {
    CONFIG
}
"#,
        OptLevel::One,
    );
    let message = init_failure(program);
    assert!(
        message.contains("while initializing global `user.CONFIG`"),
        "{message}"
    );
    assert!(
        message.contains(r#"uncaught throw: "config is missing""#),
        "{message}"
    );
    assert!(message.contains("in user.load"), "{message}");
}

/// The static order follows calls the checker can resolve. A read through a
/// virtual interface method is invisible to it, so `EARLY` (first in source
/// order, with no edge to `LATE`) runs first and reads `LATE` before its
/// initializer — which the engine reports by name instead of handing the
/// initializer a placeholder `null` typed as an `int`.
#[test]
fn a_read_before_initialization_through_a_virtual_call_is_reported() {
    let program = baml_tests::engine::compile_source_with_opt(
        r#"
interface Source {
    function value(self) -> int throws never
}

class ReadsLate {
    implements Source {
        function value(self) -> int throws never {
            LATE
        }
    }
}

function read(source: Source) -> int {
    source.value()
}

let EARLY: int = read(ReadsLate {});
let LATE = 41;

function main() -> int {
    EARLY
}
"#,
        OptLevel::One,
    );
    let message = init_failure(program);
    assert!(
        message.contains("global `user.LATE` was read before its initializer ran"),
        "{message}"
    );
    assert!(
        message.contains("while initializing global `user.EARLY`"),
        "{message}"
    );
}

/// Enough allocation during `$init` to cross the collector's budget makes the
/// VM offer a safe point (an early yield). No collector exists until `$init`
/// finishes, so `$init` resumes past it instead of failing engine creation.
#[tokio::test]
async fn an_initializer_survives_allocation_pressure() {
    let program = baml_tests::engine::compile_source_with_opt(
        r#"
function build(count: int) -> float[] {
    let values: float[] = [];
    for (let i = 0; i < count; i += 1) {
        values.push(i * 1.5);
    }
    values
}

let BIG = build(700000);

function main() -> int {
    BIG.length()
}
"#,
        OptLevel::One,
    );
    let engine = engine(program).expect("`$init` survives an early yield");
    assert_eq!(
        call(&engine, "user.main").await,
        BexExternalValue::Int(700000)
    );
}

/// Independent globals initialize in source order — file path, then position
/// — whatever order the files were loaded in. Initializers that append to a
/// shared global make the order observable.
#[tokio::test]
async fn independent_globals_initialize_in_source_order_regardless_of_load_order() {
    let mut db = ProjectDatabase::new();
    let root = db.workspace(Path::new("/globals/order"));
    // Loaded in reverse path order on purpose.
    db.file(
        "/globals/order/b.baml",
        "let B1 = record(\"b1\");\nlet B2 = record(\"b2\");\n",
    );
    db.file(
        "/globals/order/a.baml",
        r#"let LOG: string[] = [];

function record(name: string) -> string {
    LOG.push(name);
    name
}

let A1 = record("a1");

function main() -> string {
    LOG.join(",")
}
"#,
    );
    let program = compile_program(&db, root, OptLevel::One).expect("the package compiles");
    let engine = engine(program).expect("`$init` runs");
    assert_eq!(
        call(&engine, "user.main").await,
        BexExternalValue::String("a1,b1,b2".into())
    );
}

/// A dependency package's globals are initialized by its own `$init`, before
/// its dependents', and a dependent reads them as `dep.NAME`, in an
/// initializer and in a body alike — the same slot the dependency's own
/// functions read.
#[tokio::test]
async fn a_dependency_packages_globals_are_read_through_its_name() {
    let mut db = ProjectDatabase::new();
    let workspace = db.workspace(Path::new("/globals/deps"));
    db.dependency("lib");
    db.file(
        "<builtin>/lib/main.baml",
        r#"let BASE = 40;

function base() -> int {
    BASE
}
"#,
    );
    db.file(
        "/globals/deps/main.baml",
        r#"let DERIVED = lib.BASE + 2;

function main() -> int {
    DERIVED + lib.base() - lib.BASE
}
"#,
    );
    let program = compile_program(&db, workspace, OptLevel::One).expect("the world compiles");
    let engine = engine(program).expect("both packages' `$init`s run");
    assert_eq!(call(&engine, "user.main").await, BexExternalValue::Int(42));
}
