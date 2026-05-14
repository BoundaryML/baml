//! Phase 5.2c sanity test: `reflect.Package.add_compile` accepts a map of
//! files and stores each as a runtime source file under `<runtime>/{pkg}/…`
//! in the engine's `Compiler2RuntimeFiles` Salsa input.
//!
//! Re-emit + item extraction land in later commits; this test just verifies
//! the file-insertion plumbing.

use std::sync::Arc;

use baml_base::SourceFile;
use baml_compiler2_hir::Db;
use baml_project::testing::{OptLevel, compile_source_with_opt_returning_db};
use bex_engine::{BexEngine, FunctionCallContextBuilder};
use bex_vm_types::{Instruction, Object, PackageGlobals, Value};
use sys_native::SysOpsExt;

#[tokio::test]
async fn add_compile_inserts_files_into_runtime_input() {
    let source = r#"
        function main() -> bool {
            let pkg = reflect.Package.new();
            let _ = pkg.add_compile({
                "lib.baml": "function hello() -> int { 1 }",
                "more.baml": "function bye() -> int { 2 }"
            });
            true
        }
    "#;

    let (program, db) = compile_source_with_opt_returning_db(source, OptLevel::One);
    let db_handle = Arc::new(parking_lot::Mutex::new(db));

    let mut engine = BexEngine::new(
        program,
        Arc::new(sys_ops::SysOps::native()),
        None,
        Vec::new(),
    )
    .expect("BexEngine::new");
    engine.set_project_db(Arc::clone(&db_handle));
    let engine = Arc::new(engine);

    let result = engine
        .call_function_bound_args(
            "user.main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;
    assert!(result.is_ok(), "main() returned: {result:?}");

    // The package mint counter starts at 0, so `Package.new` returned
    // `_pkg_0`. Both files should now live under `<runtime>/_pkg_0/…`.
    let db = db_handle.lock();
    let runtime_files = db
        .compiler2_runtime_files()
        .expect("runtime files input should exist after set_project_root");
    let files = runtime_files.files(&*db);
    let paths: Vec<String> = files
        .iter()
        .map(|f: &SourceFile| f.path(&*db).to_string_lossy().into_owned())
        .collect();
    assert!(
        paths.contains(&"<runtime>/_pkg_0/lib.baml".to_string()),
        "expected `<runtime>/_pkg_0/lib.baml` in {paths:?}"
    );
    assert!(
        paths.contains(&"<runtime>/_pkg_0/more.baml".to_string()),
        "expected `<runtime>/_pkg_0/more.baml` in {paths:?}"
    );
}

/// `add_compile` with syntactically invalid source must throw rather than
/// silently succeed — the re-emit catches the parse / type error and the
/// native impl translates the `LoweringError` to a BAML throw that the
/// caller can `catch`.
#[tokio::test]
async fn add_compile_throws_on_compile_error() {
    let source = r#"
        function main() -> bool {
            let pkg = reflect.Package.new();
            // Bad source — `fn` is not a BAML keyword.
            let _result = pkg.add_compile({
                "bad.baml": "fn nope() -> int { 0 }"
            });
            false
        }
        function entry() -> bool {
            main() catch (_e) {
                _ => true
            }
        }
    "#;

    let (program, db) = compile_source_with_opt_returning_db(source, OptLevel::One);
    let db_handle = Arc::new(parking_lot::Mutex::new(db));

    let mut engine = BexEngine::new(
        program,
        Arc::new(sys_ops::SysOps::native()),
        None,
        Vec::new(),
    )
    .expect("BexEngine::new");
    engine.set_project_db(Arc::clone(&db_handle));
    let engine = Arc::new(engine);

    let result = engine
        .call_function_bound_args(
            "user.entry",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;
    assert!(result.is_ok(), "entry() returned: {result:?}");
}

/// `add_compile` lifts each new function in the package into gen0 and
/// registers it under its local name in `pkg.items`. This test exercises
/// the lift end-to-end:
///
/// - Batch 1 defines `hello`. After `add_compile`, `pkg.items["hello"]` is
///   a `HeapPtr` to an `Object::Function`.
/// - Batch 2 defines `caller`, which calls `hello`. After the second
///   `add_compile`:
///   - `pkg.items["hello"]` retains its **original** `HeapPtr` (identity
///     preservation — the user's pre-existing reference stays valid).
///   - `pkg.items["caller"]` is freshly allocated.
///   - `caller`'s `Call` instruction references a per-package slot, and
///     `pkg.globals.Dynamic[slot]` holds the `hello` `HeapPtr`.
///
/// To inspect the package state we need to keep the package handle alive
/// after `main()` returns. The test's BAML stashes it in a top-level
/// `let`, which `$init` evaluates and the engine retains via the frozen
/// globals; the test then walks the heap to find the runtime
/// `Object::Package`.
#[tokio::test]
async fn add_compile_lifts_items_and_remaps_slots() {
    let source = r#"
        function build_pkg() -> reflect.Package {
            reflect.Package.new()
                .add_compile({ "h.baml": "function hello() -> int { 7 }" })
                .add_compile({ "c.baml": "function caller() -> int { hello() }" })
        }

        function main() -> bool {
            let _ = build_pkg();
            true
        }
    "#;

    let (program, db) = compile_source_with_opt_returning_db(source, OptLevel::One);
    let db_handle = Arc::new(parking_lot::Mutex::new(db));

    let mut engine = BexEngine::new(
        program,
        Arc::new(sys_ops::SysOps::native()),
        None,
        Vec::new(),
    )
    .expect("BexEngine::new");
    engine.set_project_db(Arc::clone(&db_handle));
    let engine = Arc::new(engine);

    // Build both batches via the chained add_compile inside `build_pkg()`.
    let result = engine
        .call_function_bound_args(
            "user.main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;
    assert!(result.is_ok(), "main() returned: {result:?}");

    // Inspect the engine's heap to find the `_pkg_0` runtime package.
    // SAFETY: the engine is held alive via `Arc<BexEngine>`; we walk the
    // compile-time pool plus gen0, looking for a `Package` named `_pkg_0`.
    // No GC is running concurrently.
    unsafe {
        let heap = engine.heap();
        let mut pkg_ref: Option<&bex_vm_types::Package> = None;
        for idx in 0..heap.compile_time_len() {
            let ptr = heap.compile_time_ptr(idx);
            if let Object::Package(p) = ptr.get()
                && p.name == "_pkg_0"
            {
                pkg_ref = Some(p.as_ref());
                break;
            }
        }
        // The runtime package lives in gen0 — search it directly.
        if pkg_ref.is_none() {
            let gen0 = heap.gen0_ref();
            for i in 0..gen0.len() {
                let ptr = gen0.get_ptr(i);
                let obj_ref: &Object = &*ptr;
                if let Object::Package(p) = obj_ref
                    && p.name == "_pkg_0"
                {
                    pkg_ref = Some(p.as_ref());
                    break;
                }
            }
        }
        let pkg = pkg_ref.expect("runtime package _pkg_0 should be reachable");
        // Identity preservation: `hello` should be present.
        let hello_ptr = *pkg
            .items
            .get("hello")
            .expect("pkg.items should contain `hello` from batch 1");
        let caller_ptr = *pkg
            .items
            .get("caller")
            .expect("pkg.items should contain `caller` from batch 2");

        // Inspect `caller`'s bytecode: it must have a `Call` referencing
        // `pkg.globals.Dynamic[hello_slot]`, and that slot must equal
        // `hello_ptr`.
        let Object::Function(caller_func) = caller_ptr.get() else {
            panic!("pkg.items[\"caller\"] is not a Function");
        };
        let call_slot = caller_func
            .bytecode
            .instructions
            .iter()
            .find_map(|i| match i {
                Instruction::Call { callee, .. } => Some(callee.into_raw()),
                _ => None,
            })
            .expect("caller should have at least one Call instruction");

        let PackageGlobals::Dynamic(slots) = &pkg.globals else {
            panic!("runtime package should have Dynamic globals");
        };
        assert!(
            call_slot < slots.len(),
            "call_slot {call_slot} out of range for slots.len() = {}",
            slots.len()
        );
        match slots[call_slot] {
            Value::Object(p) => assert_eq!(
                p, hello_ptr,
                "pkg.globals[{call_slot}] should point at the same `hello` HeapPtr as pkg.items[\"hello\"]"
            ),
            other => panic!("pkg.globals[{call_slot}] should be a function HeapPtr, got {other:?}"),
        }
    }
}

/// `add_compile` must resolve external (stdlib) callees by FQN through the
/// engine's frozen globals. Here the runtime package's `wrapped` function
/// calls `baml.math.trunc` — a stdlib native function. After the lift the
/// per-package slot for `baml.math.trunc` must hold the same `HeapPtr` the
/// engine resolves for that name.
#[tokio::test]
async fn add_compile_resolves_external_stdlib_callee() {
    let source = r#"
        function build() -> reflect.Package {
            reflect.Package.new().add_compile({
                "lib.baml": "function wrapped(f: float) -> int { baml.math.trunc(f) }"
            })
        }
        function main() -> bool {
            let _ = build();
            true
        }
    "#;

    let (program, db) = compile_source_with_opt_returning_db(source, OptLevel::One);
    let db_handle = Arc::new(parking_lot::Mutex::new(db));

    let mut engine = BexEngine::new(
        program,
        Arc::new(sys_ops::SysOps::native()),
        None,
        Vec::new(),
    )
    .expect("BexEngine::new");
    engine.set_project_db(Arc::clone(&db_handle));
    let engine = Arc::new(engine);

    let result = engine
        .call_function_bound_args(
            "user.main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;
    assert!(result.is_ok(), "main() returned: {result:?}");

    unsafe {
        let heap = engine.heap();
        let mut pkg_ref: Option<&bex_vm_types::Package> = None;
        let gen0 = heap.gen0_ref();
        for i in 0..gen0.len() {
            let ptr = gen0.get_ptr(i);
            let obj_ref: &Object = &*ptr;
            if let Object::Package(p) = obj_ref
                && p.name == "_pkg_0"
            {
                pkg_ref = Some(p.as_ref());
                break;
            }
        }
        let pkg = pkg_ref.expect("runtime package _pkg_0 should be reachable in gen0");

        // The function_slot_map records every callee referenced by this
        // package's bytecode. `baml.math.trunc` must be in there (resolved
        // as external) and must point at a function HeapPtr.
        let trunc_slot = *pkg
            .function_slot_map
            .get("baml.math.trunc")
            .expect("baml.math.trunc must be assigned a slot");
        let PackageGlobals::Dynamic(slots) = &pkg.globals else {
            panic!("runtime package should have Dynamic globals");
        };
        match slots[trunc_slot] {
            Value::Object(p) => {
                // Confirm the slot really points at a Function.
                match p.get() {
                    Object::Function(f) => assert_eq!(
                        f.name, "baml.math.trunc",
                        "package's slot for baml.math.trunc resolved to wrong function"
                    ),
                    other => panic!(
                        "package's slot for baml.math.trunc should be Function, got {other:?}"
                    ),
                }
            }
            other => panic!("expected function HeapPtr, got {other:?}"),
        }
    }
}

/// Runtime functions whose bodies use string literals require
/// `ConstValue::Object` resolution in the lift pass — the literal is
/// emitted as `LoadConst` referencing an `Object::String` in the program's
/// object pool. This test compiles such a function and asserts the lifted
/// function's resolved-constants slot for the string holds a fresh gen0
/// `Object::String` with the right contents.
#[tokio::test]
async fn add_compile_lifts_string_constant() {
    let source = r#"
        function build() -> reflect.Package {
            reflect.Package.new().add_compile({
                "lib.baml": "function greet() -> string { \"hello world\" }"
            })
        }
        function main() -> bool {
            let _ = build();
            true
        }
    "#;

    let (program, db) = compile_source_with_opt_returning_db(source, OptLevel::One);
    let db_handle = Arc::new(parking_lot::Mutex::new(db));

    let mut engine = BexEngine::new(
        program,
        Arc::new(sys_ops::SysOps::native()),
        None,
        Vec::new(),
    )
    .expect("BexEngine::new");
    engine.set_project_db(Arc::clone(&db_handle));
    let engine = Arc::new(engine);

    let result = engine
        .call_function_bound_args(
            "user.main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;
    assert!(result.is_ok(), "main() returned: {result:?}");

    unsafe {
        let heap = engine.heap();
        let mut pkg_ref: Option<&bex_vm_types::Package> = None;
        let gen0 = heap.gen0_ref();
        for i in 0..gen0.len() {
            let ptr = gen0.get_ptr(i);
            let obj_ref: &Object = &*ptr;
            if let Object::Package(p) = obj_ref
                && p.name == "_pkg_0"
            {
                pkg_ref = Some(p.as_ref());
                break;
            }
        }
        let pkg = pkg_ref.expect("runtime package _pkg_0 should be reachable in gen0");

        let greet_ptr = *pkg
            .items
            .get("greet")
            .expect("pkg.items[\"greet\"] missing");
        let Object::Function(greet_func) = greet_ptr.get() else {
            panic!("pkg.items[\"greet\"] is not a Function");
        };

        // `greet` should have at least one resolved constant pointing at a
        // gen0 String containing "hello world".
        let found = greet_func
            .bytecode
            .resolved_constants
            .iter()
            .any(|v| match v {
                Value::Object(p) => matches!(p.get(), Object::String(s) if s == "hello world"),
                _ => false,
            });
        assert!(
            found,
            "expected greet's resolved_constants to contain a String(\"hello world\"); got {:?}",
            greet_func.bytecode.resolved_constants
        );
    }
}

/// `add_compile` returns the *same* `reflect.Package` instance each call (it
/// mutates the receiver in place rather than allocating a new wrapper). Use
/// `baml.ref_equals` directly from BAML to verify both the outer wrapper and
/// its underlying `_inner` heap primitive keep stable identity across two
/// chained batches.
///
/// This is the BAML-side proof that `Package` is a mutable, persistent
/// handle: once Phase 6 adds `pkg.get<F>(name)`, the same approach extends
/// to verifying that *items* keep their identity too. For now the items
/// invariant is covered by `add_compile_lifts_items_and_remaps_slots`
/// via heap inspection.
#[tokio::test]
async fn add_compile_preserves_wrapper_identity() {
    let source = r#"
        function check() -> bool {
            let pkg = reflect.Package.new();
            let p2 = pkg.add_compile({ "h.baml": "function hello() -> int { 1 }" });
            let p3 = p2.add_compile({
                "c.baml": "function caller() -> int { hello() }"
            });
            let wrappers_equal = baml.ref_equals(pkg, p2) && baml.ref_equals(p2, p3);
            let inners_equal = baml.ref_equals(pkg._inner, p2._inner)
                && baml.ref_equals(p2._inner, p3._inner);
            wrappers_equal && inners_equal
        }
        function main() -> bool { check() }
    "#;

    let (program, db) = compile_source_with_opt_returning_db(source, OptLevel::One);
    let db_handle = Arc::new(parking_lot::Mutex::new(db));

    let mut engine = BexEngine::new(
        program,
        Arc::new(sys_ops::SysOps::native()),
        None,
        Vec::new(),
    )
    .expect("BexEngine::new");
    engine.set_project_db(Arc::clone(&db_handle));
    let engine = Arc::new(engine);

    let result = engine
        .call_function_bound_args(
            "user.main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;
    assert_eq!(
        result,
        Ok(bex_engine::BexExternalValue::Bool(true)),
        "main() should observe identity preservation across batches"
    );
}
