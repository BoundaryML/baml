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

    let engine = BexEngine::new(
        program,
        Arc::new(sys_ops::SysOps::native()),
        None,
        Vec::new(),
    )
    .expect("BexEngine::new");
    engine
        .set_project_db(Arc::clone(&db_handle))
        .expect("set_project_db");
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

    let engine = BexEngine::new(
        program,
        Arc::new(sys_ops::SysOps::native()),
        None,
        Vec::new(),
    )
    .expect("BexEngine::new");
    engine
        .set_project_db(Arc::clone(&db_handle))
        .expect("set_project_db");
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

    let engine = BexEngine::new(
        program,
        Arc::new(sys_ops::SysOps::native()),
        None,
        Vec::new(),
    )
    .expect("BexEngine::new");
    engine
        .set_project_db(Arc::clone(&db_handle))
        .expect("set_project_db");
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

    let engine = BexEngine::new(
        program,
        Arc::new(sys_ops::SysOps::native()),
        None,
        Vec::new(),
    )
    .expect("BexEngine::new");
    engine
        .set_project_db(Arc::clone(&db_handle))
        .expect("set_project_db");
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

    let engine = BexEngine::new(
        program,
        Arc::new(sys_ops::SysOps::native()),
        None,
        Vec::new(),
    )
    .expect("BexEngine::new");
    engine
        .set_project_db(Arc::clone(&db_handle))
        .expect("set_project_db");
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

    let engine = BexEngine::new(
        program,
        Arc::new(sys_ops::SysOps::native()),
        None,
        Vec::new(),
    )
    .expect("BexEngine::new");
    engine
        .set_project_db(Arc::clone(&db_handle))
        .expect("set_project_db");
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

/// Second `add_compile` with a path that already exists in the runtime
/// Salsa input must throw a BAML-side `Unsupported`. The first batch's
/// file remains; the second batch's mutation is reverted by the
/// `RuntimeBatchGuard`.
#[tokio::test]
async fn add_compile_rejects_duplicate_path() {
    let source = r#"
        function main() -> bool {
            let pkg = reflect.Package.new();
            let _ = pkg.add_compile({
                "lib.baml": "function a() -> int { 1 }"
            });
            // Same path again — must throw.
            let _ = pkg.add_compile({
                "lib.baml": "function b() -> int { 2 }"
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

    let engine = BexEngine::new(
        program,
        Arc::new(sys_ops::SysOps::native()),
        None,
        Vec::new(),
    )
    .expect("BexEngine::new");
    engine
        .set_project_db(Arc::clone(&db_handle))
        .expect("set_project_db");
    let engine = Arc::new(engine);

    let result = engine
        .call_function_bound_args(
            "user.entry",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;
    assert_eq!(
        result,
        Ok(bex_engine::BexExternalValue::Bool(true)),
        "entry() should catch the duplicate-path throw"
    );

    // The first batch's file must still be present; the duplicate batch's
    // mutation should have been rolled back, leaving exactly one file.
    let db = db_handle.lock();
    let runtime_files = db
        .compiler2_runtime_files()
        .expect("runtime files input should exist");
    let files = runtime_files.files(&*db);
    assert_eq!(
        files.len(),
        1,
        "after the duplicate-path failure, only the first batch's file should remain"
    );
    assert_eq!(
        files[0].path(&*db).to_string_lossy(),
        "<runtime>/_pkg_0/lib.baml"
    );
}

/// A failed `add_compile` (parse/type error mid-batch) must revert the
/// Salsa runtime files input to its pre-call state. A subsequent successful
/// `add_compile` on the same package must observe only the originally
/// committed files plus its own.
#[tokio::test]
async fn add_compile_failure_reverts_runtime_files() {
    // The second batch references an undefined name, which is a name-
    // resolution error caught by HIR. `add_compile` returns a `LoweringError`,
    // the `RuntimeBatchGuard` reverts the Salsa input.
    let source = r#"
        function main() -> bool {
            let pkg = reflect.Package.new();
            let _ = pkg.add_compile({
                "good.baml": "function a() -> int { 1 }"
            });
            let _ = pkg.add_compile({
                "bad.baml": "function b() -> int { does_not_exist() }"
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

    let engine = BexEngine::new(
        program,
        Arc::new(sys_ops::SysOps::native()),
        None,
        Vec::new(),
    )
    .expect("BexEngine::new");
    engine
        .set_project_db(Arc::clone(&db_handle))
        .expect("set_project_db");
    let engine = Arc::new(engine);

    let result = engine
        .call_function_bound_args(
            "user.entry",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;
    assert_eq!(
        result,
        Ok(bex_engine::BexExternalValue::Bool(true)),
        "entry() should catch the compile-failure throw"
    );

    let db = db_handle.lock();
    let runtime_files = db
        .compiler2_runtime_files()
        .expect("runtime files input should exist");
    let files = runtime_files.files(&*db);
    let paths: Vec<String> = files
        .iter()
        .map(|f: &SourceFile| f.path(&*db).to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        paths,
        vec!["<runtime>/_pkg_0/good.baml".to_string()],
        "after a failed batch, only the first batch's file should remain"
    );
}

/// `add_compile` must lift runtime-defined classes alongside functions and
/// wire them into per-function `aux_object_ptrs` so `AllocInstance` works.
/// This test defines `class Box { x: int }` and `function make_box() -> Box`,
/// asserts `pkg.items["Box"]` is a gen0 `Object::Class`, asserts `make_box`'s
/// aux pool points at it, and confirms the `AllocInstance` operand was
/// rewritten to the aux-pool slot rather than the original program-pool one.
#[tokio::test]
async fn add_compile_lifts_class_and_populates_aux_pool() {
    let source = r#"
        function build() -> reflect.Package {
            reflect.Package.new().add_compile({
                "lib.baml": "class Box { x: int } function make_box() -> Box { Box { x: 7 } }"
            })
        }
        function main() -> bool {
            let _ = build();
            true
        }
    "#;

    let (program, db) = compile_source_with_opt_returning_db(source, OptLevel::One);
    let db_handle = Arc::new(parking_lot::Mutex::new(db));

    let engine = BexEngine::new(
        program,
        Arc::new(sys_ops::SysOps::native()),
        None,
        Vec::new(),
    )
    .expect("BexEngine::new");
    engine
        .set_project_db(Arc::clone(&db_handle))
        .expect("set_project_db");
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

        // `pkg.items["Box"]` must be a gen0 Object::Class.
        let box_ptr = *pkg
            .items
            .get("Box")
            .expect("pkg.items should contain `Box` from the lifted class");
        assert!(
            matches!(box_ptr.get(), Object::Class(_)),
            "pkg.items[\"Box\"] should be Object::Class, got {:?}",
            box_ptr.get()
        );

        // `make_box`'s aux pool must point at the same Box HeapPtr.
        let make_box_ptr = *pkg
            .items
            .get("make_box")
            .expect("pkg.items should contain `make_box`");
        let Object::Function(make_box_fn) = make_box_ptr.get() else {
            panic!("pkg.items[\"make_box\"] is not a Function");
        };
        assert!(
            !make_box_fn.aux_object_ptrs.is_empty(),
            "make_box should have a non-empty aux_object_ptrs (AllocInstance target)"
        );
        let mut found_box_in_aux = false;
        for entry in &make_box_fn.aux_object_ptrs {
            if *entry == box_ptr {
                found_box_in_aux = true;
                break;
            }
        }
        assert!(
            found_box_in_aux,
            "make_box.aux_object_ptrs should contain the gen0 Box class HeapPtr; \
             got {:?}",
            make_box_fn.aux_object_ptrs
        );

        // The AllocInstance instruction's ObjectIndex must be rewritten to
        // point into the aux pool, not the original program-pool index.
        let alloc_idx = make_box_fn
            .bytecode
            .instructions
            .iter()
            .find_map(|i| match i {
                Instruction::AllocInstance { class_obj, .. } => Some(class_obj.into_raw()),
                _ => None,
            })
            .expect("make_box should have an AllocInstance instruction");
        assert!(
            alloc_idx < make_box_fn.aux_object_ptrs.len(),
            "AllocInstance operand {alloc_idx} should be a valid aux-pool index \
             (aux_object_ptrs.len() = {})",
            make_box_fn.aux_object_ptrs.len()
        );
        assert_eq!(
            make_box_fn.aux_object_ptrs[alloc_idx], box_ptr,
            "AllocInstance operand should point at the lifted Box class"
        );
    }
}

/// `set_project_db` takes `&self` (not `&mut self`), so production embedders
/// can wrap the engine in `Arc::new(...)` *before* attaching the database
/// handle. This mirrors how `bex_project::BexProject` wires reflection
/// support: the engine is built, `Arc`-wrapped, then handed off to async
/// consumers; `set_project_db` happens through the `Arc`.
#[tokio::test]
async fn set_project_db_works_after_arc_wrap() {
    let source = r#"
        function main() -> bool {
            let pkg = reflect.Package.new();
            let _ = pkg.add_compile({
                "lib.baml": "function hello() -> int { 42 }"
            });
            true
        }
    "#;

    let (program, db) = compile_source_with_opt_returning_db(source, OptLevel::One);
    let db_handle = Arc::new(parking_lot::Mutex::new(db));

    let engine = Arc::new(
        BexEngine::new(
            program,
            Arc::new(sys_ops::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("BexEngine::new"),
    );
    // The whole point of `OnceLock`: this call works through the `Arc`.
    engine
        .set_project_db(Arc::clone(&db_handle))
        .expect("set_project_db on Arc-wrapped engine");

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
        "main() should succeed because the DB handle is attached"
    );

    // A second `set_project_db` call on the same engine must fail (it's a
    // one-shot operation; the project DB is fixed per engine instance).
    let err_handle = Arc::clone(&db_handle);
    assert!(
        engine.set_project_db(err_handle).is_err(),
        "second set_project_db must return Err (already-set)"
    );
}

/// Calling `add_compile` on an engine that wasn't constructed with
/// `set_project_db` must throw a BAML-side `Unsupported` rather than
/// panicking. `set_project_db` is the production way to wire a
/// reflection-bearing engine; tests omitting it should still get a
/// catchable BAML throw.
#[tokio::test]
async fn add_compile_without_project_db_throws_baml_unsupported() {
    let source = r#"
        function main() -> bool {
            let pkg = reflect.Package.new();
            let _ = pkg.add_compile({
                "lib.baml": "function hello() -> int { 1 }"
            });
            false
        }
        function entry() -> bool {
            main() catch (_e) {
                _ => true
            }
        }
    "#;

    let (program, _db) = compile_source_with_opt_returning_db(source, OptLevel::One);

    // Note: NO call to `engine.set_project_db(...)`. `reflect.Package.new`
    // also needs the project_db (to mint the `_pkg_N` name through the
    // runtime counter), so we expect the throw to surface at `Package.new`
    // — earlier than `add_compile`, but still a BAML throw caught by
    // `entry`. Either way, no Rust panic.
    let engine = Arc::new(
        BexEngine::new(
            program,
            Arc::new(sys_ops::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("BexEngine::new"),
    );

    let result = engine
        .call_function_bound_args(
            "user.entry",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;
    assert_eq!(
        result,
        Ok(bex_engine::BexExternalValue::Bool(true)),
        "entry() should catch the project-db-missing throw"
    );
}
