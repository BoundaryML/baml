//! Runtime IDs cannot be asserted in BAML: inspect the final heap objects and
//! engine-owned metadata across dynamic linking, GC, and serialization.

use std::{collections::HashSet, sync::Arc};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, FunctionId};
use bex_heap::{CollectionLevel, HeapPermit};
use bex_vm_types::{Function, Object};
use sys_native::SysOpsExt;

fn engine(source: &str) -> Arc<BexEngine> {
    from_program(baml_tests::stdlib_prefix::compile_source(source))
}

fn from_program(program: bex_vm_types::Program) -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new_with_runtime_compiler(
            program,
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            bex_project::runtime_compiler(),
        )
        .unwrap(),
    )
}

async fn call(
    engine: &Arc<BexEngine>,
    name: &str,
    args: Vec<BexExternalValue>,
) -> BexExternalValue {
    engine
        .call_function(
            name,
            args,
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            false,
        )
        .await
        .unwrap()
}

async fn package_functions(
    engine: &BexEngine,
    value: &BexExternalValue,
) -> Vec<(FunctionId, String)> {
    let BexExternalValue::Handle(handle) = value else {
        panic!("expected package handle: {value:?}")
    };
    let inactive = engine.heap_permit_manager().new_permit(()).await;
    let active = inactive.acquire().await;
    let pointer = engine.resolve_handle(active.proof(), handle).unwrap();
    // SAFETY: the active permit prevents collection; the handle roots the wrapper.
    let Object::Instance(wrapper) = (unsafe { pointer.get() }) else {
        panic!("not a package wrapper")
    };
    let pointer = wrapper
        .load_field(0)
        .as_object_ptr()
        .expect("package payload");
    // SAFETY: the wrapper roots its package payload under the same permit.
    let Object::Package(package) = (unsafe { pointer.get() }) else {
        panic!("not a package")
    };
    package
        .runtime()
        .unwrap()
        .objects
        .iter()
        .filter_map(|pointer| {
            // SAFETY: the owning package roots its objects under the same permit.
            match unsafe { pointer.get() } {
                Object::Function(function) => Some((
                    function
                        .telemetry_function_id
                        .expect("identity assigned before execution"),
                    function.name.clone(),
                )),
                _ => None,
            }
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn package_compile_remints_owned_functions_but_preserves_imports_and_gc_identity() {
    let engine = engine(
        r#"
function make() -> reflect.Package {
  reflect.Package.compile({"leaf.baml": "function identity_leaf() -> int { let f = () => { 7 }; f() }"})
}
function dependent(dep: reflect.Package) -> reflect.Package {
  reflect.Package.compile({"use.baml": "function use_leaf() -> int { dep.identity_leaf() }"}, packages = {"dep": dep})
}
function invoke(pkg: reflect.Package) -> int {
  let f = pkg.get_function<() -> int>("root.use_leaf") ?? throw "missing function"
  f()
}
"#,
    );
    let (first, second) = tokio::join!(
        call(&engine, "user.make", vec![]),
        call(&engine, "user.make", vec![])
    );
    let first_functions = package_functions(&engine, &first).await;
    let second_functions = package_functions(&engine, &second).await;
    let first_lambda = first_functions
        .iter()
        .find(|(_, name)| name.contains("<lambda"))
        .expect("lambda body has an identity");
    let second_lambda = second_functions
        .iter()
        .find(|(_, name)| name == &first_lambda.1)
        .expect("same lambda template");
    assert_ne!(
        first_lambda.0, second_lambda.0,
        "separate grafts have separate lambda bodies"
    );
    let leaf = |rows: &[(FunctionId, String)]| {
        rows.iter()
            .find(|(_, name)| name.ends_with(".identity_leaf"))
            .unwrap()
            .0
    };
    assert_ne!(
        leaf(&first_functions),
        leaf(&second_functions),
        "same source is two runtime objects"
    );
    // Unregistered definitions can move before their first observation too.
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(package_functions(&engine, &first).await, first_functions);
    let dependent = call(&engine, "user.dependent", vec![first.clone()]).await;
    let dependent_functions = package_functions(&engine, &dependent).await;
    assert!(
        dependent_functions
            .iter()
            .any(|(id, _)| *id == leaf(&first_functions)),
        "import reuses the original ID"
    );
    assert!(
        engine
            .function_metadata(leaf(&first_functions))
            .await
            .is_none()
    );
    assert!(
        engine
            .function_metadata(leaf(&second_functions))
            .await
            .is_none()
    );
    // Concurrent first observations must publish once, before either call path
    // can refer to the definition. Independent VMs create distinct call paths.
    let (left, right) = tokio::join!(
        call(&engine, "user.invoke", vec![dependent.clone()]),
        call(&engine, "user.invoke", vec![dependent.clone()]),
    );
    assert_eq!(
        (left, right),
        (BexExternalValue::Int(7), BexExternalValue::Int(7))
    );
    let registered_before_calls = engine
        .program_metadata()
        .await
        .function_table
        .functions
        .len();
    for _ in 0..2 {
        assert_eq!(
            call(&engine, "user.invoke", vec![dependent.clone()]).await,
            BexExternalValue::Int(7)
        );
    }
    assert_eq!(
        engine
            .program_metadata()
            .await
            .function_table
            .functions
            .len(),
        registered_before_calls,
        "new closure instances and repeated calls reuse the registered function bodies"
    );
    let before = engine.program_metadata().await;
    let leaf_rows: Vec<_> = before
        .function_table
        .functions
        .iter()
        .filter(|row| row.fqn.ends_with(".identity_leaf"))
        .collect();
    assert_eq!(
        leaf_rows.len(),
        1,
        "only the observed definition registers; imports reuse it"
    );
    // Two minors promote live dynamic definitions to Gen2. The third minor
    // must preserve them even when old-generation tracing does not visit them.
    for level in [
        CollectionLevel::Minor,
        CollectionLevel::Minor,
        CollectionLevel::Minor,
        CollectionLevel::Major,
    ] {
        engine.collect_garbage(level).await;
        assert_eq!(package_functions(&engine, &first).await, first_functions);
        assert_eq!(
            package_functions(&engine, &dependent).await,
            dependent_functions
        );
        for (id, name) in first_functions
            .iter()
            .chain(&second_functions)
            .chain(&dependent_functions)
        {
            let metadata = engine.function_metadata(*id).await;
            if before.function_table.get(*id).is_some() {
                assert_eq!(&metadata.unwrap().fqn, name);
            } else {
                assert!(
                    metadata.is_none(),
                    "GC must not register unobserved functions"
                );
            }
        }
    }
    // Resolve while another task requests moving collections. Lookup must
    // hold its permit through metadata copying, without rooting the function.
    let lookup_engine = Arc::clone(&engine);
    let lookup_id = leaf(&first_functions);
    let reader = tokio::spawn(async move {
        for _ in 0..32 {
            assert!(lookup_engine.function_metadata(lookup_id).await.is_some());
            tokio::task::yield_now().await;
        }
    });
    for _ in 0..4 {
        engine.collect_garbage(CollectionLevel::Major).await;
    }
    reader.await.unwrap();
    let retained = engine
        .function_metadata(leaf(&first_functions))
        .await
        .unwrap();
    let retained_name = retained.fqn.clone();
    drop((first, second, dependent));
    // Old-generation weak entries survive a minor cycle, even without roots.
    engine.collect_garbage(CollectionLevel::Minor).await;
    assert_eq!(
        engine.function_metadata(retained.function_id).await,
        Some(retained.clone())
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    for (id, _) in first_functions
        .iter()
        .chain(&second_functions)
        .chain(&dependent_functions)
    {
        assert!(
            engine.function_metadata(*id).await.is_none(),
            "collected dynamic lookup must miss"
        );
    }
    assert_eq!(
        retained.fqn, retained_name,
        "owned metadata remains usable after collection"
    );
    let available = engine.program_metadata().await.function_table;
    assert!(available.get(retained.function_id).is_none());
    // IDs never recycle, including when all previous dynamic entries are gone.
    let later = call(&engine, "user.make", vec![]).await;
    let later_functions = package_functions(&engine, &later).await;
    let previous_max = before.function_table.functions.last().unwrap().function_id;
    assert!(later_functions.iter().all(|(id, _)| *id > previous_max));
    let snapshot = engine.program_metadata().await.function_table;
    for (id, _) in later_functions {
        assert!(
            snapshot.get(id).is_none(),
            "new definitions stay unregistered until observed"
        );
    }
}

#[tokio::test]
async fn session_shadowing_registers_distinct_functions_and_copies_metadata() {
    let engine = engine(
        r#"
function run() -> int {
  let s = reflect.Session.new()
  s.eval(`function IdentityCurrent() -> int { 1 }`)
  let first = s.eval<int>(`IdentityCurrent()`)
  s.eval(`function IdentityCurrent() -> int { 2 }`)
  first + s.eval<int>(`IdentityCurrent()`)
}
"#,
    );
    let before = engine.program_metadata().await;
    assert_eq!(
        call(&engine, "user.run", vec![]).await,
        BexExternalValue::Int(3)
    );
    let after = engine.program_metadata().await;
    let rows: Vec<_> = after
        .function_table
        .functions
        .iter()
        .filter(|row| row.fqn.ends_with("_IdentityCurrent"))
        .collect();
    assert_eq!(
        rows.len(),
        2,
        "new functions: {:?}",
        after.function_table.functions[before.function_table.functions.len()..]
            .iter()
            .map(|row| &row.fqn)
            .collect::<Vec<_>>()
    );
    assert_ne!(rows[0].function_id, rows[1].function_id);
    assert_eq!(
        &after.function_table.functions[..before.function_table.functions.len()],
        &before.function_table.functions
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    for row in rows {
        assert!(engine.function_metadata(row.function_id).await.is_none());
        assert!(
            row.fqn.ends_with("_IdentityCurrent"),
            "snapshot survives collection"
        );
    }
}

#[tokio::test]
async fn initial_ids_match_metadata_and_runtime_identity_is_not_serialized() {
    let source = "function leaf() -> int { 1 } function main() -> int { leaf() }";
    let program = baml_tests::stdlib_prefix::compile_source(source);
    let index = program.function_index("user.main").unwrap();
    assert!(
        program.objects.iter().all(
            |object| !matches!(object, Object::Function(f) if f.telemetry_function_id.is_some())
        )
    );
    let first = from_program(program.clone());
    let serialized = borsh::to_vec(&program).unwrap();
    let reloaded = borsh::from_slice(&serialized).unwrap();
    let second = from_program(reloaded);
    assert_ne!(
        first.program_metadata().await.program_id,
        second.program_metadata().await.program_id
    );
    for engine in [&first, &second] {
        let table = engine.program_metadata().await.function_table;
        engine.collect_garbage(CollectionLevel::Major).await;
        assert_eq!(engine.program_metadata().await.function_table, table);
        let inactive = engine.heap_permit_manager().new_permit(()).await;
        let _active = inactive.acquire().await;
        let mut ids = HashSet::new();
        for (index, object) in program.objects.iter().enumerate() {
            if !matches!(object, Object::Function(_)) {
                continue;
            }
            // SAFETY: immutable compile-time region, with a heap permit held.
            let Object::Function(function) =
                (unsafe { engine.heap().compile_time_ptr(index).get() })
            else {
                panic!("function changed kind")
            };
            let id = function.telemetry_function_id.unwrap();
            assert!(ids.insert(id));
            assert_eq!(table.get(id).unwrap().fqn, function.name);
            assert_eq!(
                id.get() as usize,
                ids.len(),
                "preserve initial object-pool numbering"
            );
        }
        // SAFETY: the same active permit covers the final runtime function.
        let Object::Function(function) = (unsafe { engine.heap().compile_time_ptr(index).get() })
        else {
            unreachable!()
        };
        let cloned = function.clone();
        assert_eq!(
            cloned.telemetry_function_id, function.telemetry_function_id,
            "GC copies preserve identity"
        );
        let bytes = borsh::to_vec(function.as_ref()).unwrap();
        let template: Function = borsh::from_slice(&bytes).unwrap();
        assert_eq!(template.telemetry_function_id, None);
        assert_eq!(borsh::to_vec(&template).unwrap(), bytes);
    }
}

#[tokio::test]
async fn synthetic_entries_never_register_telemetry_functions() {
    let engine = engine("function main() -> int { 1 }");
    let initial = engine.program_metadata().await.function_table;
    for _ in 0..128 {
        call(&engine, "baml.sys.argv", vec![]).await;
    }
    let before_gc = engine.program_metadata().await.function_table;
    let entries: Vec<_> = before_gc
        .functions
        .iter()
        .filter(|row| row.fqn == "$entry::baml.sys.argv")
        .collect();
    assert!(entries.is_empty());
    assert_eq!(before_gc, initial);
    engine.collect_garbage(CollectionLevel::Minor).await;
    assert_eq!(engine.program_metadata().await.function_table, initial);
}
