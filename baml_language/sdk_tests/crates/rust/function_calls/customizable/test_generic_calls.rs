//! Generic *function-call* coverage (ns_generic_tests).
//!
//! Pins the host->engine call path for generic functions/methods whose TypeVars
//! must be bound from the call. Cases use the *explicit subscript* form: the
//! caller binds each TypeVar positionally via `fn[T1, T2](...)` (the callee's
//! own generic params, in declaration order), and a parameterized
//! `GenericBox[int]` receiver supplies a generic class's TypeVars (recovered
//! host-side from the Pydantic generic metadata). `fn[...]` is sugar over the
//! engine's named binding wire; no test passes `_types=` directly.
//!
//! (The *inference* variant — bare calls with no caller-specified bindings,
//! which depend on the not-yet-landed inbound-inference phase — is
//! intentionally not covered here yet.)
//!
//! Rust surface: python's subscript form (`fn[T](...)`, `obj.method[U](...)`)
//! is the turbofish (`fn::<T>(...)`, `obj.method::<U>(...)`), and a
//! parameterized receiver is a turbofished struct literal
//! (`GenericBox::<i64> { .. }`).

use baml_bridge::Map;
// ADAPTATION(rust): the `T | string | null` union enum is named for its arms
// (`TOrString`), not its declaring field (python's `ContainerShapesMixed`).
use baml_sdk::generic_tests::{
    ContainerShapes, GenericBox, GenericPair, GenericRecursive, GenericTriple, NamedStatic,
    StringIntPair, TOrString, choose, consume_int_wrapper, extract, identity, list_head,
    make_int_box, make_int_container, make_int_str_bool_triple, make_nested_box, make_triple,
    one_type_arg, parse_as, read_items, second_of, tag_or_value, two_type_args, wrap,
};

// ===========================================================================
// basic cases, free functions
// ===========================================================================

// --- identity<T>(x: T) -> T : single TypeVar --------------------------------

#[test]
fn test_generic_calls_identity_explicit() {
    assert_eq!(identity::<i64>(5).unwrap(), 5);
    assert_eq!(identity::<String>("hi".to_string()).unwrap(), "hi");

    let pair = StringIntPair {
        my_string: "a".to_string(),
        my_int: 1,
    };
    assert_eq!(identity::<StringIntPair>(pair.clone()).unwrap(), pair);

    let boxed = GenericBox::<GenericBox<String>> {
        value: GenericBox::<String> {
            value: "hello".to_string(),
        },
    };
    assert_eq!(
        identity::<GenericBox<GenericBox<String>>>(boxed.clone()).unwrap(),
        boxed
    );

    let triple = GenericTriple::<GenericBox<String>, f64, bool> {
        first: GenericBox::<String> {
            value: "hello".to_string(),
        },
        second: vec![1.1, 2.2],
        third: Map::from([("lorem".to_string(), true), ("ipsum".to_string(), false)]),
    };
    assert_eq!(
        identity::<GenericTriple<GenericBox<String>, f64, bool>>(triple.clone()).unwrap(),
        triple
    );
}

#[tokio::test]
async fn test_generic_calls_identity_async_explicit() {
    use baml_sdk::generic_tests::identity_async;

    assert_eq!(identity_async::<i64>(7).await.unwrap(), 7);
}

// --- tag_or_value<T>(x: T | string | null) -> string : TypeVar in a union ---

#[test]
fn test_generic_calls_tag_or_value_explicit() {
    // `tag_or_value` reflects its bound `T` back as a string; `x` must inhabit
    // the substituted `T | string | null`. Proves `T` is bound from the
    // subscript.
    // ADAPTATION(rust): the `T` arm of the generated union has no `From` impl
    // (a blanket `From<T>` would overlap the concrete arms' under coherence),
    // so union values are constructed by naming the variant. The union is
    // untagged on the wire, so with `T = String` the `T` and `String`
    // variants encode identically.
    assert_eq!(tag_or_value::<i64>(TOrString::T(5)).unwrap(), "int");
    assert_eq!(
        tag_or_value::<String>(TOrString::T("plain".to_string())).unwrap(),
        "string"
    );
    let pair = StringIntPair {
        my_string: "b".to_string(),
        my_int: 2,
    };
    assert!(
        tag_or_value::<StringIntPair>(TOrString::T(pair))
            .unwrap()
            .contains("StringIntPair")
    );
}

// --- make_triple<A, B, C>(...) -> GenericTriple<A, B, C> : multiple TypeVars -

#[test]
fn test_generic_calls_make_triple_explicit() {
    // A=int, B=str, C=bool, bound positionally by the subscript.
    let t = make_triple::<i64, String, bool>(
        1,
        vec!["a".to_string(), "b".to_string()],
        Map::from([("k".to_string(), true)]),
    )
    .unwrap();
    // DIVERGENCE(rust): python's `isinstance(t, GenericTriple)` is guaranteed
    // here by the static return type.
    assert_eq!(t.first, 1);
    assert_eq!(t.second, ["a", "b"]);
    assert_eq!(t.third, Map::from([("k".to_string(), true)]));
}

// --- one_type_arg<T>() / two_type_args<A,B>() : return-position-only TypeVars -
// No argument carries `T`; the binding can only come from the subscript. The
// cleanest proof the inbound path does not rely on argument inference.

#[test]
fn test_generic_calls_one_type_arg_explicit() {
    assert_eq!(one_type_arg::<i64>().unwrap(), "int");
    assert_eq!(one_type_arg::<String>().unwrap(), "string");
    // Nested generic binding must encode fully (base class + concrete arg).
    let nested = one_type_arg::<GenericBox<i64>>().unwrap();
    assert!(nested.contains("GenericBox") && nested.contains("int"));
}

#[test]
fn test_generic_calls_two_type_args_explicit() {
    assert_eq!(two_type_args::<i64, String>().unwrap(), "int | string");
}

#[test]
fn test_generic_calls_generic_free_fn_requires_binding() {
    // Inbound-inference is now on, so a bare generic call no longer raises in
    // the SDK. But `one_type_arg<T>()` / `two_type_args<A,B>()` are
    // return/body-only: NO argument carries the TypeVar, so inference finds no
    // evidence and the *engine* (Gate A) rejects the call.
    // DIVERGENCE(rust): a bare `one_type_arg()` / `two_type_args()` cannot
    // infer the type parameters at COMPILE time ("type annotations needed",
    // rustc E0282), so the engine-side rejection is unreachable through the
    // typed wrappers. Binding via the turbofish remains the way to call these
    // — see test_one_type_arg_explicit / test_two_type_args_explicit.
    // Inference of value-carried TypeVars lives in test_generic_inference.rs.
}

#[test]
fn test_generic_calls_subscript_wrong_arity_raises() {
    // DIVERGENCE(rust): `two_type_args::<i64>()` — one type argument where two
    // are declared — is a compile error (wrong number of generic arguments,
    // rustc E0107), not a runtime TypeError.
}

// ===========================================================================
// basic cases, generic classes
// ===========================================================================

// --- consume_int_wrapper(x: GenericBox<int>) -> int : fully-bound TypeVar ----

#[test]
fn test_generic_calls_consume_int_wrapper_baseline() {
    // No binding of any kind: a concretely-instantiated `GenericBox<int>` flows
    // in and the `int` field flows back out. Anchors the suite — if this
    // breaks, the generic *class* boundary regressed independent of TypeVar
    // binding.
    assert_eq!(
        consume_int_wrapper(GenericBox::<i64> { value: 9 }).unwrap(),
        9
    );
}

// --- GenericBox<T>.get(self) -> string : class TypeVar from the receiver -----
// Binding-sensitive: `get` is `type.of<T>()`. `get` has no own TypeVars,
// so there's nothing to subscript — `T` rides on the parameterized receiver.

#[test]
fn test_generic_calls_genericbox_get_explicit() {
    // `GenericBox[int](...)` carries the type arg; the host recovers it from
    // the receiver and seeds it as the method frame's class-level `T`.
    let b = GenericBox::<i64> { value: 5 };
    assert_eq!(b.get().unwrap(), "int");
}

// --- GenericBox<T>.pair_with<U>(self, other: U) -> string : class T + method U

#[test]
fn test_generic_calls_genericbox_pair_with_explicit() {
    // `T` from the `GenericBox[int]` receiver, `U` from the method subscript.
    let b = GenericBox::<i64> { value: 5 };
    assert_eq!(
        b.pair_with::<String>("hello world".to_string()).unwrap(),
        "int | string"
    );
}

// --- GenericBox<T>.new<V>(value: V) -> GenericBox<V> : generic STATIC method --
// No receiver, so only the static's own `V` is bound (via the subscript); no
// class type args ride along.

#[test]
fn test_generic_calls_genericbox_new_static_explicit() {
    // ADAPTATION(rust): the static's own `V` is a method-level generic,
    // inferred here from the argument. The class turbofish names the impl —
    // BAML follows Rust in that the class params belong on the struct — but
    // a static consumes no class params and none ride the wire.
    let boxed = GenericBox::<i64>::new(5).unwrap();
    // DIVERGENCE(rust): python's `isinstance(box, GenericBox)` is guaranteed
    // by the static return type.
    assert_eq!(boxed.value, 5);
}

#[test]
fn test_generic_calls_generic_static_infers_binding() {
    // A generic static method's own `V` appears in a parameter (`value: V`),
    // so `V` needs no subscript — rustc infers it from the value.
    // ADAPTATION(rust): the impl's class param must still be named to reach
    // the associated function (arbitrary here — the static consumes no class
    // params and sends none on the wire).
    let boxed = GenericBox::<()>::new(5).unwrap();
    assert_eq!(boxed.value, 5);
}

// --- NamedStatic<A,B,C>.make<D,E> : static TypeVar names DIFFER from the class -
// Proves the named `TyArg` wire slots each binding by TypeVar *name* into the
// static frame's own params (`[D, E]`) — no phantom class params. This is the
// case the named wire exists for (01pt5).

#[test]
fn test_generic_calls_named_static_distinct_typevar_names() {
    // The static's own TypeVars (`D`, `E`) are turbofished on the method.
    // ADAPTATION(rust): the enclosing class params (`A`, `B`, `C`) must be
    // named to reach the associated function (the turbofish belongs on the
    // struct), but they play no part in the call — the wire carries only
    // `[D, E]`, which is exactly the "no phantom class params" contract this
    // test pins.
    assert_eq!(
        NamedStatic::<(), (), ()>::make::<i64, String>(1, "x".to_string()).unwrap(),
        "int | string"
    );
}

// --- Negative: an instance method needing class args on an UN-parameterized
// receiver must raise (the class TypeVars can't be recovered).

#[test]
fn test_generic_calls_instance_method_unparameterized_receiver_raises() {
    // DIVERGENCE(rust): python's `GenericBox(value=5)` (no `[int]`) constructs
    // an un-parameterized receiver whose class type args can't be recovered
    // host-side. A Rust struct literal is always fully typed — the class `T`
    // is pinned at construction by inference or annotation — so an
    // unparameterized receiver is unrepresentable and this runtime rejection
    // cannot occur.
}

// --- extract<A, B, C, D>(a: GenericPair<GenericPair<A,B>, GenericPair<C,D>>) --
// Nested generic. Binding-sensitive: body is `type.of<A|B|C|D>()`.

#[test]
fn test_generic_calls_extract_explicit() {
    let pair = GenericPair::<GenericPair<i64, String>, GenericPair<bool, f64>> {
        first: GenericPair::<i64, String> {
            first: 1,
            second: "a".to_string(),
        },
        second: GenericPair::<bool, f64> {
            first: true,
            second: 1.5,
        },
    };
    assert_eq!(
        extract::<i64, String, bool, f64>(pair).unwrap(),
        "int | string | bool | float"
    );
}

// ===========================================================================
// basic case, `T` in return position only
// ===========================================================================

// --- parse_as<T>(source: string) -> T : return-position-only TypeVar --------

#[test]
fn test_generic_calls_parse_as_explicit() {
    // `T` bound by the host via the subscript (Python surface for `$types`).
    let pair = parse_as::<StringIntPair>(r#"{"my_string": "x", "my_int": 3}"#.to_string()).unwrap();
    assert_eq!(
        pair,
        StringIntPair {
            my_string: "x".to_string(),
            my_int: 3,
        }
    );
    assert_eq!(parse_as::<i64>("42".to_string()).unwrap(), 42);
}

// ===========================================================================
// complex cases
// ===========================================================================

// --- second_of<T>(p: GenericPair<int, T>) -> T : partially-bound class param -

#[test]
fn test_generic_calls_second_of_explicit() {
    assert_eq!(
        second_of::<String>(GenericPair::<i64, String> {
            first: 1,
            second: "hi".to_string(),
        })
        .unwrap(),
        "hi"
    );
    let pair = StringIntPair {
        my_string: "z".to_string(),
        my_int: 9,
    };
    let p = GenericPair::<i64, StringIntPair> {
        first: 0,
        second: pair.clone(),
    };
    assert_eq!(second_of::<StringIntPair>(p).unwrap(), pair);
}

// --- list_head<T>(list: GenericRecursive<T>) -> T : recursive generic arg ----

#[test]
fn test_generic_calls_list_head_explicit() {
    // Provisional: the recursive `next` field is assumed to codegen boxed
    // (`Option<Box<GenericRecursive<T>>>`) — a recursive Rust struct needs the
    // indirection.
    let linked_list = GenericRecursive::<i64> {
        value: 7,
        next: Some(Box::new(GenericRecursive::<i64> {
            value: 8,
            next: None,
        })),
    };
    assert_eq!(list_head::<i64>(linked_list).unwrap(), 7);
}

// --- choose<T>(left: T, right: T) -> T : unification across two args ---------

#[test]
fn test_generic_calls_choose_explicit() {
    assert_eq!(choose::<i64>(1, 2).unwrap(), 1);
    assert_eq!(
        choose::<String>("a".to_string(), "b".to_string()).unwrap(),
        "a"
    );
}

// --- read_items<T>(shape: ContainerShapes<T>) -> T[] : one T, many fields ----

#[test]
fn test_generic_calls_read_items_explicit() {
    let container = ContainerShapes::<i64> {
        item: 1,
        items: vec![1, 2, 3],
        by_key: Map::from([("k".to_string(), 4)]),
        maybe: None,
        // The `T | string | null` union field codegens as an `Option` of the
        // generated union enum (`TOrString<T>`), so the null arm is a bare
        // `None`.
        mixed: None,
    };
    assert_eq!(read_items::<i64>(container).unwrap(), vec![1, 2, 3]);
}

// ===========================================================================
// outbound generics
// ===========================================================================

// --- wrap<T>(x: T) -> GenericBox<T> : bind `T`, return a generic over it ------

#[test]
fn test_generic_calls_wrap_explicit() {
    let w = wrap::<i64>(5).unwrap();
    // DIVERGENCE(rust): python's `isinstance(w, GenericBox)` is guaranteed by
    // the static return type.
    assert_eq!(w.value, 5);
}

// ===========================================================================
// reified generics returned by NON-generic functions
// ===========================================================================
// The outbound mirror of `consume_int_wrapper`: no TypeVar binding and no
// inference — the callee's return type pins the class type args at the
// definition site, so the host only has to decode a fully-concrete generic
// instance flowing back out. One test per reification shape.

/// The concrete type args bound on a returned Pydantic generic instance.
///
/// A reified generic comes back as a *parametrized* subclass (`GenericBox[int]`),
/// whose `__pydantic_generic_metadata__["args"]` holds the bound args. A bare,
/// unbound instance instead has empty `args` and a leftover `~T` in `parameters`
/// — so a non-empty tuple here proves the host actually bound the TypeVars.
// DIVERGENCE(rust): there is no runtime generic metadata to probe — a returned
// generic is statically parametrized and an unbound instance is
// unrepresentable. `std::any::type_name` of the static type stands in so the
// per-shape assertions below can still name the expected bindings.
fn _type_args<T>(_obj: &T) -> &'static str {
    std::any::type_name::<T>()
}

// --- make_int_box() -> GenericBox<int> : one TypeVar, used once -------------

#[test]
fn test_generic_calls_make_int_box_reified() {
    let boxed = make_int_box().unwrap();
    let name = _type_args(&boxed);
    assert!(name.contains("GenericBox") && name.contains("i64"));
    assert_eq!(boxed.value, 7);
}

// --- make_int_container() -> ContainerShapes<int> : one TypeVar, many fields -
// The single `int` binding is reified into every field shape (bare, list, map,
// optional, union) — the host must decode all of them off one instance.

#[test]
fn test_generic_calls_make_int_container_reified() {
    let c = make_int_container().unwrap();
    let name = _type_args(&c);
    assert!(name.contains("ContainerShapes") && name.contains("i64"));
    assert_eq!(c.item, 1);
    assert_eq!(c.items, vec![1, 2, 3]);
    assert_eq!(c.by_key, Map::from([("k".to_string(), 4)]));
    assert_eq!(c.maybe, None);
    // The reified `T | string | null` union field decodes as an `Option` of
    // the generated union enum, one variant per non-null arm, named after
    // the arm (`TOrString<i64>` here — decode trial order is declaration
    // order, so the value lands in the `T` variant).
    assert_eq!(c.mixed, Some(TOrString::T(5)));
}

// --- make_nested_box() -> GenericBox<GenericBox<int>> : nested generic arg ---
// The type arg is itself a generic instance; the host must decode the inner
// GenericBox out of the outer one's field.

#[test]
fn test_generic_calls_make_nested_box_reified() {
    let outer = make_nested_box().unwrap();
    // The outer box's single type arg is itself the parametrized `GenericBox[int]`.
    let outer_name = _type_args(&outer);
    assert!(outer_name.contains("GenericBox") && outer_name.contains("i64"));
    let inner_name = _type_args(&outer.value);
    assert!(inner_name.contains("GenericBox") && inner_name.contains("i64"));
    assert_eq!(outer.value.value, 9);
}

// --- make_int_str_bool_triple() -> GenericTriple<int, string, bool> ---------
// Multiple TypeVars reified across mixed field shapes: scalar int, string list,
// bool-valued map.

#[test]
fn test_generic_calls_make_int_str_bool_triple_reified() {
    let t = make_int_str_bool_triple().unwrap();
    let name = _type_args(&t);
    assert!(
        name.contains("GenericTriple")
            && name.contains("i64")
            && name.contains("String")
            && name.contains("bool")
    );
    assert_eq!(t.first, 1);
    assert_eq!(t.second, ["a", "b"]);
    assert_eq!(t.third, Map::from([("k".to_string(), true)]));
}
