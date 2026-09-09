//! Concrete SDK callers use the selected implementation's contract, including
//! concrete Self. These tests cross host entry and inspect retained ownership.
mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue as V, FunctionCallContextBuilder};
use bex_external_types::{BexExternalAdt as Adt, InterfaceValue, RuntimeTy, TaggedHeapHandleKind};
use bex_heap::CollectionLevel;
use sys_native::SysOpsExt;

fn context() -> bex_engine::FunctionCallContext {
    FunctionCallContextBuilder::new(sys_types::CallId::next()).build()
}

fn engine(source: &str) -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new(
            common::compile_for_engine(source),
            Arc::new(sys_native::SysOps::native()),
            vec![],
        )
        .unwrap(),
    )
}

fn view(value: V) -> Arc<InterfaceValue> {
    match value {
        V::Adt(Adt::Interface(view)) => view,
        other => panic!("expected interface view, got {other:?}"),
    }
}

fn assert_concrete(value: &V) {
    assert!(
        matches!(
            value,
            V::Adt(Adt::TaggedHeapHandle {
                kind: TaggedHeapHandleKind::ConcreteObject,
                ..
            })
        ),
        "{value:?}"
    );
}

fn class_handle(value: V) -> bex_external_types::Handle {
    match value {
        V::Adt(Adt::TaggedHeapHandle {
            kind: TaggedHeapHandleKind::ConcreteObject,
            heap_handle,
            ..
        }) => heap_handle,
        other => panic!("expected concrete handle, got {other:?}"),
    }
}

fn name(value: &str) -> baml_type::QualifiedTypeName {
    baml_type::QualifiedTypeName::local(baml_type::Name::new(value))
}

#[tokio::test]
async fn inherent_methods_keep_owner_arguments_state_and_checked_call_contract() {
    let engine = engine(
        r#"
interface Named { function label(self) -> string throws never }
class Box<T> {
    value: T,
    function read(self) -> T throws never { self.value }
    function replace(self, value: T) -> T throws never { self.value = value; self.value }
    function echo<U>(self, value: U) -> U throws never { value }
    function static_value() -> int throws never { 42 }
    implements Named { function label(self) -> string throws never { "box" } }
}
class Other { function read(self) -> int throws never { 0 } }
function make() -> Box<string> throws never { Box<string> { value: "Ada" } }
"#,
    );
    let receiver = class_handle(
        engine
            .call_function("make", vec![], context(), true)
            .await
            .unwrap(),
    );
    let read = engine
        .bind_concrete_class_inherent_method(receiver.clone(), &name("Box"), "read", vec![])
        .await
        .unwrap();
    let replace = engine
        .bind_concrete_class_inherent_method(receiver.clone(), &name("Box"), "replace", vec![])
        .await
        .unwrap();
    let echo = engine
        .bind_concrete_class_inherent_method(
            receiver.clone(),
            &name("Box"),
            "echo",
            vec![bex_external_types::TypeArgument::Named(RuntimeTy::int())],
        )
        .await
        .unwrap();
    for (class, member, args) in [
        ("Other", "read", vec![]),
        ("Box", "missing", vec![]),
        ("Box", "static_value", vec![]),
        ("Box", "label", vec![]),
        ("Box", "echo", vec![]),
        (
            "Box",
            "read",
            vec![bex_external_types::TypeArgument::Named(RuntimeTy::int())],
        ),
    ] {
        assert!(
            engine
                .bind_concrete_class_inherent_method(receiver.clone(), &name(class), member, args)
                .await
                .is_err(),
            "{class}.{member}"
        );
    }
    assert!(
        engine
            .call_callable(replace.clone(), vec![V::Int(1)], context(), true)
            .await
            .is_err()
    );
    assert_eq!(
        engine
            .call_callable(read.clone(), vec![], context(), true)
            .await
            .unwrap(),
        V::String("Ada".into())
    );
    assert_eq!(
        engine
            .call_callable_keywords(
                replace,
                [("value".into(), V::String("Grace".into()))].into(),
                context(),
                true
            )
            .await
            .unwrap(),
        V::String("Grace".into())
    );
    drop(receiver);
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_callable(read, vec![], context(), true)
            .await
            .unwrap(),
        V::String("Grace".into())
    );
    assert_eq!(
        engine
            .call_callable(echo, vec![V::Int(9)], context(), true)
            .await
            .unwrap(),
        V::Int(9)
    );
}

const FACTORY: &str = r#"
interface Child { function label(self) -> string throws never }
class NamedChild {
    implements Child { function label(self) -> string throws never { "Ada" } }
}
interface Factory {
    function accept(self, text: string) -> int throws string
    function count(self) -> int throws never
    function child(self) -> Child throws never
    function same(self) -> Self throws never { self }
    function pair(self, other: Self) -> Self throws never { other }
}
class StoredFactory {
    calls: int,
    implements Factory {
        function accept(self, incoming: unknown, delta: int = 2) -> int throws never {
            self.calls += delta; self.calls
        }
        function count(self) -> int throws never { self.calls }
        function child(self) -> NamedChild throws never { NamedChild {} }
    }
}
class OtherFactory {
    implements Factory {
        function accept(self, text: string) -> int throws never { 0 }
        function count(self) -> int throws never { 0 }
        function child(self) -> NamedChild throws never { NamedChild {} }
    }
}
function make() -> Factory throws never { StoredFactory { calls: 0 } }
function other() -> Factory throws never { OtherFactory {} }
"#;

#[tokio::test]
async fn concrete_contract_preserves_refined_inputs_names_defaults_and_results() {
    let engine = engine(FACTORY);
    let factory = view(
        engine
            .call_function("make", vec![], context(), true)
            .await
            .unwrap(),
    );
    assert!(
        engine
            .call_interface_named(
                factory.clone(),
                "accept",
                vec![],
                [("text".into(), V::Int(7))].into(),
                context(),
            )
            .await
            .is_err()
    );
    assert_eq!(
        engine
            .call_interface(factory.clone(), "count", vec![], vec![], context())
            .await
            .unwrap(),
        V::Int(0)
    );

    // A concrete caller gets the provided required name and extra optional,
    // not the interface's narrower input or its argument-layout mapping.
    assert_eq!(
        engine
            .call_concrete_interface_named(
                factory.clone(),
                "accept",
                vec![],
                [("incoming".into(), V::Int(7)), ("delta".into(), V::Int(3))].into(),
                context(),
            )
            .await
            .unwrap(),
        V::Int(3)
    );
    assert_eq!(
        engine
            .call_concrete_interface_named(
                factory.clone(),
                "accept",
                vec![],
                [("incoming".into(), V::Bool(true))].into(),
                context(),
            )
            .await
            .unwrap(),
        V::Int(5)
    );
    assert!(
        engine
            .call_concrete_interface_named(
                factory.clone(),
                "accept",
                vec![],
                [("text".into(), V::String("wrong name".into()))].into(),
                context(),
            )
            .await
            .is_err()
    );
    assert_eq!(
        engine
            .call_interface(factory.clone(), "count", vec![], vec![], context())
            .await
            .unwrap(),
        V::Int(5)
    );

    let interface_child = engine
        .call_interface(factory.clone(), "child", vec![], vec![], context())
        .await
        .unwrap();
    assert!(matches!(interface_child, V::Adt(Adt::Interface(_))));
    let child = engine
        .bind_concrete_interface_method(factory.clone(), "child", vec![])
        .await
        .unwrap();
    let same = engine
        .bind_concrete_interface_method(factory.clone(), "same", vec![])
        .await
        .unwrap();
    drop(factory);
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_concrete(
        &engine
            .call_callable(child, vec![], context(), true)
            .await
            .unwrap(),
    );
    assert_concrete(
        &engine
            .call_callable(same, vec![], context(), true)
            .await
            .unwrap(),
    );
}

#[tokio::test]
async fn concrete_self_is_checked_against_the_actual_receiver_before_execution() {
    let engine = engine(FACTORY);
    let factory = view(
        engine
            .call_function("make", vec![], context(), true)
            .await
            .unwrap(),
    );
    let other = engine
        .call_function("other", vec![], context(), true)
        .await
        .unwrap();
    // The original interface caller remains unavailable for concrete-Self args.
    assert!(
        engine
            .bind_interface_method(factory.clone(), "pair", vec![])
            .await
            .is_err()
    );
    let pair = engine
        .bind_concrete_interface_method(factory.clone(), "pair", vec![])
        .await
        .unwrap();
    assert!(
        engine
            .call_callable(pair.clone(), vec![other], context(), true)
            .await
            .is_err()
    );
    let own = V::Adt(Adt::Interface(factory.clone()));
    let returned = engine
        .call_callable(pair, vec![own], context(), true)
        .await
        .unwrap();
    assert_concrete(&returned);
    let projected = engine
        .project_interface(
            returned,
            RuntimeTy::Interface(
                baml_type::QualifiedTypeName::new(
                    baml_type::Name::new("user"),
                    vec![],
                    baml_type::Name::new("Factory"),
                ),
                vec![],
                vec![],
                baml_type::TyAttr::default(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(projected.receiver, factory.receiver);
    assert!(
        engine
            .bind_concrete_interface_method(factory.clone(), "calls", vec![])
            .await
            .is_err()
    );
    let foreign = factory_engine();
    assert!(
        foreign
            .bind_concrete_interface_method(factory, "count", vec![])
            .await
            .is_err()
    );
}

fn factory_engine() -> Arc<BexEngine> {
    engine(FACTORY)
}

#[tokio::test]
async fn concrete_non_class_receiver_preserves_identity_and_checks_method_bounds() {
    let engine = engine(
        r#"
interface Marker {}
implements Marker for string {}
interface Echo {
    function same(self) -> Self throws never { self }
    function echo<T extends Marker>(self, value: T) -> T throws never { value }
}
implements Echo for int {}
function make() -> Echo throws never { 7 }
"#,
    );
    let receiver = view(
        engine
            .call_function("make", vec![], context(), true)
            .await
            .unwrap(),
    );
    let same = engine
        .bind_concrete_interface_method(receiver.clone(), "same", vec![])
        .await
        .unwrap();
    assert!(
        engine
            .bind_concrete_interface_method(
                receiver.clone(),
                "echo",
                vec![bex_external_types::TypeArgument::Named(RuntimeTy::int()),]
            )
            .await
            .is_err()
    );
    let echo = engine
        .bind_concrete_interface_method(
            receiver.clone(),
            "echo",
            vec![bex_external_types::TypeArgument::Named(RuntimeTy::string())],
        )
        .await
        .unwrap();
    drop(receiver);
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_callable(same, vec![], context(), true)
            .await
            .unwrap(),
        V::Int(7)
    );
    assert_eq!(
        engine
            .call_callable(echo, vec![V::String("Ada".into())], context(), true)
            .await
            .unwrap(),
        V::String("Ada".into())
    );
}

#[tokio::test]
async fn concrete_default_method_keeps_class_and_method_type_frames_after_gc() {
    let engine = engine(
        r#"
interface Source {
    type Output
    function read(self) -> Self.Output throws never
    function echo<T>(self, value: T) -> T throws never { value }
    function same(self) -> Self throws never { self }
}

class Box<A, T> {
    first: A,
    second: T,
    implements Source {
        type Output = T
        function read(self) -> T throws never { self.second }
    }
}
function make() -> Source<Output=string> throws never {
    Box<int, string> { first: 1, second: "Ada" }
}
"#,
    );
    let source = view(
        engine
            .call_function("make", vec![], context(), true)
            .await
            .unwrap(),
    );
    let echo = engine
        .bind_concrete_interface_method(
            source.clone(),
            "echo",
            vec![bex_external_types::TypeArgument::Named(RuntimeTy::int())],
        )
        .await
        .unwrap();
    assert!(
        engine
            .bind_concrete_interface_method(source.clone(), "echo", vec![])
            .await
            .is_err()
    );
    let read = engine
        .bind_concrete_interface_method(source.clone(), "read", vec![])
        .await
        .unwrap();
    let same = engine
        .bind_concrete_interface_method(source.clone(), "same", vec![])
        .await
        .unwrap();
    drop(source);
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_callable(read, vec![], context(), true)
            .await
            .unwrap(),
        V::String("Ada".into())
    );
    assert_eq!(
        engine
            .call_callable(echo.clone(), vec![V::Int(9)], context(), true)
            .await
            .unwrap(),
        V::Int(9)
    );
    assert!(
        engine
            .call_callable(
                echo,
                vec![V::String("wrong method type".into())],
                context(),
                true
            )
            .await
            .is_err()
    );
    assert_concrete(
        &engine
            .call_callable(same, vec![], context(), true)
            .await
            .unwrap(),
    );
}

#[tokio::test]
async fn generated_class_obligation_uses_the_receivers_class_slots_and_exact_declaration() {
    let engine = engine(
        r#"
interface Source {
    type Output
    function read(self) -> Self.Output throws never
    function echo<T>(self, value: T) -> T throws never { value }
}
class Box<A, T> {
    first: A,
    second: T,
    implements Source {
        type Output = T
        function read(self) -> T throws never { self.second }
    }
}
class OtherBox { function unrelated(self) -> int throws never { 0 } }
function make() -> Box<int, string> throws never {
    Box<int, string> { first: 1, second: "Ada" }
}
"#,
    );
    let receiver = class_handle(
        engine
            .call_function("make", vec![], context(), true)
            .await
            .unwrap(),
    );
    let obligation = |output| {
        RuntimeTy::Interface(
            name("Source"),
            vec![],
            vec![(baml_type::Name::new("Output"), output)],
            baml_type::TyAttr::default(),
        )
    };
    let slot = |index| {
        RuntimeTy::TypeVar(
            baml_type::ParamTy::new(index, baml_type::Name::new(format!("$sdk$class${index}"))),
            baml_type::TyAttr::default(),
        )
    };
    let read = engine
        .bind_concrete_class_interface_method(
            receiver.clone(),
            &name("Box"),
            obligation(slot(1)),
            "read",
            vec![],
        )
        .await
        .unwrap();
    let echo = engine
        .bind_concrete_class_interface_method(
            receiver.clone(),
            &name("Box"),
            obligation(slot(1)),
            "echo",
            vec![bex_external_types::TypeArgument::Named(RuntimeTy::int())],
        )
        .await
        .unwrap();
    // A caller may request an obligation, but cannot change the class's pins
    // or substitute its own concrete declaration into the receiver frame.
    for invalid in [
        obligation(slot(0)),
        obligation(slot(99)),
        obligation(RuntimeTy::int()),
        RuntimeTy::string(),
    ] {
        assert!(
            engine
                .bind_concrete_class_interface_method(
                    receiver.clone(),
                    &name("Box"),
                    invalid,
                    "read",
                    vec![],
                )
                .await
                .is_err()
        );
    }
    assert!(
        engine
            .bind_concrete_class_interface_method(
                receiver.clone(),
                &name("OtherBox"),
                obligation(slot(1)),
                "read",
                vec![],
            )
            .await
            .is_err()
    );
    assert!(
        engine
            .bind_concrete_class_interface_method(
                receiver.clone(),
                &name("Missing"),
                obligation(slot(1)),
                "read",
                vec![],
            )
            .await
            .is_err()
    );
    drop(receiver);
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_callable(read, vec![], context(), true)
            .await
            .unwrap(),
        V::String("Ada".into())
    );
    assert_eq!(
        engine
            .call_callable(echo, vec![V::Int(9)], context(), true)
            .await
            .unwrap(),
        V::Int(9)
    );
}
