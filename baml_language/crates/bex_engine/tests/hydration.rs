//! The host boundary: a value is built as what it says it is, then checked
//! against its slot (`bex_engine::hydrate`). These tests pass host
//! `BexExternalValue`s a BAML call site cannot express, and pin both halves:
//! a value that says something the program cannot hold is refused as it is
//! built (naming where), and a well-built value that is not a member of its
//! slot is refused by the one membership question the type system asks.

mod common;

use std::sync::Arc;

use baml_type::{RuntimeFunctionParamTy, TypeName};
use bex_engine::{BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder, RuntimeTy};
use bex_external_types::{BexExternalAdt, MediaKind};
use bex_resource_types::{HostValueArc, HostValueKind};
use common::compile_for_engine;
use indexmap::IndexMap;
use sys_native::SysOpsExt;

async fn call(
    source: &str,
    function: &str,
    args: Vec<BexExternalValue>,
) -> Result<BexExternalValue, EngineError> {
    let engine = Arc::new(
        BexEngine::new(
            compile_for_engine(source),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .expect("engine construction"),
    );
    engine
        .call_function(
            function,
            args,
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
}

/// The refusal's message, or a panic with whatever else came back.
#[track_caller]
fn refusal(result: Result<BexExternalValue, EngineError>) -> String {
    match result {
        Err(EngineError::TypeMismatch { message }) => message,
        other => panic!("expected a type mismatch, got {other:?}"),
    }
}

fn s(value: &str) -> BexExternalValue {
    BexExternalValue::String(value.into())
}

fn class(name: &str, args: Vec<RuntimeTy>) -> RuntimeTy {
    RuntimeTy::Class(TypeName::from_dotted_path(name), args.into())
}

fn list(element: RuntimeTy, items: Vec<BexExternalValue>) -> BexExternalValue {
    BexExternalValue::Array {
        element_type: element,
        items,
    }
}

fn instance(
    name: &str,
    args: Vec<RuntimeTy>,
    fields: &[(&str, BexExternalValue)],
) -> BexExternalValue {
    BexExternalValue::Instance {
        class_name: name.to_string(),
        type_args: args,
        fields: fields
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect(),
    }
}

// ── The check: one membership question against the slot ─────────────────────

#[tokio::test]
async fn a_scalar_is_refused_where_another_scalar_is_declared() {
    let source = r#"
        function half(x: float) -> float { x / 2.0 }
    "#;
    let message = refusal(call(source, "half", vec![BexExternalValue::Int(7)]).await);
    assert_eq!(
        message,
        "argument `x` of `half`: Value of type 'int' does not match the declared type `float`"
    );
    assert_eq!(
        call(source, "half", vec![BexExternalValue::Float(7.0)])
            .await
            .unwrap(),
        BexExternalValue::Float(3.5)
    );
}

#[tokio::test]
async fn a_list_is_the_type_its_host_states_and_generics_are_invariant() {
    let source = r#"
        function count(xs: (int | string)[]) -> int { xs.length() }
    "#;
    // `[1, 2]` sent as an `int[]` is an `int[]`, which is not a `(int | string)[]`.
    let message = refusal(
        call(
            source,
            "count",
            vec![list(
                RuntimeTy::int(),
                vec![BexExternalValue::Int(1), BexExternalValue::Int(2)],
            )],
        )
        .await,
    );
    assert_eq!(
        message,
        "argument `xs` of `count`: Value of type 'int[]' does not match the declared type \
         `(int | string)[]`"
    );
    // The same items sent as the slot's own type are taken: the host coerces,
    // the engine checks.
    let either = RuntimeTy::union([RuntimeTy::int(), RuntimeTy::string()]);
    assert_eq!(
        call(
            source,
            "count",
            vec![list(
                either,
                vec![BexExternalValue::Int(1), BexExternalValue::Int(2)],
            )],
        )
        .await
        .unwrap(),
        BexExternalValue::Int(2)
    );
}

#[tokio::test]
async fn null_is_a_member_of_an_optional_and_not_of_its_base() {
    let source = r#"
        function maybe(x: int?) -> int { x ?? -1 }
        function plain(x: int) -> int { x }
    "#;
    assert_eq!(
        call(source, "maybe", vec![BexExternalValue::Null])
            .await
            .unwrap(),
        BexExternalValue::Int(-1)
    );
    let message = refusal(call(source, "plain", vec![BexExternalValue::Null]).await);
    assert!(
        message.contains("Value of type 'null' does not match the declared type `int`"),
        "{message}"
    );
}

#[tokio::test]
async fn a_literal_slot_holds_the_one_value() {
    let source = r#"
        function stage(s: "draft" | "final") -> string { s }
    "#;
    assert_eq!(
        call(source, "stage", vec![s("draft")]).await.unwrap(),
        s("draft")
    );
    let message = refusal(call(source, "stage", vec![s("other")]).await);
    assert!(
        message.contains("Value of type 'string' does not match the declared type"),
        "{message}"
    );
}

#[tokio::test]
async fn a_generic_instance_is_checked_at_the_arguments_it_states() {
    let source = r#"
        class Box<T> { v T }
        function open(b: Box<int>) -> int { b.v }
    "#;
    assert_eq!(
        call(
            source,
            "open",
            vec![instance(
                "Box",
                vec![RuntimeTy::int()],
                &[("v", BexExternalValue::Int(5))]
            )],
        )
        .await
        .unwrap(),
        BexExternalValue::Int(5)
    );
    let message = refusal(
        call(
            source,
            "open",
            vec![instance(
                "Box",
                vec![RuntimeTy::string()],
                &[("v", s("five"))],
            )],
        )
        .await,
    );
    assert!(
        message.contains("Value of type 'Box<string>' does not match the declared type `Box<int>`"),
        "{message}"
    );
}

#[tokio::test]
async fn an_interface_slot_takes_an_implementor_and_no_other_class() {
    let source = r#"
        interface Named { function name(self) -> string throws never }
        class Person { first string implements Named { function name(self) -> string throws never { self.first } } }
        class Rock { mass int }
        function greet(x: Named) -> string { x.name() }
    "#;
    assert_eq!(
        call(
            source,
            "greet",
            vec![instance("Person", vec![], &[("first", s("Ada"))])],
        )
        .await
        .unwrap(),
        s("Ada")
    );
    let message = refusal(
        call(
            source,
            "greet",
            vec![instance(
                "Rock",
                vec![],
                &[("mass", BexExternalValue::Int(3))],
            )],
        )
        .await,
    );
    assert!(
        message.contains("Value of type 'Rock' does not match the declared type `Named`"),
        "{message}"
    );
}

#[tokio::test]
async fn the_json_alias_takes_a_json_typed_tree() {
    let source = r#"
        function size(j: baml.json.json) -> int {
            match (j) {
                let xs: baml.json.json[] => xs.length(),
                _ => -1,
            }
        }
    "#;
    let json = RuntimeTy::TypeAlias(TypeName::from_dotted_path("baml.json.json"));
    // A host whose slot is `json` states `json[]` for a list: that is the type
    // a list BAML parsed from JSON has.
    assert_eq!(
        call(
            source,
            "size",
            vec![list(json, vec![BexExternalValue::Int(1), s("two")],)],
        )
        .await
        .unwrap(),
        BexExternalValue::Int(2)
    );
    // An `int[]` is a value of another type: `int[] <: json` fails because
    // `int[]` is not `json[]` (generics are invariant).
    let message = refusal(
        call(
            source,
            "size",
            vec![list(RuntimeTy::int(), vec![BexExternalValue::Int(1)])],
        )
        .await,
    );
    assert!(
        message.contains("Value of type 'int[]' does not match the declared type"),
        "{message}"
    );
}

#[tokio::test]
async fn an_opaque_host_value_is_rust_data() {
    let source = r#"
        function keep(x: unknown) -> unknown { x }
        function count(x: int) -> int { x }
    "#;
    let opaque = BexExternalValue::HostValue(HostValueArc::new(7_001, HostValueKind::Opaque));
    // `unknown` takes every value; the opaque value comes back as itself.
    assert_eq!(
        call(source, "keep", vec![opaque.clone()]).await.unwrap(),
        opaque
    );
    let message = refusal(call(source, "count", vec![opaque]).await);
    assert!(
        message.contains("Value of type '$rust_type' does not match the declared type `int`"),
        "{message}"
    );
}

#[tokio::test]
async fn media_lands_as_its_kind() {
    let source = r#"
        function width(x: image) -> int { 1 }
    "#;
    let picture = BexExternalValue::Adt(BexExternalAdt::Media(bex_vm_types::MediaValue::from_url(
        MediaKind::Image,
        "https://example.test/p.png",
        Some("image/png"),
    )));
    assert_eq!(
        call(source, "width", vec![picture]).await.unwrap(),
        BexExternalValue::Int(1)
    );
    let clip = BexExternalValue::Adt(BexExternalAdt::Media(bex_vm_types::MediaValue::from_url(
        MediaKind::Audio,
        "https://example.test/a.mp3",
        Some("audio/mpeg"),
    )));
    let message = refusal(call(source, "width", vec![clip]).await);
    assert!(
        message.contains("Value of type 'audio' does not match the declared type `image`"),
        "{message}"
    );
}

// ── Building: what the value says, the program must hold ─────────────────────

#[tokio::test]
async fn a_child_outside_its_stated_component_type_is_refused_where_it_is() {
    let source = r#"
        class Point { x int y int }
        class Holder { items Point[] }
        function first_x(h: Holder) -> int { h.items[0].x }
    "#;
    // The heap never holds a `Point` with a string `x`: the refusal names the
    // path to the child.
    let bad = instance(
        "Holder",
        vec![],
        &[(
            "items",
            list(
                class("Point", vec![]),
                vec![instance(
                    "Point",
                    vec![],
                    &[("x", s("oops")), ("y", BexExternalValue::Int(2))],
                )],
            ),
        )],
    );
    let message = refusal(call(source, "first_x", vec![bad]).await);
    assert_eq!(
        message,
        "argument `h` of `first_x`: at `.items[0].x`: Value of type 'string' does not match \
         the declared type `int`"
    );
    // A list stated as `int[]` never holds a string either.
    let source = r#"
        function sum(xs: int[]) -> int { xs[0] }
    "#;
    let message = refusal(
        call(
            source,
            "sum",
            vec![list(
                RuntimeTy::int(),
                vec![BexExternalValue::Int(1), s("x")],
            )],
        )
        .await,
    );
    assert_eq!(
        message,
        "argument `xs` of `sum`: at `[1]`: Value of type 'string' does not match the declared \
         type `int`"
    );
}

#[tokio::test]
async fn an_instance_names_every_declared_field_and_no_other() {
    let source = r#"
        class Point { x int y int }
        function origin(p: Point) -> int { p.x + p.y }
    "#;
    let extra = instance(
        "Point",
        vec![],
        &[
            ("x", BexExternalValue::Int(1)),
            ("y", BexExternalValue::Int(2)),
            ("z", BexExternalValue::Int(3)),
        ],
    );
    let message = refusal(call(source, "origin", vec![extra]).await);
    assert!(
        message.contains("class `user.Point` has no field `z`"),
        "{message}"
    );

    let missing = instance("Point", vec![], &[("x", BexExternalValue::Int(1))]);
    let message = refusal(call(source, "origin", vec![missing]).await);
    assert!(message.contains("Missing field `y`"), "{message}");

    let unknown = instance("Pointe", vec![], &[("x", BexExternalValue::Int(1))]);
    let message = refusal(call(source, "origin", vec![unknown]).await);
    assert!(message.contains("unknown class `Pointe`"), "{message}");
}

#[tokio::test]
async fn a_generic_instance_states_exactly_its_class_arity() {
    let source = r#"
        class Box<T> { v T }
        function open(b: Box<int>) -> int { b.v }
    "#;
    let bare = instance("Box", vec![], &[("v", BexExternalValue::Int(5))]);
    let message = refusal(call(source, "open", vec![bare]).await);
    assert!(
        message.contains("class `user.Box` takes 1 type argument(s), 0 were stated"),
        "{message}"
    );
    let too_many = instance(
        "Box",
        vec![RuntimeTy::int(), RuntimeTy::int()],
        &[("v", BexExternalValue::Int(5))],
    );
    let message = refusal(call(source, "open", vec![too_many]).await);
    assert!(
        message.contains("class `user.Box` takes 1 type argument(s), 2 were stated"),
        "{message}"
    );
}

#[tokio::test]
async fn an_enum_variant_is_one_the_enum_declares() {
    let source = r#"
        enum Mood { HAPPY SAD }
        function flip(m: Mood) -> Mood { match (m) { Mood.HAPPY => Mood.SAD, Mood.SAD => Mood.HAPPY } }
    "#;
    assert_eq!(
        call(
            source,
            "flip",
            vec![BexExternalValue::variant("Mood", "HAPPY")]
        )
        .await
        .unwrap(),
        BexExternalValue::variant("user.Mood", "SAD")
    );
    let message = refusal(
        call(
            source,
            "flip",
            vec![BexExternalValue::variant("Mood", "GRUMPY")],
        )
        .await,
    );
    assert!(message.contains("has no variant `GRUMPY`"), "{message}");
    let message = refusal(
        call(
            source,
            "flip",
            vec![BexExternalValue::variant("Weather", "SUNNY")],
        )
        .await,
    );
    assert!(message.contains("unknown enum `Weather`"), "{message}");
}

#[tokio::test]
async fn a_map_states_a_key_type_its_keys_inhabit() {
    let source = r#"
        function keys(m: map<string, int>) -> int { m.length() }
    "#;
    let entries = |keys: &[&str]| -> IndexMap<String, BexExternalValue> {
        keys.iter()
            .map(|key| ((*key).to_string(), BexExternalValue::Int(1)))
            .collect()
    };
    assert_eq!(
        call(
            source,
            "keys",
            vec![BexExternalValue::Map {
                key_type: RuntimeTy::string(),
                value_type: RuntimeTy::int(),
                entries: entries(&["a", "b"]),
            }],
        )
        .await
        .unwrap(),
        BexExternalValue::Int(2)
    );
    // Keys cross the boundary as strings, so a stated key type has to hold
    // them, and each key has to be a member of it.
    let message = refusal(
        call(
            source,
            "keys",
            vec![BexExternalValue::Map {
                key_type: RuntimeTy::int(),
                value_type: RuntimeTy::int(),
                entries: entries(&["1"]),
            }],
        )
        .await,
    );
    assert!(
        message.contains("map keys cross the boundary as strings"),
        "{message}"
    );
    let only_a = RuntimeTy::Literal(
        baml_type::Literal::String("a".to_string()),
        baml_type::Freshness::Regular,
    );
    let message = refusal(
        call(
            source,
            "keys",
            vec![BexExternalValue::Map {
                key_type: only_a,
                value_type: RuntimeTy::int(),
                entries: entries(&["b"]),
            }],
        )
        .await,
    );
    assert!(
        message.contains("map key \"b\" is not a member of the stated key type `\"a\"`"),
        "{message}"
    );
}

#[tokio::test]
async fn a_host_callable_arrives_with_its_function_type() {
    let source = r#"
        function apply(f: (int) -> int, x: int) -> int { f(x) }
    "#;
    // A callable by key alone says nothing about what it takes or returns.
    let bare = BexExternalValue::HostValue(HostValueArc::new(7_002, HostValueKind::Callable));
    let message = refusal(call(source, "apply", vec![bare, BexExternalValue::Int(1)]).await);
    assert!(
        message.contains("a host callable arrives with the function type it inhabits"),
        "{message}"
    );
    // One of another function type is a value of another type.
    let stringly = BexExternalValue::HostCallable {
        handle: HostValueArc::new(7_003, HostValueKind::Callable),
        params: Box::new([RuntimeFunctionParamTy::required(None, RuntimeTy::string())]),
        ret: Box::new(RuntimeTy::string()),
        throws: Box::new(RuntimeTy::unknown()),
    };
    let message = refusal(call(source, "apply", vec![stringly, BexExternalValue::Int(1)]).await);
    assert!(
        message.contains("Value of type '(string) -> string throws unknown' does not match"),
        "{message}"
    );
    // An optional parameter is addressed by name, so its type names it.
    let nameless = BexExternalValue::HostCallable {
        handle: HostValueArc::new(7_004, HostValueKind::Callable),
        params: Box::new([RuntimeFunctionParamTy::optional(None, RuntimeTy::int())]),
        ret: Box::new(RuntimeTy::int()),
        throws: Box::new(RuntimeTy::unknown()),
    };
    let message = refusal(call(source, "apply", vec![nameless, BexExternalValue::Int(1)]).await);
    assert!(
        message.contains("optional parameter without a name"),
        "{message}"
    );
}

#[tokio::test]
async fn a_union_envelope_is_not_a_host_value() {
    let source = r#"
        function echo(x: int | string) -> int | string { x }
    "#;
    let enveloped = BexExternalValue::union(
        BexExternalValue::Int(7),
        [RuntimeTy::int(), RuntimeTy::string()],
        RuntimeTy::int(),
    );
    let message = refusal(call(source, "echo", vec![enveloped]).await);
    assert!(
        message.contains("a union envelope is not a host value"),
        "{message}"
    );
    // The value itself, as it is, is a member of the union.
    let BexExternalValue::Union { value, .. } =
        call(source, "echo", vec![BexExternalValue::Int(7)])
            .await
            .unwrap()
    else {
        panic!("a union-typed return is enveloped")
    };
    assert_eq!(*value, BexExternalValue::Int(7));
}

// ── Membership is the value's, not the primitive's, where a literal is declared ─

#[tokio::test]
async fn a_literal_field_is_checked_per_value_not_per_primitive() {
    // Two instances of one class in one list: the first inhabits the literal
    // field, the second does not. The answer for the second must not be the
    // first's just because both hold strings.
    let source = r#"
        class Sq { kind "square" }
        function second(xs: Sq[]) -> string { xs[1].kind }
        type Kind = "a" | Kind[]
        function kinds(k: Kind) -> int { 0 }
    "#;
    let good = instance("user.Sq", vec![], &[("kind", s("square"))]);
    let bad = instance("user.Sq", vec![], &[("kind", s("circle"))]);
    let message = refusal(
        call(
            source,
            "second",
            vec![list(
                class("user.Sq", vec![]),
                vec![good.clone(), bad.clone()],
            )],
        )
        .await,
    );
    assert_eq!(
        message,
        "argument `xs` of `second`: at `[1].kind`: Value of type 'string' does not match the \
         declared type `\"square\"`"
    );
    // The same through a recursive alias that mentions a literal.
    let kind = RuntimeTy::TypeAlias(TypeName::from_dotted_path("user.Kind"));
    let message = refusal(
        call(
            source,
            "kinds",
            vec![list(kind.clone(), vec![s("a"), s("b")])],
        )
        .await,
    );
    assert_eq!(
        message,
        "argument `k` of `kinds`: at `[1]`: Value of type 'string' does not match the declared \
         type `Kind`"
    );
    assert_eq!(
        call(source, "kinds", vec![list(kind, vec![s("a"), s("a")])])
            .await
            .unwrap(),
        BexExternalValue::Int(0)
    );
}
