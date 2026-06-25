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
from baml_sdk.baml import BamlPanic
from baml_sdk.generic_tests import (
    StringIntPair,
    GenericPair,
    GenericTriple,
    GenericBox,
    GenericRecursive,
    ContainerShapes,
    NamedStatic,
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
    pair,
    merge,
    combine,
    glue,
    triple_choose,
    two_in_union,
)


# ===========================================================================
# §A — single TypeVar inferred from one argument value
# ===========================================================================


def test_identity_infers_primitives():
    # T1/T2: T bound from the value; identity returns it unchanged.
    assert identity(5) == 5
    assert identity("hi") == "hi"
    assert identity(True) is True


def test_identity_infers_user_class():
    # T3: T = StringIntPair, recovered from the instance value.
    pair = StringIntPair(my_string="a", my_int=1)
    assert identity(pair) == pair


def test_identity_infers_generic_instance():
    # T4: a fully-bound GenericBox[int] carries its [int] on the wire, so T is
    # recovered as GenericBox<int> with no caller binding.
    box = GenericBox[int](value=5)
    assert identity(box) == box

    nested = GenericBox[GenericBox[str]](value=GenericBox[str](value="hello"))
    assert identity(nested) == nested


async def test_identity_async_infers():
    # T5: the async path infers identically.
    from baml_sdk.generic_tests import identity_async

    assert await identity_async(7) == 7


def test_identity_null_round_trips():
    # §I I4 (decided): a `null` actual is no inference evidence (NOT bound as
    # `T=null`) ⇒ `T` defaults to host-only `rust_type`, and the value round-trips
    # unchanged.
    assert identity(None) is None


def test_identity_unbound_generic_instance_round_trips():
    # §G G2 (decided): an UNBOUND generic instance — constructed without the
    # `[int]` subscript — carries no wire type-args, so it is host-only
    # (`T=rust_type`) and rides through the VM opaquely, round-tripping unchanged
    # (and staying distinct from a properly-bound `GenericBox[int]`, G4).
    unbound = GenericBox(value=5)
    assert identity(unbound) == unbound


# ===========================================================================
# §B — structural / container solving across one or more arguments
# ===========================================================================


def test_make_triple_infers_multiple_typevars():
    # T6: A=int (scalar), B=string (list element), C=bool (map value) — all three
    # inferred from differently-shaped arguments at once.
    t = make_triple(1, ["a", "b"], {"k": True})
    assert isinstance(t, GenericTriple)
    assert t.first == 1
    assert t.second == ["a", "b"]
    assert t.third == {"k": True}


def test_second_of_infers_from_nested_generic():
    # T9: second_of<T>(p: GenericPair<int, T>) — T binds from the instance's 2nd
    # wire arg only (`first` is pinned to int in the signature).
    assert second_of(GenericPair[int, str](first=1, second="hi")) == "hi"
    pair = StringIntPair(my_string="z", my_int=9)
    p = GenericPair[int, StringIntPair](first=0, second=pair)
    assert second_of(p) == pair


def test_read_items_infers_from_instance_wire_args():
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


def test_list_head_infers_from_recursive_generic():
    # T11: GenericRecursive<T> bottoms out at next=None; T binds from the wire arg.
    linked = GenericRecursive[int](
        value=7, next=GenericRecursive[int](value=8, next=None)
    )
    assert list_head(linked) == 7


def test_extract_infers_four_typevars_from_nesting():
    # T12: A,B,C,D recovered by walking the nested GenericPair instantiation.
    pair = GenericPair[GenericPair[int, str], GenericPair[bool, float]](
        first=GenericPair[int, str](first=1, second="a"),
        second=GenericPair[bool, float](first=True, second=1.5),
    )
    assert extract(pair) == "int | string | bool | float"


# ===========================================================================
# §C — union unification: one TypeVar across two argument positions
# ===========================================================================


def test_choose_infers_unified_typevar():
    # T14: choose(5, 6) ⇒ T = int (the two bindings merge to one). Body returns
    # `left`, so the call returns 5.
    assert choose(5, 6) == 5
    assert choose("a", "b") == "a"


def test_choose_infers_divergent_union():
    # T15: choose(5, "asdf") ⇒ T = int | string (a capability inference unlocks
    # over the explicit form, which forces a single T). Returns `left` = 5.
    assert choose(5, "asdf") == 5


# ===========================================================================
# §D — partial binding: explicit seed for one TypeVar, infer the rest
# ===========================================================================


def test_make_triple_partial_explicit_then_infer():
    # T17: bind A explicitly via a partial _types= dict; B and C are inferred.
    t = make_triple(1, ["x", "y"], {"k": True}, _types={"A": int})
    assert isinstance(t, GenericTriple)
    assert t.first == 1
    assert t.second == ["x", "y"]
    assert t.third == {"k": True}


# ===========================================================================
# §G/outbound — infer T, return a generic over it
# ===========================================================================


def test_wrap_infers_and_returns_generic():
    # T29: wrap(5) infers T=int and returns a GenericBox<int>.
    w = wrap(5)
    assert isinstance(w, GenericBox)
    assert w.value == 5


# ===========================================================================
# §K — methods: class T from the receiver, method TypeVars inferred from args
# ===========================================================================


def test_genericbox_pair_with_infers_method_typevar():
    # T37: class T=int from the GenericBox[int] receiver; method U=string inferred
    # from the bare `other` arg (no [str] subscript).
    b = GenericBox[int](value=5)
    assert b.pair_with("hello world") == "int | string"


def test_generic_static_infers_own_typevar():
    # T38: GenericBox.new<V>(value: V) — V inferred from the value, no subscript.
    box = GenericBox.new(value=5)
    assert isinstance(box, GenericBox)
    assert box.value == 5


def test_named_static_infers_distinct_typevars():
    # T39: NamedStatic.make<D,E>(d, e) — D=int, E=string inferred from the args.
    assert NamedStatic.make(1, "x") == "int | string"


# ===========================================================================
# Out-of-scope / must-specify: inference finds no evidence ⇒ engine rejects
# ===========================================================================


def test_union_with_concrete_sibling_infers_typevar():
    # 02a reverses 00b3 G5/§H: a TypeVar buried in a union beside concrete
    # members (`x: T | string | null`) is NOW solved by inference. The `int`
    # actual is not absorbed by the `string`/`null` siblings, so it routes to
    # `T` ⇒ T=int, matching the explicit form `tag_or_value[int](5) == "int"`.
    assert tag_or_value(5) == "int"


def test_union_concrete_sibling_absorbs_value_binds_rust_type():
    # §H H3 (decided): a `string` actual IS absorbed by the concrete `string`
    # sibling, so nothing routes to `T`. `T` still has a value position (the `x`
    # param) and no closure occurrence, so it defaults to host-only `rust_type`
    # (rule 4) rather than being rejected.
    assert tag_or_value("hi") == "$rust_type"


def test_union_null_actual_binds_rust_type():
    # §H H3 / §I I4 (decided): a `null` actual is no inference evidence (not bound
    # as `T=null`), and the `null` sibling absorbs it, so `T` defaults to
    # `rust_type`.
    assert tag_or_value(None) == "$rust_type"


def test_return_only_var_still_requires_binding():
    # §E: parse_as<T>(source: string) -> T — T appears only in return position,
    # so no argument can carry it. Inference finds nothing ⇒ engine rejects.
    with pytest.raises(BamlPanic):
        parse_as("42")


def test_body_only_var_still_requires_binding():
    # §E: one_type_arg<T>() reflects T but takes no argument ⇒ uninferable.
    with pytest.raises(BamlPanic):
        one_type_arg()


# ===========================================================================
# §J — variance soundness (02d/02e): conflicting occurrences of one TypeVar
# across invariant/covariant positions have no consistent binding ⇒ REJECT,
# instead of fabricating an unsound union. Agreeing occurrences still bind.
# ===========================================================================


def test_pair_invariant_list_conflict_rejects():
    # J4/E1: pair(int[], string[]) ⇒ a⇒T==int, b⇒T==string (both invariant list
    # elements) ⇒ no consistent T ⇒ reject (the old unifier fabricated
    # `(int|string)[]`).
    with pytest.raises(BamlPanic):
        pair([1, 2], ["a", "b"])


def test_pair_invariant_list_agree_binds():
    # J9/G1: pair(int[], int[]) ⇒ two invariant occurrences that AGREE ⇒ T = int.
    # The fix narrows behavior, so this must still succeed.
    assert pair([1, 2], [3, 4]) == "int"


def test_choose_union_outside_container_is_sound():
    # J10/G2: choose(int[], string[]) — both occurrences are covariant (bare `T`),
    # so the union forms OUTSIDE the container (T = int[] | string[]) and the call
    # SUCCEEDS, returning `left`. Proves the fix keys on position variance, not
    # "arrays are involved." (Contrast pair, where T is under the container.)
    assert choose([1, 2], ["a"]) == [1, 2]


def test_merge_invariant_map_value_conflict_rejects():
    # J5/E2: merge(map<string,int>, map<string,string>) ⇒ conflicting invariant
    # map-value type ⇒ reject.
    with pytest.raises(BamlPanic):
        merge({"k": 1}, {"k": "a"})


def test_combine_invariant_class_arg_conflict_rejects():
    # J6/E3: combine(GenericBox[int], GenericBox[string]) ⇒ Box<T> invariant,
    # int ≠ string ⇒ reject.
    with pytest.raises(BamlPanic):
        combine(GenericBox[int](value=1), GenericBox[str](value="x"))


def test_glue_invariant_vs_covariant_conflict_rejects():
    # J7/E4: glue(int, string[]) ⇒ arr⇒T==string (invariant) but bare⇒int <: T
    # (covariant); int <: string is false ⇒ reject (the key cross-variance case).
    with pytest.raises(BamlPanic):
        glue(1, ["a"])


def test_glue_invariant_and_covariant_agree_binds():
    # J11/G4: glue(int, int[]) ⇒ invariant (T==int) + covariant (int <: int)
    # AGREE ⇒ T = int; must still succeed.
    assert glue(1, [2, 3]) == "int"


def test_two_typevar_union_is_uninferrable_rejects():
    # J12: two_in_union<T,U>(x: T | U | int) ⇒ two free vars in one union have no
    # principled split without an explicit hint ⇒ reject (distinct from §H, which
    # is ONE var beside concrete members).
    with pytest.raises(BamlPanic):
        two_in_union("hello")


# ===========================================================================
# §D — n-ary covariant join, and §B heterogeneous container element
# ===========================================================================


def test_triple_choose_three_covariant_join():
    # D3: triple_choose(5, "asdf", True) ⇒ T = int | string | bool — three
    # covariant bare-arg occurrences union-merge (n-ary, not pairwise).
    assert triple_choose(5, "asdf", True) == "int | string | bool"


def test_make_triple_heterogeneous_list_element_unions():
    # B8: make_triple(1, [1, "x"], {"k": True}) ⇒ B = int | string — the list's
    # mixed elements union-merge while synthesizing ONE container's element type
    # (the §D join applied INSIDE a container; distinct from §J's invariant
    # conflict between two separate args). The heterogeneous list round-trips.
    t = make_triple(1, [1, "x"], {"k": True})
    assert isinstance(t, GenericTriple)
    assert t.second == [1, "x"]


def test_choose_divergent_generic_instances_union():
    # D2: choose(GenericBox[int], GenericBox[str]) ⇒ T = GenericBox<int> |
    # GenericBox<string>, the union OUTSIDE the box (both occurrences covariant).
    # Body returns `left`, so the int box comes back. Contrast `combine`, where T
    # is INSIDE the box and the same actuals conflict (§J).
    left = GenericBox[int](value=1)
    assert choose(left, GenericBox[str](value="x")) == left


def test_tag_or_value_binds_generic_instance():
    # H2: tag_or_value(GenericBox[str]) ⇒ the instance is not absorbed by the
    # `string`/`null` siblings, so it routes to T ⇒ T = GenericBox<string>.
    rendered = tag_or_value(GenericBox[str](value="asdf"))
    assert "GenericBox" in rendered and "string" in rendered
