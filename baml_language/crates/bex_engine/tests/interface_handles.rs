//! Host-entry invocation and ownership of BAML interface method handles.
//! These tests exercise the engine boundary, not only calls within BAML.

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use bex_external_types::{BexExternalAdt, Handle, TaggedHeapHandleKind};
use bex_heap::CollectionLevel;
use sys_native::SysOpsExt;

fn context() -> bex_engine::FunctionCallContext {
    FunctionCallContextBuilder::new(sys_types::CallId::next()).build()
}

fn callable(value: BexExternalValue) -> Handle {
    match value {
        BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
            kind: TaggedHeapHandleKind::Callable,
            heap_handle,
            ..
        }) => heap_handle,
        other => panic!("expected a checked callable, got {other:?}"),
    }
}

#[tokio::test]
async fn interface_bound_method_keeps_default_frame_and_receiver() {
    let program = common::compile_for_engine(
        r#"
interface Producer {
    type Output
    function next(self) -> Self.Output throws never
    function again(self) -> Self.Output throws never { self.next() }
}
class Counter {
    count: int,
    implements Producer {
        type Output = int
        function next(self) -> int throws never {
            self.count += 1;
            self.count
        }
    }
}
function make_callback() -> () -> int throws never {
    let value: Producer<Output=int> = Counter { count: 40 };
    value.again
}
function invoke(callback: () -> int throws never) -> int throws never { callback() }
"#,
    );
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );
    let callback = engine
        .call_function("make_callback", vec![], context(), true)
        .await
        .unwrap();
    let handle = callable(callback.clone());
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_callable(handle.clone(), vec![], context(), true)
            .await
            .unwrap(),
        BexExternalValue::Int(41)
    );
    assert_eq!(
        engine
            .call_function("invoke", vec![callback], context(), true)
            .await
            .unwrap(),
        BexExternalValue::Int(42)
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_callable(handle, vec![], context(), true)
            .await
            .unwrap(),
        BexExternalValue::Int(43)
    );
}

#[tokio::test]
async fn interface_bound_method_keeps_non_class_and_method_generic_frames() {
    let program = common::compile_for_engine(
        r#"
interface Identity {
    function echo<T>(self, value: T) -> T throws never { value }
    function empty<T>(self) -> T[] throws never { [] }
}
implements Identity for int {}
function make_callback() -> (string) -> string throws never {
    let value: Identity = 7;
    value.echo<string>
}
function make_empty_callback() -> () -> string[] throws never {
    let value: Identity = 7;
    value.empty<string>
}
"#,
    );
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );
    let handle = callable(
        engine
            .call_function("make_callback", vec![], context(), true)
            .await
            .unwrap(),
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    engine
        .call_callable(
            handle.clone(),
            vec![BexExternalValue::Int(9)],
            context(),
            true,
        )
        .await
        .expect_err("the explicit string specialization must not be re-inferred as int");
    assert_eq!(
        engine
            .call_callable(
                handle,
                vec![BexExternalValue::String("Ada".into())],
                context(),
                true
            )
            .await
            .unwrap(),
        BexExternalValue::String("Ada".into())
    );
    let empty = callable(
        engine
            .call_function("make_empty_callback", vec![], context(), true)
            .await
            .unwrap(),
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    let result = engine
        .call_callable(empty, vec![], context(), true)
        .await
        .unwrap();
    let BexExternalValue::Array {
        element_type,
        items,
    } = result
    else {
        panic!("expected the specialized array result");
    };
    assert!(items.is_empty());
    assert_eq!(element_type, bex_external_types::RuntimeTy::string());
}

#[tokio::test]
async fn interface_method_checks_live_arguments_and_literals_before_execution() {
    let program = common::compile_for_engine(
        r#"
interface Source {
    type Output
    function get(self) -> Self.Output throws never
}
class Box<T> {
    value: T,
    implements Source {
        type Output = T
        function get(self) -> T throws never { self.value }
    }
}
interface Gate {
    function accept(self, value: Box<string>, source: Source<Output=string>, mode: "accept") -> int throws never
    function count(self) -> int throws never
}
class CountingGate {
    calls: int,
    implements Gate {
        function accept(self, value: Box<string>, source: Source<Output=string>, mode: "accept") -> int throws never {
            self.calls += 1;
            self.calls
        }
        function count(self) -> int throws never { self.calls }
    }
}
class Callbacks {
    accept: (Box<string>, Source<Output=string>, "accept") -> int throws never,
    count: () -> int throws never,
}
function make_callbacks() -> Callbacks {
    let gate: Gate = CountingGate { calls: 0 };
    Callbacks { accept: gate.accept, count: gate.count }
}
function accept_callback(callbacks: Callbacks) -> (Box<string>, Source<Output=string>, "accept") -> int throws never {
    callbacks.accept
}
function count_callback(callbacks: Callbacks) -> () -> int throws never { callbacks.count }
function make_string() -> Box<string> { Box { value: "Ada" } }
function make_int() -> Box<int> { Box { value: 9 } }
"#,
    );
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );
    let initial_handles = engine.heap_stats().active_handles;
    let callbacks = engine
        .call_function("make_callbacks", vec![], context(), false)
        .await
        .unwrap();
    let accept = callable(
        engine
            .call_function("accept_callback", vec![callbacks.clone()], context(), true)
            .await
            .unwrap(),
    );
    let count = callable(
        engine
            .call_function("count_callback", vec![callbacks], context(), true)
            .await
            .unwrap(),
    );
    let good = engine
        .call_function("make_string", vec![], context(), false)
        .await
        .unwrap();
    let bad = engine
        .call_function("make_int", vec![], context(), false)
        .await
        .unwrap();
    let BexExternalValue::Handle(bad_handle) = bad else {
        panic!("expected a live object");
    };
    // A host-controlled annotation cannot repin the actual Box<int> receiver.
    let forged = BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
        kind: TaggedHeapHandleKind::RuntimeValue,
        ty: bex_external_types::RuntimeTy::class_with_args(
            baml_type::TypeName::local(baml_type::Name::new("Box")),
            vec![bex_external_types::RuntimeTy::string()],
        ),
        heap_handle: bad_handle,
        sdk_declaration: None,
    });
    engine.collect_garbage(CollectionLevel::Major).await;
    for args in [
        vec![
            forged.clone(),
            good.clone(),
            BexExternalValue::String("accept".into()),
        ],
        vec![
            good.clone(),
            forged,
            BexExternalValue::String("accept".into()),
        ],
        vec![
            good.clone(),
            good.clone(),
            BexExternalValue::String("reject".into()),
        ],
    ] {
        let error = engine
            .call_callable(accept.clone(), args, context(), true)
            .await
            .unwrap_err();
        assert!(
            matches!(error, bex_engine::EngineError::TypeMismatch { .. }),
            "{error:?}"
        );
        assert_eq!(
            engine
                .call_callable(count.clone(), vec![], context(), true)
                .await
                .unwrap(),
            BexExternalValue::Int(0)
        );
    }
    assert_eq!(
        engine
            .call_callable(
                accept,
                vec![
                    good.clone(),
                    good,
                    BexExternalValue::String("accept".into())
                ],
                context(),
                true
            )
            .await
            .unwrap(),
        BexExternalValue::Int(1)
    );
    assert_eq!(
        engine
            .call_callable(count, vec![], context(), true)
            .await
            .unwrap(),
        BexExternalValue::Int(1)
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine.heap_stats().active_handles,
        initial_handles,
        "completed calls and dropped method handles must release their roots"
    );
}

#[tokio::test]
async fn bound_method_keeps_class_and_method_parameters_separate() {
    let program = common::compile_for_engine(
        r#"
class ValueBox<V> {
    value: V,
    function items<T>(self, label: T) -> V[] throws never { [self.value] }
}
function make_callback() -> (string) -> int[] throws never {
    let box = ValueBox { value: 9 };
    box.items<string>
}
"#,
    );
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );
    let callback = callable(
        engine
            .call_function("make_callback", vec![], context(), true)
            .await
            .unwrap(),
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    let invalid = engine
        .call_callable(
            callback.clone(),
            vec![BexExternalValue::Int(9)],
            context(),
            true,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(invalid, bex_engine::EngineError::TypeMismatch { .. }),
        "{invalid:?}"
    );
    let output = engine
        .call_callable(
            callback,
            vec![BexExternalValue::String("items".into())],
            context(),
            true,
        )
        .await
        .unwrap();
    let BexExternalValue::Array {
        element_type,
        items,
    } = output
    else {
        panic!("expected array result");
    };
    assert_eq!(element_type, bex_external_types::RuntimeTy::int());
    assert_eq!(items, vec![BexExternalValue::Int(9)]);
}

#[tokio::test]
async fn specialized_method_preserves_explicit_opaque_host_values() {
    let program = common::compile_for_engine(
        r#"
interface Identity {
    function echo<T>(self, value: T) -> T throws never { value }
}
implements Identity for int {}
function make_callback<T>(sample: T) -> (T) -> T throws never {
    let value: Identity = 7;
    value.echo<T>
}
"#,
    );
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );
    let opaque = BexExternalValue::HostValue(bex_external_types::HostValueArc::new(
        555,
        bex_external_types::HostValueKind::Opaque,
    ));
    let callback = callable(
        engine
            .call_function("make_callback", vec![opaque.clone()], context(), true)
            .await
            .unwrap(),
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    let output = engine
        .call_callable(callback, vec![opaque.clone()], context(), true)
        .await
        .unwrap();
    assert_eq!(output, opaque);
}

#[tokio::test]
async fn specialized_enum_method_validates_erased_host_members() {
    let program = common::compile_for_engine(
        r#"
enum Flavor { Vanilla, Chocolate }
interface Echo {
    function echo<T>(self, value: T) -> T throws never { value }
}
class Identity { implements Echo {} }
function callback() -> (Flavor) -> Flavor throws never {
    let value: Echo = Identity {};
    value.echo<Flavor>
}
function optional_callback() -> (Flavor?) -> Flavor? throws never {
    let value: Echo = Identity {};
    value.echo<Flavor?>
}
function ambiguous_callback() -> (Flavor | string) -> Flavor | string throws never {
    let value: Echo = Identity {};
    value.echo<Flavor | string>
}
"#,
    );
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );
    let handle = callable(
        engine
            .call_function("callback", vec![], context(), true)
            .await
            .unwrap(),
    );
    let result = engine
        .call_callable(
            handle.clone(),
            vec![BexExternalValue::String("Vanilla".into())],
            context(),
            true,
        )
        .await
        .unwrap();
    assert!(
        matches!(result, BexExternalValue::Variant { ref variant_name, .. } if variant_name == "Vanilla")
    );
    for invalid in [
        BexExternalValue::String("Missing".into()),
        // A codec that explicitly identifies a string cannot relabel it as an enum.
        BexExternalValue::typed(
            BexExternalValue::String("Vanilla".into()),
            bex_external_types::RuntimeTy::string(),
        ),
    ] {
        engine
            .call_callable(handle.clone(), vec![invalid], context(), true)
            .await
            .expect_err("invalid member or contradictory source type must fail");
    }
    let result = engine
        .call_callable(
            handle,
            vec![BexExternalValue::String("Chocolate".into())],
            context(),
            true,
        )
        .await
        .unwrap();
    assert!(
        matches!(result, BexExternalValue::Variant { ref variant_name, .. } if variant_name == "Chocolate")
    );
    let optional = callable(
        engine
            .call_function("optional_callback", vec![], context(), true)
            .await
            .unwrap(),
    );
    engine
        .call_callable(
            optional.clone(),
            vec![BexExternalValue::String("Vanilla".into())],
            context(),
            true,
        )
        .await
        .unwrap();
    engine
        .call_callable(
            optional.clone(),
            vec![BexExternalValue::Null],
            context(),
            true,
        )
        .await
        .unwrap();
    engine
        .call_callable(
            optional,
            vec![BexExternalValue::String("Missing".into())],
            context(),
            true,
        )
        .await
        .expect_err("optional enum still validates membership");
    let ambiguous = callable(
        engine
            .call_function("ambiguous_callback", vec![], context(), true)
            .await
            .unwrap(),
    );
    engine
        .call_callable(
            ambiguous,
            vec![BexExternalValue::String("Vanilla".into())],
            context(),
            true,
        )
        .await
        .expect_err("strict callers must identify one overlapping union member");
}

#[tokio::test]
async fn interface_declaration_checks_unused_generics_independently_of_implementation() {
    let mut program = common::compile_for_engine(
        r#"
interface Marker {}
class Good { implements Marker {} }
interface Factory {
    function make<T extends Marker, U>(self) -> int throws never
}
class FactoryImpl {
    implements Factory {
        function make<T extends Marker, U>(self) -> int throws never { 7 }
    }
}
function make_factory() -> Factory throws never { FactoryImpl {} }
"#,
    );
    // Simulate an adapter implementation that does not encode this bound.
    // Admission must still enforce the interface's own checked declaration.
    let mut changed = false;
    for object in program.objects.iter_mut() {
        if let bex_vm_types::Object::Function(function) = object
            && function.is_interface_body
            && function.declared_name.as_deref() == Some("make")
        {
            for bounds in &mut function.generic_param_bounds {
                bounds.clear();
            }
            changed = true;
        }
    }
    assert!(changed);
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );
    let BexExternalValue::Adt(BexExternalAdt::Interface(view)) = engine
        .call_function("make_factory", vec![], context(), true)
        .await
        .unwrap()
    else {
        panic!("expected interface");
    };
    engine
        .call_interface(
            view.clone(),
            "make",
            vec![
                bex_external_types::RuntimeTy::int(),
                bex_external_types::RuntimeTy::string(),
            ],
            vec![],
            context(),
        )
        .await
        .expect_err("unused T still has its declared bound");
    engine
        .call_interface(
            view.clone(),
            "make",
            vec![bex_external_types::RuntimeTy::int()],
            vec![],
            context(),
        )
        .await
        .expect_err("unused U still occupies a slot");
    let good = bex_external_types::RuntimeTy::Class(
        baml_type::TypeName::from_dotted_path("user.Good"),
        vec![],
        baml_type::TyAttr::default(),
    );
    assert_eq!(
        engine
            .call_interface(
                view,
                "make",
                vec![good, bex_external_types::RuntimeTy::string()],
                vec![],
                context()
            )
            .await
            .unwrap(),
        BexExternalValue::Int(7)
    );
}
