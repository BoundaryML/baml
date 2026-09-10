//! Effective scopes and immutable ownership have no BAML read API.
#![cfg(not(target_arch = "wasm32"))]

use std::sync::{Arc, atomic::AtomicBool};

use baml_db::testing::compile_source;
use bex_events::prof::backend::{ScopeContext, ScopeValue};
use bex_vm::{BexVm, VmExecState};

fn vm(source: &str) -> BexVm {
    let program = compile_source(source);
    let entry = program.function_index("user.main").expect("main");
    let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
    let entry = vm.heap.compile_time_ptr(entry);
    vm.set_entry_point(entry, &[]);
    vm
}

fn next_scope(vm: &mut BexVm) -> Arc<ScopeContext> {
    loop {
        match vm.exec().expect("exec") {
            VmExecState::Event { event_name, .. } => {
                assert_eq!(event_name, "$baml_log");
                vm.stack.push(bex_vm_types::Value::NULL);
                return Arc::clone(vm.scope_context());
            }
            VmExecState::EarlyYield => {}
            other => panic!("expected scope probe, got {other:?}"),
        }
    }
}

#[test]
fn scope_is_copied_composed_and_restored_with_profiling_off() {
    let mut vm = vm(r#"
        function leaf() -> int { log.info("leaf"); 7 }
        function nested() -> int {
            log.info("parent");
            let dynamic = leaf;
            dynamic($id = boundary.id().context(
                metadata = { "remove": null, "missing": null, "count": 2 },
                distinct_id = "child"));
            (() -> { log.info("iife"); 0 })(
                $id = boundary.id().context(metadata = { "iife": true }));
            log.info("parent-after");
            0
        }
        function main() -> int {
            let metadata: map<string, string | int | float | bool | null> =
                { "remove": "yes", "count": 1, "flag": true, "ratio": 1.5 };
            let id = boundary.id().context(metadata = metadata, distinct_id = "parent")
                .capture(inputs = true).context(metadata = { "composed": "yes" });
            metadata["count"] = 99;
            metadata["remove"] = null;
            nested($id = id);
            log.info("root");
            0
        }
        "#);
    let parent = next_scope(&mut vm);
    assert_eq!(parent.distinct_id.as_deref(), Some("parent"));
    assert_eq!(parent.metadata["count"], ScopeValue::Int(1));
    assert_eq!(parent.metadata["remove"], ScopeValue::String("yes".into()));
    assert_eq!(
        parent.metadata["composed"],
        ScopeValue::String("yes".into())
    );
    assert_eq!(parent.metadata["flag"], ScopeValue::Bool(true));
    assert_eq!(parent.metadata["ratio"], ScopeValue::Float(1.5));
    let child = next_scope(&mut vm);
    assert_eq!(child.distinct_id.as_deref(), Some("child"));
    assert_eq!(child.metadata["count"], ScopeValue::Int(2));
    assert!(!child.metadata.contains_key("remove"));
    assert!(!child.metadata.contains_key("missing"));
    let iife = next_scope(&mut vm);
    assert_eq!(iife.distinct_id.as_deref(), Some("parent"));
    assert_eq!(iife.metadata["iife"], ScopeValue::Bool(true));
    assert!(Arc::ptr_eq(&parent, &next_scope(&mut vm)));
    assert_eq!(*next_scope(&mut vm), ScopeContext::default());
    assert!(matches!(vm.exec().unwrap(), VmExecState::Complete(_)));
    assert_eq!(**vm.scope_context(), ScopeContext::default());
}

#[test]
fn throw_restores_the_callers_scope() {
    let mut vm = vm(r#"
        function fail() -> int throws string {
            log.info("inside");
            throw "failure";
        }
        function main() -> int {
            fail($id = boundary.id().context(distinct_id = "child"))
                catch (e) { _ => 0 };
            log.info("outside");
            0
        }
        "#);
    assert_eq!(next_scope(&mut vm).distinct_id.as_deref(), Some("child"));
    assert_eq!(*next_scope(&mut vm), ScopeContext::default());
}
