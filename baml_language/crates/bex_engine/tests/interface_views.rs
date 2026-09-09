//! Engine-level use of retained BAML interface views (the generated SDK target).
mod common;
use bex_engine::{BexEngine, BexExternalValue as V, FunctionCallContextBuilder};
use bex_external_types::{BexExternalAdt, InterfaceValue, RuntimeTy};
use bex_heap::CollectionLevel;
use std::sync::Arc;
use sys_native::SysOpsExt;

fn context() -> bex_engine::FunctionCallContext {
    FunctionCallContextBuilder::new(sys_types::CallId::next()).build()
}
fn view(value: V) -> Arc<InterfaceValue> {
    match value {
        V::Adt(BexExternalAdt::Interface(view)) => view,
        other => panic!("expected retained interface, got {other:?}"),
    }
}
fn value(view: Arc<InterfaceValue>) -> V {
    V::Adt(BexExternalAdt::Interface(view))
}
fn engine(source: &str) -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new(
            common::compile_for_engine(source),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .unwrap(),
    )
}

#[tokio::test]
async fn typed_identity_checks_interface_projection_and_preserves_receiver() {
    let engine = engine(include_str!(
        "../../../sdk_tests/fixtures/interfaces/baml_src/main.baml"
    ));
    let baseline = engine.heap_stats().active_handles;
    let original = view(
        engine
            .call_function(
                "make_unspecified_counter",
                vec![V::Int(10)],
                context(),
                true,
            )
            .await
            .unwrap(),
    );
    let target = |item, error| {
        RuntimeTy::Interface(
            baml_type::QualifiedTypeName::local("CounterValue".into()),
            vec![],
            vec![("Error".into(), error), ("Value".into(), item)],
            Default::default(),
        )
    };
    let project_context = |expected| {
        FunctionCallContextBuilder::new(sys_types::CallId::next())
            .with_type_args(indexmap::indexmap! { "T".into() => expected })
            .build()
    };
    for wrong in [
        target(
            RuntimeTy::string(),
            RuntimeTy::Never {
                attr: Default::default(),
            },
        ),
        target(RuntimeTy::int(), RuntimeTy::string()),
    ] {
        assert!(
            engine
                .call_function(
                    "baml.identity",
                    vec![value(original.clone())],
                    project_context(wrong),
                    true
                )
                .await
                .is_err()
        );
    }
    let checked = view(
        engine
            .call_function(
                "baml.identity",
                vec![value(original.clone())],
                project_context(target(
                    RuntimeTy::int(),
                    RuntimeTy::Never {
                        attr: Default::default(),
                    },
                )),
                true,
            )
            .await
            .unwrap(),
    );
    assert_eq!(checked.receiver, original.receiver);
    drop(original);
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_interface(checked, "update", vec![], vec![V::Int(2)], context())
            .await
            .unwrap(),
        V::Int(12)
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(engine.heap_stats().active_handles, baseline);
}

#[tokio::test]
async fn inherited_interface_callers_retain_state_and_bound_method_ownership() {
    let engine = engine(include_str!(
        "../../../sdk_tests/fixtures/interfaces/baml_src/main.baml"
    ));
    let baseline = engine.heap_stats().active_handles;
    let counter = view(
        engine
            .call_function("make_extended_counter", vec![V::Int(10)], context(), true)
            .await
            .unwrap(),
    );
    assert_eq!(
        engine
            .call_interface(counter.clone(), "add", vec![], vec![V::Int(2)], context())
            .await
            .unwrap(),
        V::Int(12)
    );
    let parent = view(
        engine
            .call_function(
                "pass_counter",
                vec![value(counter.clone())],
                context(),
                true,
            )
            .await
            .unwrap(),
    );
    assert_eq!(counter.receiver, parent.receiver);
    let bound = engine
        .bind_interface_method(counter.clone(), "add", vec![])
        .await
        .unwrap();
    drop(counter);
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_callable(bound, vec![V::Int(3)], context(), true)
            .await
            .unwrap(),
        V::Int(15)
    );
    assert_eq!(
        engine
            .call_interface(parent, "current", vec![], vec![], context())
            .await
            .unwrap(),
        V::Int(15)
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(engine.heap_stats().active_handles, baseline);
}

#[tokio::test]
async fn inherited_interface_dispatch_preserves_shadowing_diamonds_and_self_view() {
    let engine = engine(
        r#"
interface Base {
    function label(self) -> string throws never { "base" }
    function again(self) -> Self throws never { self }
    function combine(self, other: Self) -> Self throws never { other }
    function echo<T>(self, value: T) -> T throws never { value }
}
interface Left requires Base {}
interface Right requires Base {}
interface Diamond requires Left, Right {}
interface Other { function label(self) -> string throws never { "other" } }
interface Ambiguous requires Base, Other {}
interface Shadow requires Base, Other { function label(self) -> string throws never { "shadow" } }
class Receiver {
    implements Base {}
    implements Left {}
    implements Right {}
    implements Diamond {}
    implements Other {}
    implements Ambiguous {}
    implements Shadow {}
}
function diamond() -> Diamond throws never { Receiver {} }
function ambiguous() -> Ambiguous throws never { Receiver {} }
function shadow() -> Shadow throws never { Receiver {} }
"#,
    );
    let diamond = view(
        engine
            .call_function("diamond", vec![], context(), true)
            .await
            .unwrap(),
    );
    let again = view(
        engine
            .call_interface(diamond.clone(), "again", vec![], vec![], context())
            .await
            .unwrap(),
    );
    assert_eq!(
        diamond.interface, again.interface,
        "bare Self retains the original root view"
    );
    assert_eq!(diamond.receiver, again.receiver);
    assert_eq!(
        engine
            .call_interface(diamond.clone(), "label", vec![], vec![], context())
            .await
            .unwrap(),
        V::String("base".into())
    );
    assert_eq!(
        engine
            .call_interface(
                diamond.clone(),
                "echo",
                vec![RuntimeTy::int()],
                vec![V::Int(7)],
                context()
            )
            .await
            .unwrap(),
        V::Int(7)
    );
    let error = engine
        .bind_interface_method(diamond, "combine", vec![])
        .await
        .unwrap_err();
    assert!(error.to_string().contains("concrete Self"), "{error}");
    let ambiguous = view(
        engine
            .call_function("ambiguous", vec![], context(), true)
            .await
            .unwrap(),
    );
    let error = engine
        .bind_interface_method(ambiguous, "label", vec![])
        .await
        .unwrap_err();
    assert!(error.to_string().contains("ambiguous"), "{error}");
    let shadow = view(
        engine
            .call_function("shadow", vec![], context(), true)
            .await
            .unwrap(),
    );
    assert_eq!(
        engine
            .call_interface(shadow, "label", vec![], vec![], context())
            .await
            .unwrap(),
        V::String("shadow".into())
    );
}

#[tokio::test]
async fn inherited_interface_dispatch_resolves_transitive_associated_types() {
    let engine = engine(include_str!(
        "../../../interface_probes/baml/required_interfaces.baml"
    ));
    let root = view(
        engine
            .call_function("required_root", vec![], context(), true)
            .await
            .unwrap(),
    );
    assert_eq!(
        engine
            .call_interface(
                root,
                "apply",
                vec![],
                vec![V::String("Ada".into())],
                context()
            )
            .await
            .unwrap(),
        V::String("Ada".into())
    );
    let middle = view(
        engine
            .call_function("required_middle", vec![], context(), true)
            .await
            .unwrap(),
    );
    assert_eq!(
        engine
            .call_interface(
                middle,
                "apply",
                vec![],
                vec![V::String("Grace".into())],
                context()
            )
            .await
            .unwrap(),
        V::String("Grace".into())
    );
}

#[tokio::test]
async fn iterator_required_iter_method_shares_the_existing_cursor() {
    let engine = engine(include_str!(
        "../../../sdk_tests/fixtures/interfaces/baml_src/main.baml"
    ));
    let items = view(
        engine
            .call_function(
                "as_string_iterable",
                vec![V::Array {
                    element_type: RuntimeTy::string(),
                    items: vec![V::String("Ada".into()), V::String("Grace".into())],
                }],
                context(),
                true,
            )
            .await
            .unwrap(),
    );
    let iterator = view(
        engine
            .call_interface(items, "iter", vec![], vec![], context())
            .await
            .unwrap(),
    );
    let repeated = view(
        engine
            .call_interface(iterator.clone(), "iter", vec![], vec![], context())
            .await
            .unwrap(),
    );
    assert_eq!(iterator.receiver, repeated.receiver);
    let assert_item = |result: V, expected: &str| {
        let V::Union { value, metadata } = result else {
            panic!("next returns the declared Item | Done union");
        };
        assert_eq!(metadata.selected_option, RuntimeTy::string());
        assert_eq!(*value, V::String(expected.into()));
    };
    assert_item(
        engine
            .call_interface(iterator, "next", vec![], vec![], context())
            .await
            .unwrap(),
        "Ada",
    );
    assert_item(
        engine
            .call_interface(repeated, "next", vec![], vec![], context())
            .await
            .unwrap(),
        "Grace",
    );
}

const COUNTER: &str = r#"
interface Counter {
    function add(self, amount: int) -> int throws never
    function current(self) -> int throws never
    function twice(self, amount: int) -> int throws never {
        self.add(amount);
        self.add(amount)
    }
}
class StoredCounter {
    count: int,
    implements Counter {
        function add(self, amount: int) -> int throws never { self.count += amount; self.count }
        function current(self) -> int throws never { self.count }
    }
}
class Record { label: string, counter: Counter }
function make() -> Counter { StoredCounter { count: 0 } }
function record(counter: Counter) -> Record { Record { label: "copied", counter } }
function bump(counter: Counter) -> int throws never { counter.add(1) }
"#;

#[tokio::test]
async fn retained_interface_preserves_default_dispatch_state_and_nested_identity() {
    let engine = engine(COUNTER);
    let baseline = engine.heap_stats().active_handles;
    let counter = view(
        engine
            .call_function("make", vec![], context(), true)
            .await
            .unwrap(),
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_interface(counter.clone(), "twice", vec![], vec![V::Int(2)], context())
            .await
            .unwrap(),
        V::Int(4)
    );
    assert_eq!(
        engine
            .call_function("bump", vec![value(counter.clone())], context(), true)
            .await
            .unwrap(),
        V::Int(5)
    );
    let record = engine
        .call_function("record", vec![value(counter.clone())], context(), true)
        .await
        .unwrap();
    let V::Instance { mut fields, .. } = record else {
        panic!("outer record should be copied");
    };
    assert_eq!(
        fields.shift_remove("label"),
        Some(V::String("copied".into()))
    );
    let child = view(fields.shift_remove("counter").unwrap());
    assert_eq!(counter.receiver, child.receiver);
    drop(counter);
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_interface(child, "current", vec![], vec![], context())
            .await
            .unwrap(),
        V::Int(5)
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(engine.heap_stats().active_handles, baseline);
}

#[tokio::test]
async fn primitive_interface_methods_specialize_without_a_class_receiver() {
    let engine = engine(
        r#"
interface Identity {
    function echo<T>(self, value: T) -> T throws never { value }
    function empty<T>(self) -> T[] throws never { [] }
}
implements Identity for int {}
function make() -> Identity { 7 }
"#,
    );
    let identity = view(
        engine
            .call_function("make", vec![], context(), true)
            .await
            .unwrap(),
    );
    assert!(
        engine
            .call_interface(
                identity.clone(),
                "echo",
                vec![],
                vec![V::String("Ada".into())],
                context()
            )
            .await
            .is_err()
    );
    assert!(
        engine
            .call_interface(
                identity.clone(),
                "echo",
                vec![RuntimeTy::string()],
                vec![V::Int(9)],
                context()
            )
            .await
            .is_err()
    );
    let callback = engine
        .bind_interface_method(identity.clone(), "echo", vec![RuntimeTy::string()])
        .await
        .unwrap();
    let empty = engine
        .bind_interface_method(identity, "empty", vec![RuntimeTy::string()])
        .await
        .unwrap();
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_callable(callback, vec![V::String("Ada".into())], context(), true)
            .await
            .unwrap(),
        V::String("Ada".into())
    );
    assert_eq!(
        engine
            .call_callable(empty, vec![], context(), true)
            .await
            .unwrap(),
        V::Array {
            element_type: RuntimeTy::string(),
            items: vec![]
        }
    );
}

#[tokio::test]
async fn array_interface_retains_owner_while_concrete_results_are_copied() {
    let engine = engine(
        r#"
interface Words {
    function append(self, text: string) -> int throws never
    function copy(self) -> string[] throws never
}
implements Words for string[] {
    function append(self, text: string) -> int throws never { self.push(text); self.length() }
    function copy(self) -> string[] throws never { self }
}
function make() -> Words { ["Ada"] }
function bump(words: Words) -> int throws never { words.append("Grace") }
"#,
    );
    let words = view(
        engine
            .call_function("make", vec![], context(), true)
            .await
            .unwrap(),
    );
    let copied = engine
        .call_interface(words.clone(), "copy", vec![], vec![], context())
        .await
        .unwrap();
    let V::Array { mut items, .. } = copied else {
        panic!("expected data array");
    };
    items.push(V::String("local edit".into()));
    assert_eq!(
        engine
            .call_function("bump", vec![value(words.clone())], context(), true)
            .await
            .unwrap(),
        V::Int(2)
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_interface(words, "copy", vec![], vec![], context())
            .await
            .unwrap(),
        V::Array {
            element_type: RuntimeTy::string(),
            items: vec![V::String("Ada".into()), V::String("Grace".into())],
        }
    );
}

#[tokio::test]
async fn interface_rejects_foreign_runtime_and_unavailable_member() {
    let first = engine(COUNTER);
    let second = engine(COUNTER);
    let counter = view(
        first
            .call_function("make", vec![], context(), true)
            .await
            .unwrap(),
    );
    assert!(
        second
            .call_interface(counter.clone(), "current", vec![], vec![], context())
            .await
            .is_err()
    );
    assert!(
        second
            .call_function("bump", vec![value(counter.clone())], context(), true)
            .await
            .is_err()
    );
    assert!(
        first
            .call_interface(counter.clone(), "count", vec![], vec![], context())
            .await
            .is_err()
    );
    assert_eq!(
        first
            .call_interface(counter, "current", vec![], vec![], context())
            .await
            .unwrap(),
        V::Int(0)
    );
}

#[tokio::test]
async fn interface_contract_controls_arguments_results_and_self() {
    let engine = engine(
        r#"
interface Child {
    function label(self) -> string throws never
}
class NamedChild {
    name: string,
    implements Child { function label(self) -> string throws never { self.name } }
}
interface Factory {
    function accept(self, text: string) -> int throws never
    function count(self) -> int throws never
    function child(self) -> Child throws never
    function same(self) -> Self throws never
}
class StoredFactory {
    calls: int,
    implements Factory {
        function accept(self, text: unknown) -> int throws never { self.calls += 1; self.calls }
        function count(self) -> int throws never { self.calls }
        function child(self) -> NamedChild throws never { NamedChild { name: "Ada" } }
        function same(self) -> Self throws never { self }
    }
}
function make() -> Factory { StoredFactory { calls: 0 } }
function invoke(f: (string) -> int throws never, text: string) -> int throws never {
    assert.equal(reflect.signature(f).args[0].type, reflect.Type.of<string>());
    f(text)
}
"#,
    );
    let baseline = engine.heap_stats().active_handles;
    let factory = view(
        engine
            .call_function("make", vec![], context(), true)
            .await
            .unwrap(),
    );
    let accept = engine
        .bind_interface_method(factory.clone(), "accept", vec![])
        .await
        .unwrap();
    assert!(
        engine
            .call_callable(accept.clone(), vec![V::Int(9)], context(), true)
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
    let child = view(
        engine
            .call_interface(factory.clone(), "child", vec![], vec![], context())
            .await
            .unwrap(),
    );
    let same = view(
        engine
            .call_interface(factory.clone(), "same", vec![], vec![], context())
            .await
            .unwrap(),
    );
    assert_eq!(same.receiver, factory.receiver);
    assert_eq!(same.interface, factory.interface);
    assert_eq!(
        engine
            .call_function(
                "invoke",
                vec![V::Handle(accept.clone()), V::String("pass back".into())],
                context(),
                true
            )
            .await
            .unwrap(),
        V::Int(1)
    );
    drop(factory);
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_interface(child, "label", vec![], vec![], context())
            .await
            .unwrap(),
        V::String("Ada".into())
    );
    assert_eq!(
        engine
            .call_callable(accept, vec![V::String("valid".into())], context(), true)
            .await
            .unwrap(),
        V::Int(2)
    );
    assert_eq!(
        engine
            .call_interface(same, "count", vec![], vec![], context())
            .await
            .unwrap(),
        V::Int(2)
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(engine.heap_stats().active_handles, baseline);
}

#[tokio::test]
async fn interface_contract_keeps_associated_and_method_type_parameters_distinct() {
    let engine = engine(
        r#"
interface Source<T> {
    type Output
    function get(self, input: T) -> Self.Output throws never
    function echo<U>(self, value: U) -> U throws never { value }
    function again(self, input: T) -> Self.Output throws never { self.get(input) }
}
class IntSource {
    implements Source<string> {
        type Output = int
        function get(self, input: string) -> int throws never { input.length() }
    }
}
function make() -> Source<string, Output=int> { IntSource {} }
"#,
    );
    let source = view(
        engine
            .call_function("make", vec![], context(), true)
            .await
            .unwrap(),
    );
    assert_eq!(
        engine
            .call_interface(
                source.clone(),
                "again",
                vec![],
                vec![V::String("Ada".into())],
                context()
            )
            .await
            .unwrap(),
        V::Int(3)
    );
    assert_eq!(
        engine
            .call_interface(
                source.clone(),
                "echo",
                vec![RuntimeTy::bool()],
                vec![V::Bool(true)],
                context()
            )
            .await
            .unwrap(),
        V::Bool(true)
    );
    assert!(
        engine
            .call_interface(source, "get", vec![], vec![V::Int(3)], context())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn interface_cffi_transfer_retains_roots_until_last_release() {
    use bridge_ctypes::{
        CffiHandleTableOptions, HANDLE_TABLE,
        baml_bridge::cffi::{
            BamlHandle, BamlHandleType, InboundValue, baml_outbound_value, inbound_value,
        },
        external_to_outbound, inbound_to_external,
    };
    let engine = engine(COUNTER);
    let baseline = engine.heap_stats().active_handles;
    let counter = view(
        engine
            .call_function("make", vec![], context(), true)
            .await
            .unwrap(),
    );
    let encoded = external_to_outbound(
        &value(counter.clone()),
        &CffiHandleTableOptions::for_in_process(),
    )
    .unwrap();
    let Some(baml_outbound_value::Value::HandleValue(wire)) = encoded.value else {
        panic!("expected handle");
    };
    assert_eq!(wire.handle_type, BamlHandleType::AdtInterface as i32);
    assert!(wire.ty.is_some());
    let receiver = counter.receiver.clone();
    drop(counter);
    let owned_clone = HANDLE_TABLE.clone_handle(wire.key).unwrap();
    assert!(HANDLE_TABLE.release(wire.key));
    engine.collect_garbage(CollectionLevel::Major).await;
    let decoded = inbound_to_external(
        InboundValue {
            value_type: None,
            value: Some(inbound_value::Value::Handle(BamlHandle {
                key: owned_clone,
                handle_type: BamlHandleType::AdtInterface as i32,
            })),
        },
        &HANDLE_TABLE,
    )
    .unwrap();
    assert!(
        HANDLE_TABLE.resolve(owned_clone).is_none(),
        "decode consumes wire ownership"
    );
    let counter = view(decoded);
    assert_eq!(counter.receiver, receiver);
    drop(receiver);
    assert_eq!(
        engine
            .call_interface(counter, "add", vec![], vec![V::Int(2)], context())
            .await
            .unwrap(),
        V::Int(2)
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(engine.heap_stats().active_handles, baseline);
}

#[tokio::test]
async fn interface_rejects_incomplete_pins_and_concrete_self_operations() {
    let engine = engine(
        r#"
interface Restricted {
    type Output
    function get(self) -> Self.Output throws never
    function combine(self, other: Self) -> Self throws never
    function construct() -> Self throws never
    function nested(self) -> Self[] throws never
}
implements Restricted for int {
    type Output = string
    function get(self) -> string throws never { self.to_string() }
    function combine(self, other: Self) -> Self throws never { self + other }
    function construct() -> Self throws never { 1 }
    function nested(self) -> Self[] throws never { [self] }
}
function make() -> Restricted<Output=string> { 7 }
"#,
    );
    let valid = view(
        engine
            .call_function("make", vec![], context(), true)
            .await
            .unwrap(),
    );
    for method in ["combine", "construct", "nested"] {
        assert!(
            engine
                .bind_interface_method(valid.clone(), method, vec![])
                .await
                .is_err(),
            "{method} must require a concrete projection"
        );
    }
    let wire_ty = valid
        .interface
        .as_runtime_ty()
        .try_map_heads(&mut |head| Ok::<_, std::convert::Infallible>(head.name().overlay_name()))
        .unwrap();
    for case in ["missing", "wrong", "duplicate"] {
        let mut ty = wire_ty.clone();
        let RuntimeTy::Interface(_, _, pins, _) = &mut ty else {
            unreachable!()
        };
        match case {
            "missing" => pins.clear(),
            "wrong" => pins[0].1 = RuntimeTy::int(),
            "duplicate" => pins.push(pins[0].clone()),
            _ => unreachable!(),
        }
        assert!(
            engine
                .project_interface(value(valid.clone()), ty)
                .await
                .is_err(),
            "{case} pins must fail"
        );
    }
    assert_eq!(
        engine
            .call_interface(valid, "get", vec![], vec![], context())
            .await
            .unwrap(),
        V::String("7".into())
    );
}

#[tokio::test]
async fn checked_method_maps_named_optionals_to_implementation_slots() {
    let engine = engine(
        r#"
interface Formatter {
    function format(self, value: string, prefix: string = "Hello", suffix: string = "!") -> string throws never { prefix + value + suffix }
}
class Fancy {
    implements Formatter {
        function format(self, text: unknown, suffix: string = "?", extra: string = ".", prefix: string = "Hi") -> string throws never {
            prefix + text.to_string() + suffix + extra
        }
    }
}
function make() -> Formatter { Fancy {} }
function invoke(f: (value: string, prefix?: string, suffix?: string) -> string throws never) -> string throws never {
    f("Ada", prefix = "<", suffix = ">")
}
"#,
    );
    let formatter = view(
        engine
            .call_function("make", vec![], context(), true)
            .await
            .unwrap(),
    );
    let method = engine
        .bind_interface_method(formatter, "format", vec![])
        .await
        .unwrap();
    assert_eq!(
        engine
            .call_callable_named(
                method.clone(),
                indexmap::indexmap! { "value".into() => V::String("Ada".into()) },
                indexmap::IndexMap::new(),
                context(),
                true
            )
            .await
            .unwrap(),
        V::String("HiAda?.".into())
    );
    assert_eq!(engine.call_callable_named(method.clone(), indexmap::indexmap! { "value".into() => V::String("Ada".into()) }, indexmap::indexmap! { "prefix".into() => V::String("<".into()), "suffix".into() => V::String(">".into()) }, context(), true).await.unwrap(), V::String("<Ada>.".into()));
    assert!(
        engine
            .call_callable_named(
                method.clone(),
                indexmap::indexmap! { "value".into() => V::String("Ada".into()) },
                indexmap::indexmap! { "extra".into() => V::String("hidden".into()) },
                context(),
                true
            )
            .await
            .is_err()
    );
    assert_eq!(
        engine
            .call_function("invoke", vec![V::Handle(method)], context(), true)
            .await
            .unwrap(),
        V::String("<Ada>.".into())
    );
}

#[tokio::test]
async fn checked_method_narrower_callable_and_native_arguments() {
    let engine = engine(
        r#"
interface Formatter {
    function format(self, value: string, prefix: string = "Hello") -> string throws never { prefix + value }
}
class Fancy {
    implements Formatter {
        function format(self, value: string, extra: string = ".", prefix: string = "Hi") -> string throws never { prefix + value + extra }
    }
}
function make() -> Formatter { Fancy {} }
function invoke_narrow(f: (string) -> string throws never) -> string throws never { f("Ada") }
function invoke_native(f: (string) -> string throws never) -> string[] throws never { ["Ada"].map(f) }
function invoke_interface(f: Formatter) -> string { f.format("Ada", prefix = "<", $id = boundary.id()) }
function invoke_reflected(f: reflect.AnyFunction<Returns = string, Throws = never>) -> string {
    reflect.call_any(f, { "value": "Ada", "prefix": "<" })
}
"#,
    );
    let formatter = view(
        engine
            .call_function("make", vec![], context(), true)
            .await
            .unwrap(),
    );
    let method = engine
        .bind_interface_method(formatter.clone(), "format", vec![])
        .await
        .unwrap();
    let direct = engine
        .call_function("invoke_interface", vec![value(formatter)], context(), true)
        .await;
    let narrow = engine
        .call_function(
            "invoke_narrow",
            vec![V::Handle(method.clone())],
            context(),
            true,
        )
        .await;
    let reflected = engine
        .call_function(
            "invoke_reflected",
            vec![V::Handle(method.clone())],
            context(),
            true,
        )
        .await;
    let native = engine
        .call_function("invoke_native", vec![V::Handle(method)], context(), true)
        .await;
    let expected_native = V::Array {
        element_type: RuntimeTy::string(),
        items: vec![V::String("HiAda.".into())],
    };
    assert!(
        matches!(&reflected, Ok(V::String(s)) if s.as_str() == "<Ada.")
            && matches!(&direct, Ok(V::String(s)) if s.as_str() == "<Ada.")
            && matches!(&narrow, Ok(V::String(s)) if s.as_str() == "HiAda.")
            && matches!(&native, Ok(value) if value == &expected_native),
        "reflected: {reflected:?}\ndirect interface: {direct:?}\nnarrowed callable: {narrow:?}\nnative higher-order call: {native:?}",
    );
}

#[tokio::test]
async fn runtime_created_receiver_retains_implementation_and_passes_back() {
    let source = format!(
        r#"{COUNTER}
function make_runtime() -> Counter {{
    let package = reflect.Package.compile({{ "counter.baml": `
class RuntimeCounter {{
    count: int,
    implements app.Counter {{
        function add(self, amount: int) -> int throws never {{ self.count += amount; self.count }}
        function current(self) -> int throws never {{ self.count }}
    }}
}}
function make() -> app.Counter {{ RuntimeCounter {{ count: 10 }} }}
` }}, packages = {{ "app": reflect.Package.current() }})
    let make = package.get_function<() -> Counter>("root.make") ?? throw "missing make"
    make()
}}
"#
    );
    let engine = Arc::new(
        BexEngine::new_with_runtime_compiler(
            common::compile_for_engine(&source),
            Arc::new(sys_native::SysOps::native()),
            vec![],
            bex_project::runtime_compiler(),
        )
        .unwrap(),
    );
    let baseline = engine.heap_stats().active_handles;
    let counter = view(
        engine
            .call_function("make_runtime", vec![], context(), true)
            .await
            .unwrap(),
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    let method = engine
        .bind_interface_method(counter.clone(), "twice", vec![])
        .await
        .unwrap();
    let passed_back = engine
        .call_function("bump", vec![value(counter.clone())], context(), true)
        .await;
    let called = engine
        .call_callable(method, vec![V::Int(2)], context(), true)
        .await;
    assert_eq!(passed_back.unwrap(), V::Int(11));
    assert_eq!(called.unwrap(), V::Int(15));
    drop(counter);
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(engine.heap_stats().active_handles, baseline);
}

#[tokio::test]
async fn runtime_created_interface_return_preserves_exact_declaration() {
    let engine = Arc::new(
        BexEngine::new_with_runtime_compiler(
            common::compile_for_engine(
                r#"
type Factory = () -> unknown throws never
function make_factory() -> Factory {
    let package = reflect.Package.compile({ "named.baml": `
interface Label {
    function label(self) -> string throws never
}
interface Named requires Label {
    function children(self) -> Named[] throws never
    function index(self) -> map<string, Named> throws never
    function choose(self) -> Named | int throws never
    function fail(self) -> null throws Named
}


class Stored {
    implements Label {
        function label(self) -> string throws never { "runtime" }
    }
    implements Named {
        function children(self) -> Named[] throws never { [self] }
        function index(self) -> map<string, Named> throws never { { "self": self } }
        function choose(self) -> Named | int throws never { self }
        function fail(self) -> null throws Named { throw self }
    }
}
function make() -> Named throws never { Stored {} }
` })
    package.get_function<Factory>("root.make") ?? throw "missing make"
}
"#,
            ),
            Arc::new(sys_native::SysOps::native()),
            vec![],
            bex_project::runtime_compiler(),
        )
        .unwrap(),
    );
    let baseline = engine.heap_stats().active_handles;
    let factory = engine
        .call_function("make_factory", vec![], context(), true)
        .await
        .unwrap();
    let factory = match factory {
        V::Handle(handle)
        | V::Adt(BexExternalAdt::TaggedHeapHandle {
            heap_handle: handle,
            ..
        }) => handle,
        other => panic!("expected a callable handle, got {other:?}"),
    };
    engine.collect_garbage(CollectionLevel::Major).await;
    let value = view(
        engine
            .call_callable(factory, vec![], context(), true)
            .await
            .unwrap(),
    );
    assert_eq!(
        engine
            .call_interface(value.clone(), "label", vec![], vec![], context())
            .await
            .unwrap(),
        V::String("runtime".into())
    );
    let children = engine
        .call_interface(value.clone(), "children", vec![], vec![], context())
        .await
        .unwrap();
    let V::Array { mut items, .. } = children else {
        panic!("expected array")
    };
    assert_eq!(view(items.remove(0)).interface, value.interface);
    let index = engine
        .call_interface(value.clone(), "index", vec![], vec![], context())
        .await
        .unwrap();
    let V::Map { mut entries, .. } = index else {
        panic!("expected map")
    };
    assert_eq!(
        view(entries.shift_remove("self").unwrap()).interface,
        value.interface
    );
    let chosen = engine
        .call_interface(value.clone(), "choose", vec![], vec![], context())
        .await
        .unwrap();
    let V::Union { value: chosen, .. } = chosen else {
        panic!("expected union")
    };
    assert_eq!(view(*chosen).interface, value.interface);
    let failure = engine
        .call_interface(value.clone(), "fail", vec![], vec![], context())
        .await
        .unwrap_err();
    let bex_engine::EngineError::UnhandledThrow { value: thrown, .. } = failure else {
        panic!("expected declared throw")
    };
    assert_eq!(view(*thrown).interface, value.interface);

    // Every compile creates fresh declarations, even with identical source
    // spellings. Both views must keep their own declaration and implementation.
    let V::Adt(BexExternalAdt::TaggedHeapHandle {
        heap_handle: factory,
        ..
    }) = engine
        .call_function("make_factory", vec![], context(), true)
        .await
        .unwrap()
    else {
        panic!("expected callable")
    };
    let other = view(
        engine
            .call_callable(factory, vec![], context(), true)
            .await
            .unwrap(),
    );
    assert_ne!(value.interface, other.interface);
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_interface(other.clone(), "label", vec![], vec![], context())
            .await
            .unwrap(),
        V::String("runtime".into())
    );
    drop(other);
    drop(value);
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(engine.heap_stats().active_handles, baseline);
}

#[tokio::test]
async fn runtime_mount_preserves_associated_and_generic_methods() {
    let engine = Arc::new(BexEngine::new_with_runtime_compiler(common::compile_for_engine(r#"
interface Label { function label(self) -> string throws never }
class Input {
    implements Label { function label(self) -> string throws never { "Ada" } }
}
interface Source<T extends Label> {
    type Output
    function get(self, input: T) -> Self.Output throws never
    function echo<U>(self, value: U) -> U throws never { value }
    function again(self, input: T) -> Self.Output throws never { self.get(input) }
}
function make_runtime() -> Source<Input, Output=int> {
    let package = reflect.Package.compile({ "source.baml": `
class RuntimeSource {
    implements app.Source<app.Input> {
        type Output = int
        function get(self, input: app.Input) -> int throws never { input.label().length() }
    }
}
function make() -> app.Source<app.Input, Output=int> throws never { RuntimeSource {} }
` }, packages = { "app": reflect.Package.current() })
    let make = package.get_function<() -> Source<Input, Output=int>>("root.make") ?? throw "missing make"
    make()
}
function use(source: Source<Input, Output=int>) -> int throws never { source.again(Input {}) }
"#), Arc::new(sys_native::SysOps::native()), vec![], bex_project::runtime_compiler()).unwrap());
    let baseline = engine.heap_stats().active_handles;
    let source = view(
        engine
            .call_function("make_runtime", vec![], context(), true)
            .await
            .unwrap(),
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_function("use", vec![value(source.clone())], context(), true)
            .await
            .unwrap(),
        V::Int(3)
    );
    assert_eq!(
        engine
            .call_interface(
                source.clone(),
                "echo",
                vec![RuntimeTy::bool()],
                vec![V::Bool(true)],
                context()
            )
            .await
            .unwrap(),
        V::Bool(true)
    );
    drop(source);
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(engine.heap_stats().active_handles, baseline);
}

#[tokio::test]
async fn runtime_interface_inputs_use_exact_declarations_before_execution() {
    let engine = Arc::new(BexEngine::new_with_runtime_compiler(common::compile_for_engine(r#"
type Factory = () -> unknown throws never
function factory() -> Factory {
    let package = reflect.Package.compile({ "peer.baml": `
interface Peer {
    function visit(self, other: Peer) -> int throws never
    function visit_many(self, others: Peer[]) -> int throws never
    function current(self) -> int throws never
    function visit_map(self, others: map<string, Peer>) -> int throws never
    function visit_record(self, pair: Pair) -> int throws never
    function visit_maybe(self, other: Peer?) -> int throws never
    function visit_nested(self, others: map<string, Peer?[]>?) -> int throws never
    function visit_selection(self, others: Peer[] | int[]) -> int throws never
    function visit_record_maybe(self, pair: Pair?) -> int throws never
}
class Pair { peer: Peer }
class Stored {
    calls: int,
    implements Peer {
        function visit(self, other: Peer) -> int throws never { self.calls += 1; self.calls }
        function visit_many(self, others: Peer[]) -> int throws never { self.calls += others.length(); self.calls }
        function current(self) -> int throws never { self.calls }
        function visit_map(self, others: map<string, Peer>) -> int throws never { self.calls += others.length(); self.calls }
        function visit_record(self, pair: Pair) -> int throws never { self.calls += 1; self.calls }
        function visit_maybe(self, other: Peer?) -> int throws never { if (other != null) { self.calls += 1; }; self.calls }
        function visit_nested(self, others: map<string, Peer?[]>?) -> int throws never { self.calls += 1; self.calls }
        function visit_selection(self, others: Peer[] | int[]) -> int throws never { self.calls += 1; self.calls }
        function visit_record_maybe(self, pair: Pair?) -> int throws never { self.calls += 1; self.calls }
    }
}
function make() -> Peer throws never { Stored { calls: 0 } }
` })
    package.get_function<Factory>("root.make") ?? throw "missing make"
}
"#), Arc::new(sys_native::SysOps::native()), vec![], bex_project::runtime_compiler()).unwrap());
    let baseline = engine.heap_stats().active_handles;
    async fn make(engine: &Arc<BexEngine>) -> Arc<InterfaceValue> {
        let V::Adt(BexExternalAdt::TaggedHeapHandle { heap_handle, .. }) = engine
            .call_function("factory", vec![], context(), true)
            .await
            .unwrap()
        else {
            panic!("expected callable")
        };
        view(
            engine
                .call_callable(heap_handle, vec![], context(), true)
                .await
                .unwrap(),
        )
    }
    let peer = make(&engine).await;
    let unrelated = make(&engine).await;
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_interface(
                peer.clone(),
                "visit",
                vec![],
                vec![value(peer.clone())],
                context()
            )
            .await
            .unwrap(),
        V::Int(1)
    );
    assert!(
        engine
            .call_interface(
                peer.clone(),
                "visit",
                vec![],
                vec![value(unrelated.clone())],
                context()
            )
            .await
            .is_err()
    );
    // Concrete array storage is copied; its live children must retain their
    // exact identity. A late invalid child must not enter the receiver body.
    let array = |items| V::Array {
        element_type: RuntimeTy::unknown(),
        items,
    };
    assert_eq!(
        engine
            .call_interface(
                peer.clone(),
                "visit_many",
                vec![],
                vec![array(vec![value(peer.clone()), value(peer.clone())])],
                context()
            )
            .await
            .unwrap(),
        V::Int(3)
    );
    let roots = engine.heap_stats().active_handles;
    assert!(
        engine
            .call_interface(
                peer.clone(),
                "visit_many",
                vec![],
                vec![array(vec![value(peer.clone()), value(unrelated.clone())])],
                context()
            )
            .await
            .is_err()
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(engine.heap_stats().active_handles, roots);
    assert_eq!(
        engine
            .call_interface(peer.clone(), "current", vec![], vec![], context())
            .await
            .unwrap(),
        V::Int(3)
    );
    let map = V::Map {
        key_type: RuntimeTy::string(),
        value_type: RuntimeTy::unknown(),
        entries: indexmap::indexmap! { "one".into() => value(peer.clone()), "two".into() => value(peer.clone()) },
    };
    assert_eq!(
        engine
            .call_interface(peer.clone(), "visit_map", vec![], vec![map], context())
            .await
            .unwrap(),
        V::Int(5)
    );
    let pair = |child| V::Instance {
        class_name: "user.Pair".into(),
        type_args: vec![],
        fields: indexmap::indexmap! { "peer".into() => child },
    };
    assert_eq!(
        engine
            .call_interface(
                peer.clone(),
                "visit_record",
                vec![],
                vec![pair(value(peer.clone()))],
                context()
            )
            .await
            .unwrap(),
        V::Int(6)
    );
    assert!(
        engine
            .call_interface(
                peer.clone(),
                "visit_record",
                vec![],
                vec![pair(value(unrelated.clone()))],
                context()
            )
            .await
            .is_err()
    );
    assert_eq!(
        engine
            .call_interface(
                peer.clone(),
                "visit_maybe",
                vec![],
                vec![V::Null],
                context()
            )
            .await
            .unwrap(),
        V::Int(6)
    );
    assert_eq!(
        engine
            .call_interface(
                peer.clone(),
                "visit_maybe",
                vec![],
                vec![value(peer.clone())],
                context()
            )
            .await
            .unwrap(),
        V::Int(7)
    );
    assert!(
        engine
            .call_interface(
                peer.clone(),
                "visit_maybe",
                vec![],
                vec![value(unrelated.clone())],
                context()
            )
            .await
            .is_err()
    );
    let nested = |child| V::Map {
        key_type: RuntimeTy::string(),
        value_type: RuntimeTy::unknown(),
        entries: indexmap::indexmap! {
            "peers".into() => array(vec![V::Null, child]),
        },
    };
    assert_eq!(
        engine
            .call_interface(
                peer.clone(),
                "visit_nested",
                vec![],
                vec![nested(value(peer.clone()))],
                context()
            )
            .await
            .unwrap(),
        V::Int(8)
    );
    assert!(
        engine
            .call_interface(
                peer.clone(),
                "visit_nested",
                vec![],
                vec![nested(value(unrelated.clone()))],
                context()
            )
            .await
            .is_err()
    );
    assert_eq!(
        engine
            .call_interface(
                peer.clone(),
                "visit_selection",
                vec![],
                vec![array(vec![value(peer.clone())])],
                context()
            )
            .await
            .unwrap(),
        V::Int(9)
    );
    assert_eq!(
        engine
            .call_interface(
                peer.clone(),
                "visit_selection",
                vec![],
                vec![array(vec![V::Int(3)])],
                context()
            )
            .await
            .unwrap(),
        V::Int(10)
    );
    assert!(
        engine
            .call_interface(
                peer.clone(),
                "visit_selection",
                vec![],
                vec![array(vec![value(unrelated.clone())])],
                context()
            )
            .await
            .is_err()
    );
    assert_eq!(
        engine
            .call_interface(
                peer.clone(),
                "visit_record_maybe",
                vec![],
                vec![pair(value(peer.clone()))],
                context()
            )
            .await
            .unwrap(),
        V::Int(11)
    );
    assert!(
        engine
            .call_interface(
                peer.clone(),
                "visit_record_maybe",
                vec![],
                vec![pair(value(unrelated.clone()))],
                context()
            )
            .await
            .is_err()
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_interface(peer.clone(), "current", vec![], vec![], context())
            .await
            .unwrap(),
        V::Int(11)
    );
    drop(peer);
    drop(unrelated);
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(engine.heap_stats().active_handles, baseline);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn named_generic_calls_preserve_live_inference_and_reject_conflicts_before_execution() {
    let source = format!(
        r#"{COUNTER}
interface Source {{
    type Output
    function item(self) -> Self.Output throws never
}}
function read_output<O>(source: Source<Output=O>) -> O throws never {{ source.item() }}
type Factory = () -> unknown throws never
function factory(target: string) -> Factory {{
    let package = reflect.Package.compile({{ "named.baml": `
interface Named {{ function label(self) -> string throws never }}
class Stored {{
    implements Named {{ function label(self) -> string throws never {{ "named" }} }}
    implements app.Source {{
        type Output = Named
        function item(self) -> Named throws never {{ self }}
    }}
}}
function make() -> Named throws never {{ Stored {{}} }}
function make_source() -> app.Source<Output=Named> throws never {{ Stored {{}} }}
` }}, packages = {{ "app": reflect.Package.current() }})
    package.get_function<Factory>(target) ?? throw "missing make"
}}
function identity<T>(value: T) -> T throws never {{ value }}
function optional<T>(value: T?) -> T? throws never {{ value }}
function record_pair<T>(counter: Counter, left: T[], right: T[]) -> int throws never {{ counter.add(1) }}
"#
    );
    let engine = Arc::new(
        BexEngine::new_with_runtime_compiler(
            common::compile_for_engine(&source),
            Arc::new(sys_native::SysOps::native()),
            vec![],
            bex_project::runtime_compiler(),
        )
        .unwrap(),
    );
    async fn make(engine: &Arc<BexEngine>) -> Arc<InterfaceValue> {
        let V::Adt(BexExternalAdt::TaggedHeapHandle { heap_handle, .. }) = engine
            .call_function(
                "factory",
                vec![V::String("root.make".into())],
                context(),
                true,
            )
            .await
            .unwrap()
        else {
            panic!("expected factory")
        };
        view(
            engine
                .call_callable(heap_handle, vec![], context(), true)
                .await
                .unwrap(),
        )
    }
    let baseline = engine.heap_stats().active_handles;
    let first = make(&engine).await;
    let second = make(&engine).await;
    let counter = view(
        engine
            .call_function("make", vec![], context(), true)
            .await
            .unwrap(),
    );
    for name in ["identity", "optional"] {
        let output = engine
            .call_function(name, vec![value(first.clone())], context(), true)
            .await
            .unwrap();
        let output = match output {
            V::Union { value, .. } => *value,
            other => other,
        };
        let output = view(output);
        assert_eq!(output.interface, first.interface);
        assert_eq!(output.receiver, first.receiver);
        assert_eq!(
            engine
                .call_interface(output, "label", vec![], vec![], context())
                .await
                .unwrap(),
            V::String("named".into())
        );
    }
    let V::Adt(BexExternalAdt::TaggedHeapHandle { heap_handle, .. }) = engine
        .call_function(
            "factory",
            vec![V::String("root.make_source".into())],
            context(),
            true,
        )
        .await
        .unwrap()
    else {
        panic!("expected source factory")
    };
    let source = view(
        engine
            .call_callable(heap_handle, vec![], context(), true)
            .await
            .unwrap(),
    );
    let expected = view(
        engine
            .call_interface(source.clone(), "item", vec![], vec![], context())
            .await
            .unwrap(),
    );
    let output = view(
        engine
            .call_function("read_output", vec![value(source.clone())], context(), true)
            .await
            .unwrap(),
    );
    assert_eq!(output.interface, expected.interface);
    assert_eq!(output.receiver, expected.receiver);
    drop(output);
    drop(expected);
    drop(source);
    let array = |v: Arc<InterfaceValue>| V::Array {
        element_type: RuntimeTy::unknown(),
        items: vec![value(v)],
    };
    assert_eq!(
        engine
            .call_function(
                "record_pair",
                vec![
                    value(counter.clone()),
                    array(first.clone()),
                    array(first.clone())
                ],
                context(),
                true
            )
            .await
            .unwrap(),
        V::Int(1)
    );
    let conflict = engine
        .call_function(
            "record_pair",
            vec![
                value(counter.clone()),
                array(first.clone()),
                array(second.clone()),
            ],
            context(),
            true,
        )
        .await;
    assert!(
        matches!(conflict, Err(bex_engine::EngineError::TypeMismatch { .. })),
        "different declarations must conflict: {conflict:?}"
    );
    let explicit = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_type_args(indexmap::indexmap! { "T".into() => RuntimeTy::int() })
        .build();
    let wrong = engine
        .call_function(
            "record_pair",
            vec![
                value(counter.clone()),
                array(first.clone()),
                array(first.clone()),
            ],
            explicit,
            true,
        )
        .await;
    assert!(
        matches!(wrong, Err(bex_engine::EngineError::TypeMismatch { .. })),
        "explicit type must win: {wrong:?}"
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_interface(counter.clone(), "current", vec![], vec![], context())
            .await
            .unwrap(),
        V::Int(1)
    );
    drop(first);
    drop(second);
    drop(counter);
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(engine.heap_stats().active_handles, baseline);
}

const COPIED_INPUT: &str = r#"
interface Tag {
    type Item
    type Error = never
}
class TaggedRecord<T> {
    child: Counter?,
    value: T,
    implements Tag { type Item = T }
}
class OtherRecord { value: string }
function copied() -> TaggedRecord<string> throws never {
    TaggedRecord<string> { value: "Ada", child: null }
}
function use_tag(value: Tag<Item=string, Error=never>, effects: Counter) -> string throws never {
    effects.add(1); "tagged"
}
function optional_tag(value: Tag<Item=string, Error=never>?) -> string throws never {
    match (value) { null => "none", _ => "tagged" }
}
function child_count(value: Tag<Item=string, Error=never>) -> int throws never {
    match (value) { let record: TaggedRecord<string> => record.child?.current() ?? -1, _ => -2 }
}
function wrong_error(value: Tag<Item=string, Error=string>, effects: Counter) -> string throws never {
    effects.add(1); "entered"
}
function retain_tag(value: Tag<Item=string, Error=never>) -> Tag<Item=string, Error=never> throws never {
    value
}
"#;

fn copied_record_type(args: Vec<RuntimeTy>) -> RuntimeTy {
    RuntimeTy::Class(
        baml_type::TypeName::from_dotted_path("user.TaggedRecord"),
        args,
        Default::default(),
    )
}

fn copied_record(item: V, args: Vec<RuntimeTy>, child: V) -> V {
    V::typed(
        V::Instance {
            class_name: "TaggedRecord".into(),
            type_args: args.clone(),
            fields: indexmap::indexmap! {"value".into() => item, "child".into() => child},
        },
        copied_record_type(args),
    )
}

#[tokio::test]
async fn copied_records_enter_interfaces_without_changing_the_record_codec() {
    let engine = engine(&format!("{COUNTER}\n{COPIED_INPUT}"));
    let baseline = engine.heap_stats().active_handles;
    let record = engine
        .call_function("copied", vec![], context(), true)
        .await
        .unwrap();
    assert!(
        matches!(record, V::Instance { .. }),
        "record must remain copied: {record:?}"
    );
    let effect = view(
        engine
            .call_function("make", vec![], context(), true)
            .await
            .unwrap(),
    );
    assert_eq!(
        engine
            .call_function(
                "use_tag",
                vec![record, value(effect.clone())],
                context(),
                true
            )
            .await
            .unwrap(),
        V::String("tagged".into())
    );
    let annotated = copied_record(V::String("Ada".into()), vec![RuntimeTy::string()], V::Null);
    assert_eq!(
        engine
            .call_function("optional_tag", vec![annotated.clone()], context(), true)
            .await
            .unwrap(),
        V::String("tagged".into())
    );
    assert_eq!(
        engine
            .call_function("optional_tag", vec![V::Null], context(), true)
            .await
            .unwrap(),
        V::String("none".into())
    );
    let retained = view(
        engine
            .call_function("retain_tag", vec![annotated], context(), true)
            .await
            .unwrap(),
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_function(
                "optional_tag",
                vec![value(retained.clone())],
                context(),
                true
            )
            .await
            .unwrap(),
        V::String("tagged".into())
    );
    drop(retained);
    drop(effect);
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(engine.heap_stats().active_handles, baseline);
}

#[tokio::test]
async fn copied_interface_inputs_reject_pins_and_malformed_data_before_execution() {
    let engine = engine(&format!("{COUNTER}\n{COPIED_INPUT}"));
    let effects = view(
        engine
            .call_function("make", vec![], context(), true)
            .await
            .unwrap(),
    );
    let valid = copied_record(V::String("Ada".into()), vec![RuntimeTy::string()], V::Null);
    assert!(
        engine
            .call_function(
                "wrong_error",
                vec![valid, value(effects.clone())],
                context(),
                true
            )
            .await
            .is_err()
    );
    let invalid = vec![
        copied_record(V::Int(1), vec![RuntimeTy::int()], V::Null),
        copied_record(V::Int(1), vec![RuntimeTy::string()], V::Null),
        copied_record(V::String("Ada".into()), vec![], V::Null),
        copied_record(
            V::String("Ada".into()),
            vec![RuntimeTy::string(), RuntimeTy::int()],
            V::Null,
        ),
        copied_record(
            V::String("Ada".into()),
            vec![RuntimeTy::string()],
            V::Int(7),
        ),
        V::Instance {
            class_name: "OtherRecord".into(),
            type_args: vec![],
            fields: indexmap::indexmap! {"value".into() => V::String("Ada".into())},
        },
        V::typed(
            V::Instance {
                class_name: "TaggedRecord".into(),
                type_args: vec![],
                fields: indexmap::indexmap! {},
            },
            copied_record_type(vec![RuntimeTy::string()]),
        ),
    ];
    for input in invalid {
        let debug = format!("{input:?}");
        assert!(
            engine
                .call_function(
                    "use_tag",
                    vec![input, value(effects.clone())],
                    context(),
                    true
                )
                .await
                .is_err(),
            "accepted {debug}"
        );
    }
    assert_eq!(
        engine
            .call_interface(effects, "current", vec![], vec![], context())
            .await
            .unwrap(),
        V::Int(0)
    );
}

#[tokio::test]
async fn copied_interface_input_retains_live_children_and_releases_failed_copies() {
    let engine = engine(&format!("{COUNTER}\n{COPIED_INPUT}"));
    let baseline = engine.heap_stats().active_handles;
    let child = view(
        engine
            .call_function("make", vec![], context(), true)
            .await
            .unwrap(),
    );
    let record = copied_record(
        V::String("Ada".into()),
        vec![RuntimeTy::string()],
        value(child.clone()),
    );
    let retained = view(
        engine
            .call_function("retain_tag", vec![record], context(), true)
            .await
            .unwrap(),
    );
    drop(child);
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(
        engine
            .call_function(
                "optional_tag",
                vec![value(retained.clone())],
                context(),
                true
            )
            .await
            .unwrap(),
        V::String("tagged".into())
    );
    assert_eq!(
        engine
            .call_function(
                "child_count",
                vec![value(retained.clone())],
                context(),
                true
            )
            .await
            .unwrap(),
        V::Int(0)
    );
    drop(retained);
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(engine.heap_stats().active_handles, baseline);
    let child = view(
        engine
            .call_function("make", vec![], context(), true)
            .await
            .unwrap(),
    );
    let invalid = copied_record(V::Int(7), vec![RuntimeTy::string()], value(child));
    // No outer annotation: materialize the valid child field before the
    // later value field fails, exercising cleanup of a partial copy.
    let V::Union { value: invalid, .. } = invalid else {
        unreachable!()
    };
    let invalid = *invalid;
    assert!(
        engine
            .call_function("retain_tag", vec![invalid], context(), true)
            .await
            .is_err()
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    assert_eq!(engine.heap_stats().active_handles, baseline);
}
