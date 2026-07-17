//! Generic *function-call* coverage — the INFERENCE variant (ns_generic_tests).
//!
//! Sibling of `test_generic_calls.rs` (the explicit-subscript suite). Here every
//! call is **bare**: no `fn[T](...)` subscript and no `_types=`. The engine solves
//! each TypeVar from the argument *values* (inbound-inference, 01a/01b), so these
//! calls produce the same result the explicit form does — minus the binding the
//! caller no longer has to write.
//!
//! Case labels map to `thoughts/.../inbound-inference/00b3-labeled-cases.md`.
//! A TypeVar buried in a union beside a concrete member (00b3 G5/§H) is now IN
//! SCOPE (02a reverses G5): inference subtracts the concrete siblings and routes
//! the residual to the TypeVar. Genuinely uninferable cases (return/body-only
//! vars, §E; a value fully absorbed by a concrete sibling) still require `_types=`
//! and are pinned here as negative cases — inference leaves them for Gate A.
//!
//! Rust surface: a "bare" python call is ordinary Rust type inference, so most
//! positive cases read as plain calls; python's runtime rejections and its
//! dynamically-typed evidence games (heterogeneous lists, divergent unions,
//! unbound instances) are compile-time impossibilities here and keep their
//! names with `DIVERGENCE(rust)` notes and empty bodies.

use baml_bridge::Map;
use baml_sdk::generic_tests::{
    ContainerShapes, GenericBox, GenericPair, GenericRecursive, NamedStatic, SomeEnum,
    StringIntPair, apply, choose, elem_type, extract, first_or, glue, identity, list_head,
    make_triple, maybe_id, one_type_arg, pair, parse_as, read_items, second_of, tag_or_value,
    values_of, wrap,
};

/// A failed generic call must surface as a typed engine rejection (the engine's
/// `EngineError::TypeMismatch` ⇒ `baml.errors.TypeMismatch`, not an opaque
/// `BamlPanic`), and its message must mention each `needle`. Pins both the
/// *kind* and the *message* of every inference rejection.
// DIVERGENCE(rust): every rejection this helper pinned in python is a
// COMPILE-time error under Rust's typed wrappers (see the per-test
// DIVERGENCE notes below), so no runtime caller remains; the helper is kept
// for suite parity with the python source.
fn _assert_type_error<T: std::fmt::Debug, E: std::fmt::Display>(
    result: Result<T, E>,
    needles: &[&str],
) {
    let err = match result {
        Ok(value) => panic!("expected the generic call to be rejected, got {value:?}"),
        Err(err) => err,
    };
    let message = err.to_string();
    for needle in needles {
        assert!(
            message.contains(needle),
            "missing {needle:?} in rejection message: {message:?}"
        );
    }
}

// ===========================================================================
// §A — single TypeVar inferred from one argument value
// ===========================================================================

#[test]
fn test_identity_infers_primitives() {
    // T1/T2: T bound from the value; identity returns it unchanged.
    assert_eq!(identity(5).unwrap(), 5);
    assert_eq!(identity("hi".to_string()).unwrap(), "hi");
    assert!(identity(true).unwrap());
}

#[test]
fn test_identity_infers_user_class() {
    // T3: T = StringIntPair, recovered from the instance value.
    let pair = StringIntPair {
        my_string: "a".to_string(),
        my_int: 1,
    };
    assert_eq!(identity(pair.clone()).unwrap(), pair);
}

#[test]
fn test_identity_infers_generic_instance() {
    // T4: a fully-bound GenericBox[int] carries its [int] on the wire, so T is
    // recovered as GenericBox<int> with no caller binding.
    let boxed = GenericBox::<i64> { value: 5 };
    assert_eq!(identity(boxed.clone()).unwrap(), boxed);

    let nested = GenericBox::<GenericBox<String>> {
        value: GenericBox::<String> {
            value: "hello".to_string(),
        },
    };
    assert_eq!(identity(nested.clone()).unwrap(), nested);
}

#[tokio::test]
async fn test_identity_async_infers() {
    // T5: the async path infers identically.
    use baml_sdk::generic_tests::identity_async;

    assert_eq!(identity_async(7).await.unwrap(), 7);
}

#[test]
fn test_identity_null_round_trips() {
    // §I I4 (decided): a `null` actual is no inference evidence (NOT bound as
    // `T=null`) ⇒ `T` defaults to host-only `rust_type`, and the value
    // round-trips unchanged.
    // DIVERGENCE(rust): a bare `None` carries no type for rustc either — the
    // turbofish supplies the `Option` the engine never sees evidence for.
    assert_eq!(identity::<Option<i64>>(None).unwrap(), None);
}

#[test]
fn test_identity_unbound_generic_instance_round_trips() {
    // §G G2 (decided): an UNBOUND generic instance — constructed without the
    // `[int]` subscript — carries no wire type-args, so it is host-only
    // (`T=rust_type`) and rides through the VM opaquely, round-tripping
    // unchanged (and staying distinct from a properly-bound `GenericBox[int]`,
    // G4).
    // DIVERGENCE(rust): an unbound generic instance is unrepresentable — a
    // Rust struct literal always has its type parameter pinned by inference
    // or annotation, so the opaque host-only ride cannot be reached from the
    // typed wrappers.
}

// ===========================================================================
// §B — structural / container solving across one or more arguments
// ===========================================================================

#[test]
fn test_make_triple_infers_multiple_typevars() {
    // T6: A=int (scalar), B=string (list element), C=bool (map value) — all
    // three inferred from differently-shaped arguments at once.
    let t = make_triple(
        1,
        vec!["a".to_string(), "b".to_string()],
        Map::from([("k".to_string(), true)]),
    )
    .unwrap();
    assert_eq!(t.first, 1);
    assert_eq!(t.second, ["a", "b"]);
    assert_eq!(t.third, Map::from([("k".to_string(), true)]));
}

#[test]
fn test_second_of_infers_from_nested_generic() {
    // T9: second_of<T>(p: GenericPair<int, T>) — T binds from the instance's
    // 2nd wire arg only (`first` is pinned to int in the signature).
    assert_eq!(
        second_of(GenericPair::<i64, String> {
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
    assert_eq!(second_of(p).unwrap(), pair);
}

#[test]
fn test_read_items_infers_from_instance_wire_args() {
    // T10: ContainerShapes<T> — T recovered from the instance's single wire
    // arg, NOT by re-unifying every field. Empty fields don't erase it (T42).
    let container = ContainerShapes::<i64> {
        item: 1,
        items: vec![1, 2, 3],
        by_key: Map::from([("k".to_string(), 4)]),
        maybe: None,
        mixed: None,
    };
    assert_eq!(read_items(container).unwrap(), vec![1, 2, 3]);

    let empty_fields = ContainerShapes::<i64> {
        item: 1,
        items: vec![],
        by_key: Map::new(),
        maybe: None,
        mixed: None,
    };
    assert_eq!(read_items(empty_fields).unwrap(), Vec::<i64>::new());
}

#[test]
fn test_list_head_infers_from_recursive_generic() {
    // T11: GenericRecursive<T> bottoms out at next=None; T binds from the wire
    // arg.
    let linked = GenericRecursive::<i64> {
        value: 7,
        next: Some(Box::new(GenericRecursive::<i64> {
            value: 8,
            next: None,
        })),
    };
    assert_eq!(list_head(linked).unwrap(), 7);
}

#[test]
fn test_extract_infers_four_typevars_from_nesting() {
    // T12: A,B,C,D recovered by walking the nested GenericPair instantiation.
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
    assert_eq!(extract(pair).unwrap(), "int | string | bool | float");
}

// ===========================================================================
// §C — union unification: one TypeVar across two argument positions
// ===========================================================================

#[test]
fn test_choose_infers_unified_typevar() {
    // T14: choose(5, 6) ⇒ T = int (the two bindings merge to one). Body
    // returns `left`, so the call returns 5.
    assert_eq!(choose(5, 6).unwrap(), 5);
    assert_eq!(choose("a".to_string(), "b".to_string()).unwrap(), "a");
}

#[test]
fn test_choose_infers_divergent_union() {
    // T15: choose(5, "asdf") ⇒ T = int | string (a capability inference
    // unlocks over the explicit form, which forces a single T). Returns
    // `left` = 5.
    // DIVERGENCE(rust): `choose(5, "asdf")` cannot compile — both parameters
    // are the same `T`, so divergent actuals are a type error and the
    // engine's union-merge is unreachable through the typed wrappers.
}

// ===========================================================================
// §D — partial binding: explicit seed for one TypeVar, infer the rest
// ===========================================================================

#[test]
fn test_make_triple_partial_explicit_then_infer() {
    // C2/T17: bind A explicitly via a partial `_types=` dict; B and C are
    // inferred.
    //
    // NOTE: this is an *unusual* situation — only SOME type vars are
    // explicitly bound while the rest are inferred. Users should generally
    // NOT reach for `_types=` at all: inbound inference binds every
    // value-carried TypeVar from the arguments (see the rest of this file),
    // and the explicit *subscript* form (`make_triple[int, str, bool](...)`,
    // test_make_triple_subscript_*) is the supported surface for the rare
    // case where a binding must be forced. `_types=` is an internal wiring
    // detail kept mainly for this partial-bind escape hatch; prefer plain
    // inference.
    //
    // Rust surface for the partial bind: turbofish with `_` holes — the
    // seeded var is written out, the rest are left to inference.
    let t = make_triple::<i64, _, _>(
        1,
        vec!["x".to_string(), "y".to_string()],
        Map::from([("k".to_string(), true)]),
    )
    .unwrap();
    assert_eq!(t.first, 1);
    assert_eq!(t.second, ["x", "y"]);
    assert_eq!(t.third, Map::from([("k".to_string(), true)]));
}

// ===========================================================================
// §G/outbound — infer T, return a generic over it
// ===========================================================================

#[test]
fn test_wrap_infers_and_returns_generic() {
    // T29: wrap(5) infers T=int and returns a GenericBox<int>.
    let w = wrap(5).unwrap();
    assert_eq!(w.value, 5);
}

// ===========================================================================
// §K — methods: class T from the receiver, method TypeVars inferred from args
// ===========================================================================

#[test]
fn test_genericbox_pair_with_infers_method_typevar() {
    // T37: class T=int from the GenericBox[int] receiver; method U=string
    // inferred from the bare `other` arg (no [str] subscript).
    let b = GenericBox::<i64> { value: 5 };
    assert_eq!(
        b.pair_with("hello world".to_string()).unwrap(),
        "int | string"
    );
}

#[test]
fn test_generic_static_infers_own_typevar() {
    // T38: GenericBox.new<V>(value: V) — V inferred from the value, no
    // subscript.
    let boxed = GenericBox::new(5).unwrap();
    assert_eq!(boxed.value, 5);
}

#[test]
fn test_named_static_infers_distinct_typevars() {
    // T39: NamedStatic.make<D,E>(d, e) — D=int, E=string inferred from the
    // args.
    // Provisional: the enclosing class params (`A`, `B`, `C`) play no part in
    // the call and are assumed not to need binding on this path.
    assert_eq!(
        NamedStatic::make(1, "x".to_string()).unwrap(),
        "int | string"
    );
}

// ===========================================================================
// Out-of-scope / must-specify: inference finds no evidence ⇒ engine rejects
// ===========================================================================

#[test]
fn test_union_with_concrete_sibling_infers_typevar() {
    // 02a reverses 00b3 G5/§H: a TypeVar buried in a union beside concrete
    // members (`x: T | string | null`) is NOW solved by inference. The `int`
    // actual is not absorbed by the `string`/`null` siblings, so it routes to
    // `T` ⇒ T=int, matching the explicit form `tag_or_value[int](5) == "int"`.
    // Provisional: the union-typed parameter is assumed to accept any arm's
    // value directly (an `Into`-style conversion on the generated union) and
    // to let the arm's value type drive inference of `T`.
    assert_eq!(tag_or_value(5).unwrap(), "int");
}

#[test]
fn test_union_concrete_sibling_absorbs_value_binds_rust_type() {
    // §H H3 (decided): a `string` actual IS absorbed by the concrete `string`
    // sibling, so nothing routes to `T`. `T` still has a value position (the
    // `x` param) and no closure occurrence, so it defaults to host-only
    // `rust_type` (rule 4) rather than being rejected.
    // DIVERGENCE(rust): rustc cannot leave `T` unbound — a string actual that
    // feeds only the `string` arm gives the type parameter no inference
    // source and the bare call is a compile error ("type annotations
    // needed"), so the host-only `rust_type` default is unreachable from the
    // typed wrappers.
}

#[test]
fn test_union_null_actual_binds_rust_type() {
    // §H H3 / §I I4 (decided): a `null` actual is no inference evidence (not
    // bound as `T=null`), and the `null` sibling absorbs it, so `T` defaults
    // to `rust_type`.
    // DIVERGENCE(rust): same as the string-absorption case above — a bare
    // null actual leaves `T` with no compile-time inference source, so the
    // call cannot be written without a turbofish and the `rust_type` default
    // is unreachable.
}

#[test]
fn test_return_only_var_still_requires_binding() {
    // §E: parse_as<T>(source: string) -> T — T appears only in return
    // position, so no argument can carry it. Inference finds nothing ⇒ the
    // engine rejects the call as a TYPE error (Python `TypeError`), and the
    // message complains that the type parameter couldn't be inferred and
    // names the function.
    // DIVERGENCE(rust): a bare `parse_as("42".to_string())` is a compile
    // error ("type annotations needed") — the engine-side Gate A rejection is
    // unreachable through the typed wrappers.
}

#[test]
fn test_body_only_var_still_requires_binding() {
    // §E: one_type_arg<T>() reflects T but takes no argument ⇒ uninferable ⇒
    // a Python `TypeError` whose message complains about the un-inferrable
    // type parameter and names the function.
    // DIVERGENCE(rust): a bare `one_type_arg()` is the same compile error —
    // see test_generic_free_fn_requires_binding in test_generic_calls.rs.
}

// ===========================================================================
// §J — variance soundness (02d/02e): conflicting occurrences of one TypeVar
// across invariant/covariant positions have no consistent binding ⇒ REJECT,
// instead of fabricating an unsound union. Agreeing occurrences still bind.
// ===========================================================================

#[test]
fn test_pair_invariant_list_conflict_rejects() {
    // J4/E1: pair(int[], string[]) ⇒ a⇒T==int, b⇒T==string (both invariant
    // list elements) ⇒ no consistent T ⇒ reject (the old unifier fabricated
    // `(int|string)[]`). Surfaces as a Python `TypeError` whose message names
    // the function, the clashing concrete types, and that they can't be
    // reconciled.
    // DIVERGENCE(rust): `pair(vec![1, 2], vec!["a", "b"])` cannot compile —
    // both parameters share `T`, so the conflicting element types are a
    // compile error and the engine's rejection is unreachable.
}

#[test]
fn test_pair_invariant_list_agree_binds() {
    // J9/G1: pair(int[], int[]) ⇒ two invariant occurrences that AGREE ⇒
    // T = int. The fix narrows behavior, so this must still succeed.
    assert_eq!(pair(vec![1, 2], vec![3, 4]).unwrap(), "int");
}

#[test]
fn test_choose_union_outside_container_is_sound() {
    // J10/G2: choose(int[], string[]) — both occurrences are covariant (bare
    // `T`), so the union forms OUTSIDE the container (T = int[] | string[])
    // and the call SUCCEEDS, returning `left`. Proves the fix keys on
    // position variance, not "arrays are involved." (Contrast pair, where T
    // is under the container.)
    // DIVERGENCE(rust): divergent actuals for one `T` cannot compile, so the
    // sound outside-the-container union is unreachable through the typed
    // wrappers — the variance distinction lives entirely engine-side.
}

#[test]
fn test_merge_invariant_map_value_conflict_rejects() {
    // J5/E2: merge(map<string,int>, map<string,string>) ⇒ conflicting
    // invariant map-value type ⇒ reject as a Python `TypeError`.
    // DIVERGENCE(rust): conflicting map-value types for one `T` are a compile
    // error; the engine rejection is unreachable.
}

#[test]
fn test_combine_invariant_class_arg_conflict_rejects() {
    // J6/E3: combine(GenericBox[int], GenericBox[string]) ⇒ Box<T> invariant,
    // int ≠ string ⇒ reject as a Python `TypeError`.
    // DIVERGENCE(rust): `GenericBox<i64>` vs `GenericBox<String>` for one `T`
    // is a compile error; the engine rejection is unreachable.
}

#[test]
fn test_glue_invariant_vs_covariant_conflict_rejects() {
    // J7/E4: glue(int, string[]) ⇒ arr⇒T==string (invariant) but bare⇒int <: T
    // (covariant); int <: string is false ⇒ reject as a Python `TypeError`.
    // DIVERGENCE(rust): `glue(1, vec!["a"])` cannot compile — `T` cannot be
    // both `i64` and `String` — so the engine rejection is unreachable.
}

#[test]
fn test_glue_invariant_and_covariant_agree_binds() {
    // J11/G4: glue(int, int[]) ⇒ invariant (T==int) + covariant (int <: int)
    // AGREE ⇒ T = int; must still succeed.
    assert_eq!(glue(1, vec![2, 3]).unwrap(), "int");
}

#[test]
fn test_two_typevar_union_is_uninferrable_rejects() {
    // J12: two_in_union<T,U>(x: T | U | int) ⇒ two free vars in one union have
    // no principled split without an explicit hint ⇒ reject as a Python
    // `TypeError` (distinct from §H, which is ONE var beside concrete
    // members).
    // DIVERGENCE(rust): a bare `two_in_union("hello".to_string())` leaves `T`
    // and `U` with no compile-time inference source ("type annotations
    // needed"), so the engine rejection is unreachable.
}

// ===========================================================================
// §D — n-ary covariant join, and §B heterogeneous container element
// ===========================================================================

#[test]
fn test_triple_choose_three_covariant_join() {
    // D3: triple_choose(5, "asdf", True) ⇒ T = int | string | bool — three
    // covariant bare-arg occurrences union-merge (n-ary, not pairwise).
    // DIVERGENCE(rust): three divergent actuals for one `T` cannot compile,
    // so the n-ary covariant join is unreachable through the typed wrappers.
}

#[test]
fn test_make_triple_heterogeneous_list_element_unions() {
    // B8: make_triple(1, [1, "x"], {"k": True}) ⇒ B = int | string — the
    // list's mixed elements union-merge while synthesizing ONE container's
    // element type (the §D join applied INSIDE a container; distinct from
    // §J's invariant conflict between two separate args). The heterogeneous
    // list round-trips.
    // DIVERGENCE(rust): a heterogeneous `Vec` is unrepresentable — `[1, "x"]`
    // has no Rust element type — so the in-container union-merge is
    // unreachable through the typed wrappers.
}

#[test]
fn test_choose_divergent_generic_instances_union() {
    // D2: choose(GenericBox[int], GenericBox[str]) ⇒ T = GenericBox<int> |
    // GenericBox<string>, the union OUTSIDE the box (both occurrences
    // covariant). Body returns `left`, so the int box comes back. Contrast
    // `combine`, where T is INSIDE the box and the same actuals conflict
    // (§J).
    // DIVERGENCE(rust): `GenericBox<i64>` vs `GenericBox<String>` for one `T`
    // cannot compile, so the outside-the-box union is unreachable.
}

#[test]
fn test_tag_or_value_binds_generic_instance() {
    // H2: tag_or_value(GenericBox[str]) ⇒ the instance is not absorbed by the
    // `string`/`null` siblings, so it routes to T ⇒ T = GenericBox<string>.
    // Provisional: same `Into`-style union-parameter assumption as
    // test_union_with_concrete_sibling_infers_typevar.
    let rendered = tag_or_value(GenericBox::<String> {
        value: "asdf".to_string(),
    })
    .unwrap();
    assert!(rendered.contains("GenericBox") && rendered.contains("string"));
}

// ===========================================================================
// §B — empty collections on a FREE function (low-evidence ⇒ rust_type)
// ===========================================================================

#[test]
fn test_first_or_empty_list_round_trips_none() {
    // B7: a free function has no wire-arg channel and an empty list yields no
    // element evidence ⇒ the element T = rust_type; `first_or([])` returns
    // None. (Contrast read_items over a *bound* instance with empty fields,
    // B6, where T is still recovered from the wire type-arg.)
    // DIVERGENCE(rust): an unannotated `vec![]` has no element type for rustc
    // either — the turbofish supplies the element type the engine never sees
    // evidence for.
    assert_eq!(first_or::<i64>(vec![]).unwrap(), None);
}

#[test]
fn test_first_or_nonempty_infers_element() {
    // B7 twin: a non-empty list DOES carry element evidence, so the same
    // function binds T from the element and returns the head.
    assert_eq!(first_or(vec![7, 8, 9]).unwrap(), Some(7));
}

#[test]
fn test_values_of_empty_map_round_trips_empty_list() {
    // B9: the map-value position is the only evidence channel and the empty
    // map yields no value ⇒ T = rust_type; `values_of({})` returns []. Pins
    // that the empty-collection rule applies to `map<_, T>`, not just `T[]`.
    // DIVERGENCE(rust): an unannotated empty map has no value type for rustc
    // either — the turbofish supplies it.
    assert_eq!(values_of::<i64>(Map::new()).unwrap(), Vec::<i64>::new());
}

#[test]
fn test_values_of_nonempty_returns_values() {
    // B9 twin: a non-empty map carries value evidence and returns its values.
    assert_eq!(
        values_of(Map::from([("a".to_string(), 1), ("b".to_string(), 2)])).unwrap(),
        vec![1, 2]
    );
}

// ===========================================================================
// §C — caller-specified & partial binding via the SUBSCRIPT host surface
// (distinct from the `_types=` surface above). C1 seeds one var, C3 seeds all.
// ===========================================================================

#[test]
fn test_make_triple_partial_subscript_requires_full_arity() {
    // C1 (pins the host surface): the SUBSCRIPT form requires *all* type args
    // — a partial `make_triple[int]` (1 of 3) raises a host-side TypeError
    // before the call. Partial seed-then-infer is the `_types=` surface (C2,
    // test_make_triple_partial_explicit_then_infer), not subscript; the full
    // subscript is C3 (test_make_triple_subscript_fully_bound).
    // DIVERGENCE(rust): `make_triple::<i64>(...)` with one of three type args
    // is a compile error (wrong number of generic arguments, rustc E0107);
    // Rust's surface for the partial seed is turbofish `_` holes
    // (test_make_triple_partial_explicit_then_infer).
}

#[test]
fn test_make_triple_subscript_fully_bound() {
    // C3: every var is seeded by the subscript, inference does nothing, and
    // each arg is validated against its now-concrete formal. A cross-check
    // that the fully-bound path agrees with the partial/inferred cases (the
    // explicit suite in test_generic_calls.rs exercises this path broadly).
    let t = make_triple::<i64, String, bool>(
        5,
        vec!["x".to_string()],
        Map::from([("k".to_string(), true)]),
    )
    .unwrap();
    assert_eq!(t.first, 5);
    assert_eq!(t.second, ["x"]);
    assert_eq!(t.third, Map::from([("k".to_string(), true)]));
}

// ===========================================================================
// §E — must-specify (negatives above); the explicit `_types=` form succeeds.
// ===========================================================================

#[test]
fn test_one_type_arg_explicit_types_succeeds() {
    // E2: the body-only var is uninferable (E1), but supplying it via
    // `_types=` (Rust surface: the turbofish) succeeds and reflects the bound
    // type.
    assert_eq!(one_type_arg::<i64>().unwrap(), "int");
}

#[test]
fn test_parse_as_explicit_types_succeeds() {
    // E4: the return-only var is uninferable (E3); `_types=` (Rust surface:
    // the turbofish) binds it and the value parses to the bound type.
    assert_eq!(parse_as::<i64>("42".to_string()).unwrap(), 42);
}

// ===========================================================================
// §G — unbound generic instances: recover if the formal forces recursion,
// else host-only `rust_type` (and bound ≠ unbound).
// ===========================================================================

#[test]
fn test_second_of_unbound_instance_recovers_field_type() {
    // G1: an UNBOUND `GenericPair(first=1, second="hi")` (no `[int, str]`)
    // carries no wire type-args, but the formal `GenericPair<int, T>` forces
    // inference into the second slot ⇒ T=string recovered from the field
    // VALUE; returns "hi". (Contrast
    // B2/test_second_of_infers_from_nested_generic, a *bound* instance.)
    // DIVERGENCE(rust): there is no unbound instance — the un-turbofished
    // struct literal below is fully typed by rustc's own inference before the
    // call is made, which is the closest Rust analogue of the formal-directed
    // recovery.
    assert_eq!(
        second_of(GenericPair {
            first: 1,
            second: "hi".to_string(),
        })
        .unwrap(),
        "hi"
    );
}

#[test]
fn test_identity_nested_unbound_round_trips() {
    // G3: an outer UNBOUND instance under a bare-`T` formal ⇒ the whole value
    // is rust_type and rides opaquely, round-tripping unchanged.
    // DIVERGENCE(rust): the un-turbofished literal is fully typed by rustc
    // inference, so the value rides as an ordinary bound instance; only the
    // round-trip itself is portable.
    let nested = GenericBox {
        value: GenericBox {
            value: "hello".to_string(),
        },
    };
    assert_eq!(identity(nested.clone()).unwrap(), nested);
}

#[test]
fn test_wrap_infers_and_returns_bound_generic() {
    // G4 (positive half): `wrap(5)` infers T=int and returns a properly-bound
    // `GenericBox[int]`, equal to the bound literal. The bound≠unbound
    // discriminator proper is a value-layer concern (round-tripped values
    // differ) asserted at the bex layer — Pydantic `==` ignores the generic
    // parameterization, so `GenericBox[int](value=5) == GenericBox(value=5)`
    // here and the distinction isn't observable through Python equality.
    assert_eq!(wrap(5).unwrap(), GenericBox::<i64> { value: 5 });
}

// ===========================================================================
// §I — nullable param, literal/enum widening edges.
// ===========================================================================

#[test]
fn test_maybe_id_present_value_infers() {
    // I1: the non-null arm of `T?` binds against the int actual ⇒ T=int; the
    // value round-trips.
    assert_eq!(maybe_id(Some(5)).unwrap(), Some(5));
}

#[test]
fn test_maybe_id_null_round_trips() {
    // I4: a `null`-only actual gives the value position no concrete leaf ⇒
    // T=rust_type (we do NOT null-strip `T?` to bind `T=null`); None
    // round-trips.
    // DIVERGENCE(rust): a bare `None` carries no type for rustc either — the
    // turbofish supplies the element type the engine never sees evidence for.
    assert_eq!(maybe_id::<i64>(None).unwrap(), None);
}

#[test]
fn test_identity_enum_round_trips() {
    // I3 (python surface): an enum value rides through inference and
    // round-trips. The codegen emits `SomeEnum(str, enum.Enum)`, but
    // proto.py's `enum` arm now precedes its `str` arm, so a str-enum encodes
    // on the wire as an `EnumVariant` (T binds to the enum type `SomeEnum`,
    // not `string`) and the value decodes back to the enum member — matching
    // the bex layer, where a `Variant` actual is unambiguously an enum. The
    // `isinstance` check is load-bearing: a bare `string` round-trip (the old
    // behavior) would still pass `== SomeEnum.VARIANT` via str-enum equality,
    // so only the type assertion proves the value came back as a real enum
    // member rather than its string value.
    // DIVERGENCE(rust): the generated `SomeEnum` is a real Rust enum, not a
    // str-subclass — the static return type is python's `isinstance`
    // assertion, and string equality can't sneak through.
    let result = identity(SomeEnum::VARIANT).unwrap();
    assert_eq!(result, SomeEnum::VARIANT);
}

// ===========================================================================
// §F — host-only object boundary (RustType round-trip lives at the bex layer).
// ===========================================================================

#[test]
fn test_host_only_object_not_encodable_from_python() {
    // §F (host boundary): the §F RustType round-trip (an arbitrary host
    // object riding opaquely) is reachable at the bex/value layer, but the
    // Python bridge only encodes primitives, lists, maps, callables, and
    // Pydantic models — an arbitrary Python object has no wire encoding and
    // is rejected at encode time with a TypeError BEFORE the call reaches the
    // engine. This pins the SDK-side boundary that makes F1–F3 a bex-only
    // concern.
    // DIVERGENCE(rust): the same boundary is the encode trait bound on the
    // generated signature — `identity(HostThing { n: 3 })` for a local struct
    // that doesn't implement it is a compile error, so the encode-time
    // rejection cannot occur at runtime.
}

// ===========================================================================
// §J J13 — a function-typed (host callable) argument poisons its TypeVars: they
// must be specified up front (the bridge can't infer from / validate against an
// opaque handle), even though `x` would otherwise pin `T`.
// ===========================================================================

#[test]
fn test_apply_closure_poisons_typevars_must_specify() {
    // J13: `apply(lambda v: v + 1, 5)` — `T` is poisoned by its occurrence in
    // the closure parameter `(T)` (even though `x=5` would pin it) and `R`
    // lives only in the closure's return, so both must be specified; bare ⇒
    // rejected as a Python `TypeError` complaining that a type parameter
    // couldn't be inferred.
    // DIVERGENCE(rust): rustc solves `T` and `R` from the closure and `x` at
    // COMPILE time, so the generated wrapper always has both reified and can
    // send them explicitly — a bare `apply(|v| v + 1, 5)` is equivalent to
    // the specified form and the runtime poisoning rejection cannot occur.
}

#[test]
fn test_apply_closure_typevars_specified_succeeds() {
    // J13 (positive): once `T` and `R` are specified, the call goes through
    // and the callable is invoked ⇒ apply(lambda v: v + 1, 5) == 6.
    // Rust surface for `_types={"T": int, "R": int}`: annotating the closure.
    assert_eq!(apply(|v: i64| -> i64 { v + 1 }, 5).unwrap(), 6);
}

// ===========================================================================
// §L — methods: class T from the receiver, method vars from method args.
// ===========================================================================

#[test]
fn test_genericbox_get_infers_class_var_from_receiver() {
    // L1: GenericBox[int](value=5).get() == "int" — class T recovered from
    // the receiver's wire type-args (no method var to infer).
    assert_eq!(GenericBox::<i64> { value: 5 }.get().unwrap(), "int");
}

#[test]
fn test_genericbox_pair_with_unbound_receiver_recovers_class_var() {
    // L5: a BARE method call on an UNBOUND receiver `GenericBox(value=5)` (no
    // `[int]`) sends empty class type-args, but the method's
    // `self: GenericBox<T>` formal forces recursion into the `value` field
    // (the G1 path) ⇒ class T=int recovered from `value=5`, unioned with
    // method var U=string ⇒ "int | string". (The bare path is supported
    // precisely because no method param is explicitly bound; the
    // `_types=`/subscript form on an unparameterized receiver raises — see
    // test_generic_calls.rs' test_instance_method_unparameterized_receiver_raises.)
    // DIVERGENCE(rust): the un-turbofished receiver literal is fully typed by
    // rustc inference — there is no unbound receiver — so only the call
    // result is portable.
    assert_eq!(
        GenericBox { value: 5 }.pair_with("x".to_string()).unwrap(),
        "int | string"
    );
}

// ===========================================================================
// §C C4 — a caller-specified binding contradicted by the actual value rejects at
// the engine (Gate B), on BOTH host surfaces: the `_types=` kwarg and the `[...]`
// subscript (which is pure sugar over `_types=` and adds no Python-side value
// validation of its own — see `_GenericCallable.__getitem__`). Both reach the
// same engine check and reject identically.
// ===========================================================================

#[test]
fn test_make_triple_types_kwarg_contradicted_by_actual_rejects() {
    // C4 (`_types=` surface): a partial `_types={"A": int}` fixes A=int, but
    // `a="nope"` is a string. Inference is bypassed for the caller-specified
    // A, so the engine's per-arg structural check (Gate B) is the only gate —
    // and it now rejects the contradicting scalar at CALL time as a
    // `TypeMismatch` (Python `TypeError`), naming the function. (Previously
    // this seam skipped every non-instance arg, so the call ran and the
    // mismatch only surfaced later at DECODE time as a Pydantic
    // ValidationError when re-validating the returned value.) Only `_types=`
    // can bind a *partial* set of vars — the subscript requires full arity
    // (C1).
    // DIVERGENCE(rust): `make_triple::<i64, _, _>("nope", ...)` cannot
    // compile — the seeded `A = i64` types the first parameter, so a string
    // actual is a compile error and the engine's Gate B is unreachable; type
    // args and values cannot disagree through the typed wrappers.
}

#[test]
fn test_make_triple_full_subscript_contradicted_by_actual_rejects() {
    // C4 (subscript surface): `make_triple[int, str, bool]("nope", ...)`
    // seeds every var via the subscript, which is pure sugar for
    // `_types={"A": int, "B": str, "C": bool}` (`__getitem__` →
    // `functools.partial(..., _types=bound)` → the same call path → bex). The
    // subscript adds NO value validation of its own — it only checks type-arg
    // *arity* (C1) — so the `a="nope"` string vs the now-concrete `int`
    // formal is caught at the SAME engine Gate B as the `_types=` surface,
    // surfacing as a `TypeError` naming the function. Pins that the subscript
    // path delegates rather than re-validating, and that both surfaces reject
    // identically.
    // DIVERGENCE(rust): same as the partial-seed case — the full turbofish
    // types every parameter, so the contradicting actual is a compile error
    // and both host surfaces collapse into the one that cannot disagree.
}

// ===========================================================================
// §B/§D — heterogeneous array unification: the elements of one T[] union-merge
// into the element type, so inference over a mixed array yields a union.
// ===========================================================================

#[test]
fn test_elem_type_heterogeneous_array_unifies() {
    // The mixed elements of a single `T[]` union-merge while synthesizing the
    // container's element type ⇒ elem_type([1, "x"]) binds T = int | string.
    // Directly asserts the unified element type (B8 only reads back the
    // values).
    // DIVERGENCE(rust): a heterogeneous `Vec` is unrepresentable, so the
    // element union-merge is unreachable through the typed wrappers.
}

#[test]
fn test_elem_type_homogeneous_array_is_single_type() {
    // The degenerate case: a homogeneous array dedups to a single type.
    assert_eq!(elem_type(vec![1, 2, 3]).unwrap(), "int");
}

#[test]
fn test_elem_type_three_way_heterogeneous_array_unifies() {
    // n-ary element union: three distinct element types all merge.
    // DIVERGENCE(rust): a heterogeneous `Vec` is unrepresentable, so the
    // n-ary element union is unreachable through the typed wrappers.
}

// ===========================================================================
// §G generalized — an UNBOUND generic instance (constructed WITHOUT type args)
// is still inferrable when the formal forces recursion into its fields.
//
// Normally an unbound generic instance carries no wire type-args and rides as
// host-only `rust_type` (G2). But when the parameter's formal is itself
// `Container<T>` / `Recursive<T>` / nested `Pair<...>`, inference is DIRECTED
// into the corresponding field values and recovers `T` from them (G1) — so a
// Python caller who forgot the `[int]` subscript still gets a working call.
//
// DIVERGENCE(rust): rustc's own inference types every un-turbofished literal
// below before the call is made, standing in for the engine's field-directed
// recovery; the runtime round trips are what remains portable.
// ===========================================================================

#[test]
fn test_read_items_unbound_container_recovers_T_from_fields() {
    // ContainerShapes constructed WITHOUT `[int]`: no wire type-args, but the
    // `read_items(shape: ContainerShapes<T>)` formal forces recursion into
    // the fields ⇒ T=int recovered from the field VALUES; returns `items`.
    let unbound = ContainerShapes {
        item: 1,
        items: vec![1, 2, 3],
        by_key: Map::from([("k".to_string(), 4)]),
        maybe: None,
        mixed: None,
    };
    assert_eq!(read_items(unbound).unwrap(), vec![1, 2, 3]);
}

#[test]
fn test_list_head_unbound_recursive_recovers_T_from_fields() {
    // GenericRecursive constructed WITHOUT `[int]`: the `list_head(list:
    // GenericRecursive<T>)` formal forces recursion into `value`/`next` ⇒
    // T=int recovered from the field values even though the wire carries no
    // type-args.
    let unbound = GenericRecursive {
        value: 7,
        next: Some(Box::new(GenericRecursive {
            value: 8,
            next: None,
        })),
    };
    assert_eq!(list_head(unbound).unwrap(), 7);
}

#[test]
fn test_extract_fully_unbound_nested_pair_recovers_all_vars() {
    // Nested GenericPair with NO `[...]` subscripts at ANY level — every
    // instance is unbound. The `extract(a: GenericPair<GenericPair<A,B>,
    // GenericPair<C,D>>)` formal drives recursion all the way down: the
    // engine reconstructs each nested unbound instance against its slot's
    // formal (deep G1), recovering A,B,C,D from the leaf field values. So a
    // caller who forgot every subscript still gets a working call.
    let fully_unbound = GenericPair {
        first: GenericPair {
            first: 1,
            second: "a".to_string(),
        },
        second: GenericPair {
            first: true,
            second: 1.5,
        },
    };
    assert_eq!(
        extract(fully_unbound).unwrap(),
        "int | string | bool | float"
    );
}

// ===========================================================================
// §D concrete-type join — the covariant union-merge also handles non-primitive
// actuals: a concrete BAML class and an enum participate in the join.
// ===========================================================================

#[test]
fn test_triple_choose_join_includes_concrete_class() {
    // triple_choose(int, StringIntPair, string) — the covariant join merges a
    // primitive, a concrete BAML class, and a primitive ⇒ T includes
    // StringIntPair.
    // DIVERGENCE(rust): three divergent actuals for one `T` cannot compile,
    // so the concrete-class join is unreachable through the typed wrappers.
}

#[test]
fn test_triple_choose_join_includes_enum_variant() {
    // triple_choose(int, SomeEnum, StringIntPair) — the covariant join merges
    // a primitive, an enum, and a concrete class. proto.py now encodes a
    // str-enum as an `EnumVariant` (see test_identity_enum_round_trips), so
    // the enum actual rides as the enum type `SomeEnum` and the join is the
    // full `T = int | SomeEnum | StringIntPair`.
    // DIVERGENCE(rust): three divergent actuals for one `T` cannot compile,
    // so the enum-including join is unreachable through the typed wrappers.
}
