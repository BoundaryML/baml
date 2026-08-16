"""Generic *function-call* coverage — the INFERENCE variant (ns_generic_tests).

Sibling of `test_generic_calls.py` (the explicit-subscript suite). Here every
call is **bare**: no `fn[T](...)` subscript and no `_types=`. The engine solves
each TypeVar from the argument *values* (inbound-inference, 01a/01b), so these
calls produce the same result the explicit form does — minus the binding the
caller no longer has to write.

Case labels map to `thoughts/.../inbound-inference/00b3-labeled-cases.md`.
A TypeVar buried in a union beside a concrete member (00b3 G5/§H) is now IN
SCOPE (02a reverses G5): inference subtracts the concrete siblings and routes
the residual to the TypeVar. Genuinely uninferable cases (return/body-only
vars, §E; a value fully absorbed by a concrete sibling) still require `_types=`
and are pinned here as negative cases — inference leaves them for Gate A.
"""

import pytest

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.generic_tests import (
    StringIntPair,
    GenericPair,
    GenericTriple,
    GenericBox,
    GenericRecursive,
    ContainerShapes,
    NamedStatic,
    SomeEnum,
    identity,
    second_of,
    tag_or_value,
    list_head,
    choose,
    read_items,
    make_triple,
    extract,
    wrap,
    parse_as,
    one_type_arg,
    optional_only,
    maybe_id,
    first_or,
    values_of,
    elem_type,
    apply,
    pair,
    merge,
    combine,
    glue,
    triple_choose,
    two_in_union,
)


def _assert_type_error(call, *needles):
    """A failed generic call must surface as a Python ``TypeError`` (the engine's
    `EngineError::TypeMismatch` ⇒ `baml.errors.TypeMismatch` ⇒ native `TypeError`,
    not an opaque `BamlPanic`), and its message must mention each `needle`. Pins
    both the *type* and the *message* of every inference rejection."""
    with pytest.raises(TypeError) as excinfo:
        call()
    message = str(excinfo.value)
    for needle in needles:
        assert needle in message, f"missing {needle!r} in TypeError message: {message!r}"
    return excinfo


# ===========================================================================
# §A — single TypeVar inferred from one argument value
# ===========================================================================


def test_generic_inference_identity_infers_primitives():
    # T1/T2: T bound from the value; identity returns it unchanged.
    assert identity(5) == 5
    assert identity("hi") == "hi"
    assert identity(True) is True


def test_generic_inference_identity_infers_user_class():
    # T3: T = StringIntPair, recovered from the instance value.
    pair = StringIntPair(my_string="a", my_int=1)
    assert identity(pair) == pair


def test_generic_inference_identity_infers_generic_instance():
    # T4: a fully-bound GenericBox[int] carries its [int] on the wire, so T is
    # recovered as GenericBox<int> with no caller binding.
    box = GenericBox[int](value=5)
    assert identity(box) == box

    nested = GenericBox[GenericBox[str]](value=GenericBox[str](value="hello"))
    assert identity(nested) == nested


async def test_generic_inference_identity_async_infers():
    # T5: the async path infers identically.
    from baml_sdk.generic_tests import identity_async

    assert await identity_async(7) == 7


def test_generic_inference_identity_null_round_trips():
    # §I I4 (decided): a `null` actual is no inference evidence (NOT bound as
    # `T=null`) ⇒ `T` defaults to host-only `rust_type`, and the value round-trips
    # unchanged.
    assert identity(None) is None


def test_generic_inference_default_only_value_position():
    assert optional_only() is None
    assert optional_only(x=7) == 7


def test_generic_inference_identity_unbound_generic_instance_round_trips():
    # §G G2 (decided): an UNBOUND generic instance — constructed without the
    # `[int]` subscript — carries no wire type-args, so it is host-only
    # (`T=rust_type`) and rides through the VM opaquely, round-tripping unchanged
    # (and staying distinct from a properly-bound `GenericBox[int]`, G4).
    unbound = GenericBox(value=5)
    assert identity(unbound) == unbound


# ===========================================================================
# §B — structural / container solving across one or more arguments
# ===========================================================================


def test_generic_inference_make_triple_infers_multiple_typevars():
    # T6: A=int (scalar), B=string (list element), C=bool (map value) — all three
    # inferred from differently-shaped arguments at once.
    t = make_triple(1, ["a", "b"], {"k": True})
    assert isinstance(t, GenericTriple)
    assert t.first == 1
    assert t.second == ["a", "b"]
    assert t.third == {"k": True}


def test_generic_inference_second_of_infers_from_nested_generic():
    # T9: second_of<T>(p: GenericPair<int, T>) — T binds from the instance's 2nd
    # wire arg only (`first` is pinned to int in the signature).
    assert second_of(GenericPair[int, str](first=1, second="hi")) == "hi"
    pair = StringIntPair(my_string="z", my_int=9)
    p = GenericPair[int, StringIntPair](first=0, second=pair)
    assert second_of(p) == pair


def test_generic_inference_read_items_infers_from_instance_wire_args():
    # T10: ContainerShapes<T> — T recovered from the instance's single wire arg,
    # NOT by re-unifying every field. Empty fields don't erase it (T42).
    container = ContainerShapes[int](
        item=1, items=[1, 2, 3], by_key={"k": 4}, maybe=None, mixed=None
    )
    assert read_items(container) == [1, 2, 3]

    empty_fields = ContainerShapes[int](
        item=1, items=[], by_key={}, maybe=None, mixed=None
    )
    assert read_items(empty_fields) == []


def test_generic_inference_list_head_infers_from_recursive_generic():
    # T11: GenericRecursive<T> bottoms out at next=None; T binds from the wire arg.
    linked = GenericRecursive[int](
        value=7, next=GenericRecursive[int](value=8, next=None)
    )
    assert list_head(linked) == 7


def test_generic_inference_extract_infers_four_typevars_from_nesting():
    # T12: A,B,C,D recovered by walking the nested GenericPair instantiation.
    pair = GenericPair[GenericPair[int, str], GenericPair[bool, float]](
        first=GenericPair[int, str](first=1, second="a"),
        second=GenericPair[bool, float](first=True, second=1.5),
    )
    assert extract(pair) == "int | string | bool | float"


# ===========================================================================
# §C — union unification: one TypeVar across two argument positions
# ===========================================================================


def test_generic_inference_choose_infers_unified_typevar():
    # T14: choose(5, 6) ⇒ T = int (the two bindings merge to one). Body returns
    # `left`, so the call returns 5.
    assert choose(5, 6) == 5
    assert choose("a", "b") == "a"


def test_generic_inference_choose_infers_divergent_union():
    # T15: choose(5, "asdf") ⇒ T = int | string (a capability inference unlocks
    # over the explicit form, which forces a single T). Returns `left` = 5.
    assert choose(5, "asdf") == 5


# ===========================================================================
# §D — partial binding: explicit seed for one TypeVar, infer the rest
# ===========================================================================


def test_generic_inference_make_triple_partial_explicit_then_infer():
    # C2/T17: bind A explicitly via a partial `_types=` dict; B and C are inferred.
    #
    # NOTE: this is an *unusual* situation — only SOME type vars are explicitly
    # bound while the rest are inferred. Users should generally NOT reach for
    # `_types=` at all: inbound inference binds every value-carried TypeVar from
    # the arguments (see the rest of this file), and the explicit *subscript*
    # form (`make_triple[int, str, bool](...)`, test_make_triple_subscript_*) is
    # the supported surface for the rare case where a binding must be forced.
    # `_types=` is an internal wiring detail kept mainly for this partial-bind
    # escape hatch; prefer plain inference.
    t = make_triple(1, ["x", "y"], {"k": True}, _types={"A": int})
    assert isinstance(t, GenericTriple)
    assert t.first == 1
    assert t.second == ["x", "y"]
    assert t.third == {"k": True}


# ===========================================================================
# §G/outbound — infer T, return a generic over it
# ===========================================================================


def test_generic_inference_wrap_infers_and_returns_generic():
    # T29: wrap(5) infers T=int and returns a GenericBox<int>.
    w = wrap(5)
    assert isinstance(w, GenericBox)
    assert w.value == 5


# ===========================================================================
# §K — methods: class T from the receiver, method TypeVars inferred from args
# ===========================================================================


def test_generic_inference_genericbox_pair_with_infers_method_typevar():
    # T37: class T=int from the GenericBox[int] receiver; method U=string inferred
    # from the bare `other` arg (no [str] subscript).
    b = GenericBox[int](value=5)
    assert b.pair_with("hello world") == "int | string"


def test_generic_inference_generic_static_infers_own_typevar():
    # T38: GenericBox.new<V>(value: V) — V inferred from the value, no subscript.
    box = GenericBox.new(value=5)
    assert isinstance(box, GenericBox)
    assert box.value == 5


def test_generic_inference_named_static_infers_distinct_typevars():
    # T39: NamedStatic.make<D,E>(d, e) — D=int, E=string inferred from the args.
    assert NamedStatic.make(1, "x") == "int | string"


# ===========================================================================
# Out-of-scope / must-specify: inference finds no evidence ⇒ engine rejects
# ===========================================================================


def test_generic_inference_union_with_concrete_sibling_infers_typevar():
    # 02a reverses 00b3 G5/§H: a TypeVar buried in a union beside concrete
    # members (`x: T | string | null`) is NOW solved by inference. The `int`
    # actual is not absorbed by the `string`/`null` siblings, so it routes to
    # `T` ⇒ T=int, matching the explicit form `tag_or_value[int](5) == "int"`.
    assert tag_or_value(5) == "int"


def test_generic_inference_union_concrete_sibling_absorbs_value_binds_rust_type():
    # §H H3 (decided): a `string` actual IS absorbed by the concrete `string`
    # sibling, so nothing routes to `T`. `T` still has a value position (the `x`
    # param) and no closure occurrence, so it defaults to host-only `rust_type`
    # (rule 4) rather than being rejected.
    assert tag_or_value("hi") == "$rust_type"


def test_generic_inference_union_null_actual_binds_rust_type():
    # §H H3 / §I I4 (decided): a `null` actual is no inference evidence (not bound
    # as `T=null`), and the `null` sibling absorbs it, so `T` defaults to
    # `rust_type`.
    assert tag_or_value(None) == "$rust_type"


def test_generic_inference_return_only_var_still_requires_binding():
    # §E: parse_as<T>(source: string) -> T — T appears only in return position,
    # so no argument can carry it. Inference finds nothing ⇒ the engine rejects
    # the call as a TYPE error (Python `TypeError`), and the message complains
    # that the type parameter couldn't be inferred and names the function.
    _assert_type_error(
        lambda: parse_as("42"),
        "could not infer a type",
        "parse_as",
    )


def test_generic_inference_body_only_var_still_requires_binding():
    # §E: one_type_arg<T>() reflects T but takes no argument ⇒ uninferable ⇒ a
    # Python `TypeError` whose message complains about the un-inferrable type
    # parameter and names the function.
    _assert_type_error(
        lambda: one_type_arg(),
        "could not infer a type",
        "one_type_arg",
    )


# ===========================================================================
# §J — variance soundness (02d/02e): conflicting occurrences of one TypeVar
# across invariant/covariant positions have no consistent binding ⇒ REJECT,
# instead of fabricating an unsound union. Agreeing occurrences still bind.
# ===========================================================================


def test_generic_inference_pair_invariant_list_conflict_rejects():
    # J4/E1: pair(int[], string[]) ⇒ a⇒T==int, b⇒T==string (both invariant list
    # elements) ⇒ no consistent T ⇒ reject (the old unifier fabricated
    # `(int|string)[]`). Surfaces as a Python `TypeError` whose message names the
    # function, the clashing concrete types, and that they can't be reconciled.
    _assert_type_error(
        lambda: pair([1, 2], ["a", "b"]),
        "can't be reconciled",
        "pair",
        "int",
        "string",
    )


def test_generic_inference_pair_invariant_list_agree_binds():
    # J9/G1: pair(int[], int[]) ⇒ two invariant occurrences that AGREE ⇒ T = int.
    # The fix narrows behavior, so this must still succeed.
    assert pair([1, 2], [3, 4]) == "int"


def test_generic_inference_choose_union_outside_container_is_sound():
    # J10/G2: choose(int[], string[]) — both occurrences are covariant (bare `T`),
    # so the union forms OUTSIDE the container (T = int[] | string[]) and the call
    # SUCCEEDS, returning `left`. Proves the fix keys on position variance, not
    # "arrays are involved." (Contrast pair, where T is under the container.)
    assert choose([1, 2], ["a"]) == [1, 2]


def test_generic_inference_merge_invariant_map_value_conflict_rejects():
    # J5/E2: merge(map<string,int>, map<string,string>) ⇒ conflicting invariant
    # map-value type ⇒ reject as a Python `TypeError`.
    _assert_type_error(
        lambda: merge({"k": 1}, {"k": "a"}),
        "can't be reconciled",
        "merge",
    )


def test_generic_inference_combine_invariant_class_arg_conflict_rejects():
    # J6/E3: combine(GenericBox[int], GenericBox[string]) ⇒ Box<T> invariant,
    # int ≠ string ⇒ reject as a Python `TypeError`.
    _assert_type_error(
        lambda: combine(GenericBox[int](value=1), GenericBox[str](value="x")),
        "can't be reconciled",
        "combine",
    )


def test_generic_inference_glue_invariant_vs_covariant_conflict_rejects():
    # J7/E4: glue(int, string[]) ⇒ arr⇒T==string (invariant) but bare⇒int <: T
    # (covariant); int <: string is false ⇒ reject as a Python `TypeError`.
    _assert_type_error(
        lambda: glue(1, ["a"]),
        "can't be reconciled",
        "glue",
    )


def test_generic_inference_glue_invariant_and_covariant_agree_binds():
    # J11/G4: glue(int, int[]) ⇒ invariant (T==int) + covariant (int <: int)
    # AGREE ⇒ T = int; must still succeed.
    assert glue(1, [2, 3]) == "int"


def test_generic_inference_two_typevar_union_is_uninferrable_rejects():
    # J12: two_in_union<T,U>(x: T | U | int) ⇒ two free vars in one union have no
    # principled split without an explicit hint ⇒ reject as a Python `TypeError`
    # (distinct from §H, which is ONE var beside concrete members).
    _assert_type_error(
        lambda: two_in_union("hello"),
        "two_in_union",
    )


# ===========================================================================
# §D — n-ary covariant join, and §B heterogeneous container element
# ===========================================================================


def test_generic_inference_triple_choose_three_covariant_join():
    # D3: triple_choose(5, "asdf", True) ⇒ T = int | string | bool — three
    # covariant bare-arg occurrences union-merge (n-ary, not pairwise).
    assert triple_choose(5, "asdf", True) == "int | string | bool"


def test_generic_inference_make_triple_heterogeneous_list_element_unions():
    # B8: make_triple(1, [1, "x"], {"k": True}) ⇒ B = int | string — the list's
    # mixed elements union-merge while synthesizing ONE container's element type
    # (the §D join applied INSIDE a container; distinct from §J's invariant
    # conflict between two separate args). The heterogeneous list round-trips.
    t = make_triple(1, [1, "x"], {"k": True})
    assert isinstance(t, GenericTriple)
    assert t.second == [1, "x"]


def test_generic_inference_choose_divergent_generic_instances_union():
    # D2: choose(GenericBox[int], GenericBox[str]) ⇒ T = GenericBox<int> |
    # GenericBox<string>, the union OUTSIDE the box (both occurrences covariant).
    # Body returns `left`, so the int box comes back. Contrast `combine`, where T
    # is INSIDE the box and the same actuals conflict (§J).
    left = GenericBox[int](value=1)
    assert choose(left, GenericBox[str](value="x")) == left


def test_generic_inference_tag_or_value_binds_generic_instance():
    # H2: tag_or_value(GenericBox[str]) ⇒ the instance is not absorbed by the
    # `string`/`null` siblings, so it routes to T ⇒ T = GenericBox<string>.
    rendered = tag_or_value(GenericBox[str](value="asdf"))
    assert "GenericBox" in rendered and "string" in rendered


# ===========================================================================
# §B — empty collections on a FREE function (low-evidence ⇒ rust_type)
# ===========================================================================


def test_generic_inference_first_or_empty_list_round_trips_none():
    # B7: a free function has no wire-arg channel and an empty list yields no
    # element evidence ⇒ the element T = rust_type; `first_or([])` returns None.
    # (Contrast read_items over a *bound* instance with empty fields, B6, where
    # T is still recovered from the wire type-arg.)
    assert first_or([]) is None


def test_generic_inference_first_or_nonempty_infers_element():
    # B7 twin: a non-empty list DOES carry element evidence, so the same function
    # binds T from the element and returns the head.
    assert first_or([7, 8, 9]) == 7


def test_generic_inference_values_of_empty_map_round_trips_empty_list():
    # B9: the map-value position is the only evidence channel and the empty map
    # yields no value ⇒ T = rust_type; `values_of({})` returns []. Pins that the
    # empty-collection rule applies to `map<_, T>`, not just `T[]`.
    assert values_of({}) == []


def test_generic_inference_values_of_nonempty_returns_values():
    # B9 twin: a non-empty map carries value evidence and returns its values.
    assert values_of({"a": 1, "b": 2}) == [1, 2]


# ===========================================================================
# §C — caller-specified & partial binding via the SUBSCRIPT host surface
# (distinct from the `_types=` surface above). C1 seeds one var, C3 seeds all.
# ===========================================================================


def test_generic_inference_make_triple_partial_subscript_requires_full_arity():
    # C1 (pins the host surface): the SUBSCRIPT form requires *all* type args — a
    # partial `make_triple[int]` (1 of 3) raises a host-side TypeError before the
    # call. Partial seed-then-infer is the `_types=` surface (C2,
    # test_make_triple_partial_explicit_then_infer), not subscript; the full
    # subscript is C3 (test_make_triple_subscript_fully_bound).
    with pytest.raises(TypeError):
        make_triple[int](5, ["hello", "world"], {"asdf": 5})


def test_generic_inference_make_triple_subscript_fully_bound():
    # C3: every var is seeded by the subscript, inference does nothing, and each
    # arg is validated against its now-concrete formal. A cross-check that the
    # fully-bound path agrees with the partial/inferred cases (the explicit suite
    # in test_generic_calls.py exercises this path broadly).
    t = make_triple[int, str, bool](5, ["x"], {"k": True})
    assert isinstance(t, GenericTriple)
    assert t.first == 5
    assert t.second == ["x"]
    assert t.third == {"k": True}


# ===========================================================================
# §E — must-specify (negatives above); the explicit `_types=` form succeeds.
# ===========================================================================


def test_generic_inference_one_type_arg_explicit_types_succeeds():
    # E2: the body-only var is uninferable (E1), but supplying it via `_types=`
    # succeeds and reflects the bound type.
    assert one_type_arg(_types={"T": int}) == "int"


def test_generic_inference_parse_as_explicit_types_succeeds():
    # E4: the return-only var is uninferable (E3); `_types=` binds it and the
    # value parses to the bound type.
    assert parse_as("42", _types={"T": int}) == 42


# ===========================================================================
# §G — unbound generic instances: recover if the formal forces recursion,
# else host-only `rust_type` (and bound ≠ unbound).
# ===========================================================================


def test_generic_inference_second_of_unbound_instance_recovers_field_type():
    # G1: an UNBOUND `GenericPair(first=1, second="hi")` (no `[int, str]`) carries
    # no wire type-args, but the formal `GenericPair<int, T>` forces inference into
    # the second slot ⇒ T=string recovered from the field VALUE; returns "hi".
    # (Contrast B2/test_second_of_infers_from_nested_generic, a *bound* instance.)
    assert second_of(GenericPair(first=1, second="hi")) == "hi"


def test_generic_inference_identity_nested_unbound_round_trips():
    # G3: an outer UNBOUND instance under a bare-`T` formal ⇒ the whole value is
    # rust_type and rides opaquely, round-tripping unchanged.
    nested = GenericBox(value=GenericBox(value="hello"))
    assert identity(nested) == nested


def test_generic_inference_wrap_infers_and_returns_bound_generic():
    # G4 (positive half): `wrap(5)` infers T=int and returns a properly-bound
    # `GenericBox[int]`, equal to the bound literal. The bound≠unbound
    # discriminator proper is a value-layer concern (round-tripped values differ)
    # asserted at the bex layer — Pydantic `==` ignores the generic
    # parameterization, so `GenericBox[int](value=5) == GenericBox(value=5)` here
    # and the distinction isn't observable through Python equality.
    assert wrap(5) == GenericBox[int](value=5)


# ===========================================================================
# §I — nullable param, literal/enum widening edges.
# ===========================================================================


def test_generic_inference_maybe_id_present_value_infers():
    # I1: the non-null arm of `T?` binds against the int actual ⇒ T=int; the value
    # round-trips.
    assert maybe_id(5) == 5


def test_generic_inference_maybe_id_null_round_trips():
    # I4: a `null`-only actual gives the value position no concrete leaf ⇒
    # T=rust_type (we do NOT null-strip `T?` to bind `T=null`); None round-trips.
    assert maybe_id(None) is None


def test_generic_inference_identity_enum_round_trips():
    # I3 (python surface): an enum value rides through inference and round-trips.
    # The codegen emits `SomeEnum(str, enum.Enum)`, but proto.py's `enum` arm now
    # precedes its `str` arm, so a str-enum encodes on the wire as an
    # `EnumVariant` (T binds to the enum type `SomeEnum`, not `string`) and the
    # value decodes back to the enum member — matching the bex layer, where a
    # `Variant` actual is unambiguously an enum. The `isinstance` check is
    # load-bearing: a bare `string` round-trip (the old behavior) would still pass
    # `== SomeEnum.VARIANT` via str-enum equality, so only the type assertion
    # proves the value came back as a real enum member rather than its string value.
    result = identity(SomeEnum.VARIANT)
    assert result == SomeEnum.VARIANT
    assert isinstance(result, SomeEnum)


# ===========================================================================
# §F — host-only object boundary (RustType round-trip lives at the bex layer).
# ===========================================================================


def test_generic_inference_host_only_object_not_encodable_from_python():
    # §F (host boundary): the §F RustType round-trip (an arbitrary host object
    # riding opaquely) is reachable at the bex/value layer, but the Python bridge
    # only encodes primitives, lists, maps, callables, and Pydantic models — an
    # arbitrary Python object has no wire encoding and is rejected at encode time
    # with a TypeError BEFORE the call reaches the engine. This pins the SDK-side
    # boundary that makes F1–F3 a bex-only concern.
    class HostThing:
        def __init__(self, n: int) -> None:
            self.n = n

    with pytest.raises(TypeError) as excinfo:
        identity(HostThing(3))
    assert "Cannot encode" in str(excinfo.value)


# ===========================================================================
# §J J13 — a function-typed (host callable) argument poisons its TypeVars: they
# must be specified up front (the bridge can't infer from / validate against an
# opaque handle), even though `x` would otherwise pin `T`.
# ===========================================================================


def test_generic_inference_apply_closure_poisons_typevars_must_specify():
    # J13: `apply(lambda v: v + 1, 5)` — `T` is poisoned by its occurrence in the
    # closure parameter `(T)` (even though `x=5` would pin it) and `R` lives only
    # in the closure's return, so both must be specified; bare ⇒ rejected as a
    # Python `TypeError` complaining that a type parameter couldn't be inferred.
    _assert_type_error(
        lambda: apply(lambda v: v + 1, 5),
        "could not infer a type",
        "apply",
    )


def test_generic_inference_apply_closure_typevars_specified_succeeds():
    # J13 (positive): once `T` and `R` are specified, the call goes through and the
    # callable is invoked ⇒ apply(lambda v: v + 1, 5) == 6.
    assert apply(lambda v: v + 1, 5, _types={"T": int, "R": int}) == 6


# ===========================================================================
# §L — methods: class T from the receiver, method vars from method args.
# ===========================================================================


def test_generic_inference_genericbox_get_infers_class_var_from_receiver():
    # L1: GenericBox[int](value=5).get() == "int" — class T recovered from the
    # receiver's wire type-args (no method var to infer).
    assert GenericBox[int](value=5).get() == "int"


def test_generic_inference_genericbox_pair_with_unbound_receiver_recovers_class_var():
    # L5: a BARE method call on an UNBOUND receiver `GenericBox(value=5)` (no
    # `[int]`) sends empty class type-args, but the method's `self: GenericBox<T>`
    # formal forces recursion into the `value` field (the G1 path) ⇒ class T=int
    # recovered from `value=5`, unioned with method var U=string ⇒ "int | string".
    # (The bare path is supported precisely because no method param is explicitly
    # bound; the `_types=`/subscript form on an unparameterized receiver raises —
    # see test_generic_calls.test_instance_method_unparameterized_receiver_raises.)
    assert GenericBox(value=5).pair_with("x") == "int | string"


# ===========================================================================
# §C C4 — a caller-specified binding contradicted by the actual value rejects at
# the engine (Gate B), on BOTH host surfaces: the `_types=` kwarg and the `[...]`
# subscript (which is pure sugar over `_types=` and adds no Python-side value
# validation of its own — see `_GenericCallable.__getitem__`). Both reach the
# same engine check and reject identically.
# ===========================================================================


def test_generic_inference_make_triple_types_kwarg_contradicted_by_actual_rejects():
    # C4 (`_types=` surface): a partial `_types={"A": int}` fixes A=int, but
    # `a="nope"` is a string. Inference is bypassed for the caller-specified A, so
    # the engine's per-arg structural check (Gate B) is the only gate — and it now
    # rejects the contradicting scalar at CALL time as a `TypeMismatch` (Python
    # `TypeError`), naming the function. (Previously this seam skipped every
    # non-instance arg, so the call ran and the mismatch only surfaced later at
    # DECODE time as a Pydantic ValidationError when re-validating the returned
    # value.) Only `_types=` can bind a *partial* set of vars — the subscript
    # requires full arity (C1).
    _assert_type_error(
        lambda: make_triple("nope", ["x"], {"k": True}, _types={"A": int}),
        "make_triple",
    )


def test_generic_inference_make_triple_full_subscript_contradicted_by_actual_rejects():
    # C4 (subscript surface): `make_triple[int, str, bool]("nope", ...)` seeds every
    # var via the subscript, which is pure sugar for `_types={"A": int, "B": str,
    # "C": bool}` (`__getitem__` → `functools.partial(..., _types=bound)` → the same
    # call path → bex). The subscript adds NO value validation of its own — it only
    # checks type-arg *arity* (C1) — so the `a="nope"` string vs the now-concrete
    # `int` formal is caught at the SAME engine Gate B as the `_types=` surface,
    # surfacing as a `TypeError` naming the function. Pins that the subscript path
    # delegates rather than re-validating, and that both surfaces reject identically.
    _assert_type_error(
        lambda: make_triple[int, str, bool]("nope", ["x"], {"k": True}),
        "make_triple",
    )


# ===========================================================================
# §B/§D — heterogeneous array unification: the elements of one T[] union-merge
# into the element type, so inference over a mixed array yields a union.
# ===========================================================================


def test_generic_inference_elem_type_heterogeneous_array_unifies():
    # The mixed elements of a single `T[]` union-merge while synthesizing the
    # container's element type ⇒ elem_type([1, "x"]) binds T = int | string.
    # Directly asserts the unified element type (B8 only reads back the values).
    assert elem_type([1, "x"]) == "int | string"


def test_generic_inference_elem_type_homogeneous_array_is_single_type():
    # The degenerate case: a homogeneous array dedups to a single type.
    assert elem_type([1, 2, 3]) == "int"


def test_generic_inference_elem_type_three_way_heterogeneous_array_unifies():
    # n-ary element union: three distinct element types all merge.
    rendered = elem_type([1, "x", True])
    assert "int" in rendered and "string" in rendered and "bool" in rendered


# ===========================================================================
# §G generalized — an UNBOUND generic instance (constructed WITHOUT type args)
# is still inferrable when the formal forces recursion into its fields.
#
# Normally an unbound generic instance carries no wire type-args and rides as
# host-only `rust_type` (G2). But when the parameter's formal is itself
# `Container<T>` / `Recursive<T>` / nested `Pair<...>`, inference is DIRECTED
# into the corresponding field values and recovers `T` from them (G1) — so a
# Python caller who forgot the `[int]` subscript still gets a working call.
# ===========================================================================


def test_generic_inference_read_items_unbound_container_recovers_t_from_fields():
    # ContainerShapes constructed WITHOUT `[int]`: no wire type-args, but the
    # `read_items(shape: ContainerShapes<T>)` formal forces recursion into the
    # fields ⇒ T=int recovered from the field VALUES; returns `items`.
    unbound = ContainerShapes(
        item=1, items=[1, 2, 3], by_key={"k": 4}, maybe=None, mixed=None
    )
    assert read_items(unbound) == [1, 2, 3]


def test_generic_inference_list_head_unbound_recursive_recovers_t_from_fields():
    # GenericRecursive constructed WITHOUT `[int]`: the `list_head(list:
    # GenericRecursive<T>)` formal forces recursion into `value`/`next` ⇒ T=int
    # recovered from the field values even though the wire carries no type-args.
    unbound = GenericRecursive(value=7, next=GenericRecursive(value=8, next=None))
    assert list_head(unbound) == 7


def test_generic_inference_extract_fully_unbound_nested_pair_recovers_all_vars():
    # Nested GenericPair with NO `[...]` subscripts at ANY level — every instance is
    # unbound. The `extract(a: GenericPair<GenericPair<A,B>, GenericPair<C,D>>)`
    # formal drives recursion all the way down: the engine reconstructs each nested
    # unbound instance against its slot's formal (deep G1), recovering A,B,C,D from
    # the leaf field values. So a caller who forgot every subscript still gets a
    # working call.
    fully_unbound = GenericPair(
        first=GenericPair(first=1, second="a"),
        second=GenericPair(first=True, second=1.5),
    )
    assert extract(fully_unbound) == "int | string | bool | float"


# ===========================================================================
# §D concrete-type join — the covariant union-merge also handles non-primitive
# actuals: a concrete BAML class and an enum participate in the join.
# ===========================================================================


def test_generic_inference_triple_choose_join_includes_concrete_class():
    # triple_choose(int, StringIntPair, string) — the covariant join merges a
    # primitive, a concrete BAML class, and a primitive ⇒ T includes StringIntPair.
    rendered = triple_choose(5, StringIntPair(my_string="a", my_int=1), "x")
    assert "int" in rendered and "StringIntPair" in rendered and "string" in rendered


def test_generic_inference_triple_choose_join_includes_enum_variant():
    # triple_choose(int, SomeEnum, StringIntPair) — the covariant join merges a
    # primitive, an enum, and a concrete class. proto.py now encodes a str-enum as
    # an `EnumVariant` (see test_identity_enum_round_trips), so the enum actual
    # rides as the enum type `SomeEnum` and the join is the full
    # `T = int | SomeEnum | StringIntPair`.
    rendered = triple_choose(
        5, SomeEnum.VARIANT, StringIntPair(my_string="a", my_int=1)
    )
    assert (
        "int" in rendered and "SomeEnum" in rendered and "StringIntPair" in rendered
    )
