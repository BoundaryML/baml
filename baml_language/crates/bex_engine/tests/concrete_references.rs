//! Concrete receivers retain identity through host calls and interface inputs.
mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue as V, FunctionCallContextBuilder};
use bex_external_types::{BexExternalAdt as Adt, RuntimeTy, TaggedHeapHandleKind as Kind};
use bex_heap::CollectionLevel;
use sys_native::SysOpsExt;

fn context() -> bex_engine::FunctionCallContext {
    FunctionCallContextBuilder::new(sys_types::CallId::next()).build()
}

fn engine() -> Arc<BexEngine> {
    let source = format!(
        "{}\n{}",
        include_str!("../../../sdk_tests/fixtures/interfaces/baml_src/main.baml"),
        r#"
function concrete_counter(initial: int) -> StoredCounter throws never { StoredCounter { count: initial } }
class ConcreteRecord { label: string, counter: StoredCounter }
function concrete_record(counter: StoredCounter) -> ConcreteRecord throws never {
    ConcreteRecord { label: "copied", counter }
}
interface Getter {
    type Output
    function get(self) -> Self.Output throws never
}
class Box<T> {
    value: T,
    implements Getter {
        type Output = T
        function get(self) -> T throws never { self.value }
    }
}
function string_box() -> Box<string> throws never { Box<string> { value: "Ada" } }
function int_box(value: Box<int>, effects: Counter) -> int throws never { effects.add(1); value.get() }
function callback() -> () -> int throws never { () -> { 42 } }
function invoke(value: () -> int throws never) -> int throws never { value() }
"#
    );
    Arc::new(
        BexEngine::new(
            common::compile_for_engine(&source),
            Arc::new(sys_native::SysOps::native()),
            vec![],
        )
        .unwrap(),
    )
}

fn assert_concrete(value: &V) {
    assert!(
        matches!(
            value,
            V::Adt(Adt::TaggedHeapHandle {
                kind: Kind::ConcreteObject,
                ..
            })
        ),
        "{value:?}"
    );
}

#[tokio::test]
async fn concrete_factory_result_enters_interface_without_copy_or_manual_view() {
    let engine = engine();
    let greeter = engine
        .call_function(
            "FriendlyGreeter.new",
            vec![V::String("Hello".into())],
            context(),
            true,
        )
        .await
        .unwrap();
    assert_concrete(&greeter);
    assert_eq!(
        engine
            .call_function(
                "welcome",
                vec![greeter, V::String("Ada".into())],
                context(),
                true
            )
            .await
            .unwrap(),
        V::String("Hello, Ada!".into())
    );

    let counter = engine
        .call_function("concrete_counter", vec![V::Int(4)], context(), true)
        .await
        .unwrap();
    assert_concrete(&counter);
    let record = engine
        .call_function("concrete_record", vec![counter.clone()], context(), true)
        .await
        .unwrap();
    let V::Instance { mut fields, .. } = record else {
        panic!("outer record must stay copied")
    };
    let child = fields.shift_remove("counter").unwrap();
    assert_concrete(&child);
    drop(counter);
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_function(
                "add_in_baml",
                vec![child.clone(), V::Int(3)],
                context(),
                true
            )
            .await
            .unwrap(),
        V::Int(7)
    );
    let view = engine
        .call_function("pass_counter", vec![child.clone()], context(), true)
        .await
        .unwrap();
    drop(child);
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_function("add_in_baml", vec![view, V::Int(2)], context(), true)
            .await
            .unwrap(),
        V::Int(9)
    );
}

#[tokio::test]
async fn concrete_generic_arguments_come_from_receiver_not_wire_annotations() {
    let engine = engine();
    let value = engine
        .call_function("string_box", vec![], context(), true)
        .await
        .unwrap();
    let V::Adt(Adt::TaggedHeapHandle {
        kind, heap_handle, ..
    }) = value
    else {
        panic!("expected concrete ref")
    };
    assert_eq!(kind, Kind::ConcreteObject);
    let forged = V::Adt(Adt::TaggedHeapHandle {
        kind,
        heap_handle,
        sdk_declaration: None,
        ty: RuntimeTy::class_with_args(
            baml_type::QualifiedTypeName::new("user".into(), vec![], "Box".into()),
            vec![RuntimeTy::int()],
        ),
    });
    let effects = engine
        .call_function("concrete_counter", vec![V::Int(0)], context(), true)
        .await
        .unwrap();
    assert!(
        engine
            .call_function("int_box", vec![forged, effects.clone()], context(), true)
            .await
            .is_err()
    );
    assert_eq!(
        engine
            .call_function("add_in_baml", vec![effects, V::Int(0)], context(), true)
            .await
            .unwrap(),
        V::Int(0)
    );
}

#[tokio::test]
async fn concrete_role_rejects_another_runtime_and_non_class_receivers() {
    let first = engine();
    let second = engine();
    let value = first
        .call_function("concrete_counter", vec![V::Int(0)], context(), true)
        .await
        .unwrap();
    assert!(
        second
            .call_function("add_in_baml", vec![value, V::Int(1)], context(), true)
            .await
            .is_err()
    );
    let function = first
        .call_function("callback", vec![], context(), true)
        .await
        .unwrap();
    let V::Adt(Adt::TaggedHeapHandle { heap_handle, .. }) = function else {
        panic!("expected callable")
    };
    let forged = V::Adt(Adt::TaggedHeapHandle {
        kind: Kind::ConcreteObject,
        heap_handle,
        sdk_declaration: None,
        ty: RuntimeTy::unknown(),
    });
    let error = first
        .call_function("invoke", vec![forged], context(), true)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("does not contain a class receiver"),
        "{error}"
    );
}
