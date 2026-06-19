"""Generic *function-call* coverage (ns_generic_tests).

Pins the host->engine call path for generic functions/methods whose TypeVars
must be bound from the call. Cases use the *explicit* variant: the caller
specifies the bindings — `_types=` for a function's / method's own TypeVars,
and a parameterized `GenericBox[int]` receiver for a generic class's TypeVars
(recovered host-side from the Pydantic generic metadata). This is the path the
bridge implements, so these pass.

(The *inference* variant — bare calls with no caller-specified bindings, which
depend on the not-yet-landed inbound-inference phase — is intentionally not
covered here yet.)
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
    consume_int_wrapper,
    one_type_arg,
    two_type_args,
)


# ===========================================================================
# basic cases, free functions
# ===========================================================================


# --- identity<T>(x: T) -> T : single TypeVar, infer from one argument -------


def test_identity_explicit():
    assert identity(5, _types={"T": int}) == 5
    assert identity("hi", _types={"T": str}) == "hi"
    pair = StringIntPair(my_string="a", my_int=1)
    assert identity(pair, _types={"T": StringIntPair}) == pair


async def test_identity_async_explicit():
    from baml_sdk.generic_tests import identity_async

    assert await identity_async(7, _types={"T": int}) == 7


# --- tag_or_value<T>(x: T | string | null) -> T? : TypeVar in a union -------


def test_tag_or_value_explicit():
    # `tag_or_value` reflects its bound `T` back as a string; `x` must inhabit
    # the substituted `T | string | null`. Proves `T` is bound from `_types=`.
    assert tag_or_value(5, _types={"T": int}) == "int"
    assert tag_or_value("plain", _types={"T": str}) == "string"
    pair = StringIntPair(my_string="b", my_int=2)
    assert "StringIntPair" in tag_or_value(pair, _types={"T": StringIntPair})


# --- make_triple<A, B, C>(...) -> GenericTriple<A, B, C> : multiple TypeVars -


def test_make_triple_explicit():
    # A=int, B=str, C=bool, bound by name via the `_types=` dict.
    t = make_triple(1, ["a", "b"], {"k": True}, _types={"A": int, "B": str, "C": bool})
    assert isinstance(t, GenericTriple)
    assert t.first == 1
    assert t.second == ["a", "b"]
    assert t.third == {"k": True}


# --- one_type_arg<T>() / two_type_args<A,B>() : return-position-only TypeVars -
# No argument carries `T`; the binding can only come from `_types=`. The
# cleanest proof the inbound path does not rely on argument inference.


def test_one_type_arg_explicit():
    assert one_type_arg(_types={"T": int}) == "int"
    assert one_type_arg(_types={"T": str}) == "string"
    # Nested generic binding must encode fully (base class + concrete arg).
    nested = one_type_arg(_types={"T": GenericBox[int]})
    assert "GenericBox" in nested and "int" in nested


def test_two_type_args_explicit():
    assert two_type_args(_types={"A": int, "B": str}) == "int | string"


def test_generic_free_fn_requires_types():
    # `_types=` is required for a generic free function: omitting it (or
    # binding only some params) is a hard error, not silent inference.
    with pytest.raises(TypeError) as exc:
        one_type_arg()
    assert str(exc.value) == (
        "_types= is required for this generic call: bind every type parameter "
        "in ['T'] with a dict, e.g. _types={'T': int}"
    )
    with pytest.raises(TypeError) as exc:
        two_type_args(_types={"A": int})  # missing B
    assert str(exc.value) == (
        "_types= is missing binding(s) for ['B']: every type parameter in "
        "['A', 'B'] must be bound."
    )


def test_generic_free_fn_rejects_non_dict_types():
    # The dict is the only accepted `_types=` shape — the legacy single-type
    # and positional tuple/list forms are gone.
    with pytest.raises(TypeError) as exc:
        one_type_arg(_types=int)
    assert str(exc.value) == (
        "_types= must be a dict mapping type-parameter names to types "
        "(e.g. _types={'T': int}); got type. The single-type and positional "
        "tuple/list forms are no longer accepted."
    )
    with pytest.raises(TypeError) as exc:
        make_triple(1, ["a"], {"k": True}, _types=(int, str, bool))
    assert str(exc.value) == (
        "_types= must be a dict mapping type-parameter names to types "
        "(e.g. _types={'A': int}); got tuple. The single-type and positional "
        "tuple/list forms are no longer accepted."
    )


# ===========================================================================
# basic cases, generic classes
# ===========================================================================


# --- consume_int_wrapper(x: GenericBox<int>) -> int : fully-bound TypeVar ----


def test_consume_int_wrapper_baseline():
    # No binding of any kind: a concretely-instantiated `GenericBox<int>` flows
    # in and the `int` field flows back out. Anchors the suite — if this breaks,
    # the generic *class* boundary regressed independent of TypeVar binding.
    assert consume_int_wrapper(GenericBox[int](value=9)) == 9


# --- GenericBox<T>.get(self) -> string : class TypeVar from the receiver -----
# Binding-sensitive: `get` is `reflect.type_of<T>()`.


def test_genericbox_get_explicit():
    # `GenericBox[int](...)` carries the type arg; the host recovers it from the
    # receiver and seeds it as the method frame's class-level `T`.
    b = GenericBox[int](value=5)
    assert b.get() == "int"


# --- GenericBox<T>.pair_with<U>(self, other: U) -> string : class T + method U


def test_genericbox_pair_with_explicit():
    # `T` from `GenericBox[int]` (receiver), `U` from `_types=str` (method var).
    b = GenericBox[int](value=5)
    assert b.pair_with("hello world", _types={"U": str}) == "int | string"


# --- GenericBox<T>.new<T>(value: T) -> GenericBox<T> : generic STATIC method ---
# No receiver, so only the static's own `T` is bound (via `_types=`); no class
# type args ride along.


def test_genericbox_new_static_explicit():
    box = GenericBox.new(value=5, _types={"V": int})
    assert isinstance(box, GenericBox)
    assert box.value == 5


def test_generic_static_requires_types():
    # A generic static method requires `_types=` (no receiver to recover from).
    with pytest.raises(TypeError) as exc:
        GenericBox.new(value=5)
    assert str(exc.value) == (
        "_types= is required for this generic call: bind every type parameter "
        "in ['V'] with a dict, e.g. _types={'V': int}"
    )


# --- NamedStatic<A,B,C>.make<D,E> : static TypeVar names DIFFER from the class -
# Proves the named `TyArg` wire slots each binding by TypeVar *name* into the
# static frame's own params (`[D, E]`) — no phantom class params. This is the
# case the named wire exists for (01pt5).


def test_named_static_distinct_typevar_names():
    assert NamedStatic.make(1, "x", _types={"D": int, "E": str}) == "int | string"


# --- Negative: an instance method needing class args on an UN-parameterized
# receiver must raise (the class TypeVars can't be recovered).


def test_instance_method_unparameterized_receiver_raises():
    # `GenericBox(value=5)` (no `[int]`) carries no concrete class type args, so
    # `pair_with`'s class `T` can't be recovered host-side.
    with pytest.raises(TypeError) as exc:
        GenericBox(value=5).pair_with("x", _types={"U": str})
    assert str(exc.value) == (
        "_types= on a generic method requires a Pydantic generic receiver so "
        "the class type args can be recovered"
    )


# ===========================================================================
# Phase 6: ergonomic subscript syntax — `fn[T1, T2](...)` sugar for
# `fn(..., _types={...})`. Pure front-end desugaring; identical behavior.
# ===========================================================================


def test_subscript_free_function_single():
    assert one_type_arg[int]() == "int"
    assert one_type_arg[str]() == "string"
    nested = one_type_arg[GenericBox[int]]()
    assert "GenericBox" in nested and "int" in nested


def test_subscript_free_function_multiple():
    assert two_type_args[int, str]() == "int | string"


def test_subscript_equivalent_to_types_kwarg():
    # Subscript is pure sugar: same result as the explicit `_types=` form.
    assert identity[int](5) == identity(5, _types={"T": int})
    assert two_type_args[int, str]() == two_type_args(_types={"A": int, "B": str})


def test_subscript_wrong_arity_raises():
    with pytest.raises(TypeError) as exc:
        two_type_args[int]()  # needs two type args
    assert str(exc.value) == "expected 2 type argument(s) for ['A', 'B'], got 1"


def test_subscript_static_method():
    box = GenericBox.new[int](value=5)
    assert isinstance(box, GenericBox)
    assert box.value == 5


def test_subscript_instance_method():
    b = GenericBox[int](value=5)
    assert b.pair_with[str]("hello world") == "int | string"


def test_subscript_named_static_distinct_names():
    assert NamedStatic.make[int, str](1, "x") == "int | string"


# --- extract<A, B, C, D>(a: GenericPair<GenericPair<A,B>, GenericPair<C,D>>) --
# Nested generic. Binding-sensitive: body is `reflect.type_of<A|B|C|D>()`.


def _nested_pair():
    return GenericPair[GenericPair[int, str], GenericPair[bool, float]](
        first=GenericPair[int, str](first=1, second="a"),
        second=GenericPair[bool, float](first=True, second=1.5),
    )


def test_extract_explicit():
    assert extract(_nested_pair(), _types={"A": int, "B": str, "C": bool, "D": float}) == (
        "int | string | bool | float"
    )


# ===========================================================================
# basic case, `T` in return position only
# ===========================================================================


# --- parse_as<T>(source: string) -> T : return-position-only TypeVar --------


def test_parse_as_explicit():
    # `T` bound by the host via `_types=` (Python surface for `$types`).
    pair = parse_as('{"my_string": "x", "my_int": 3}', _types={"T": StringIntPair})
    assert pair == StringIntPair(my_string="x", my_int=3)
    assert parse_as("42", _types={"T": int}) == 42


# ===========================================================================
# complex cases
# ===========================================================================


# --- second_of<T>(p: GenericPair<int, T>) -> T : partially-bound class param -


def test_second_of_explicit():
    assert second_of(GenericPair[int, str](first=1, second="hi"), _types={"T": str}) == "hi"
    pair = StringIntPair(my_string="z", my_int=9)
    p = GenericPair[int, StringIntPair](first=0, second=pair)
    assert second_of(p, _types={"T": StringIntPair}) == pair


# --- list_head<T>(list: GenericRecursive<T>) -> T : recursive generic arg ----


def _recursive_list():
    return GenericRecursive[int](value=7, next=GenericRecursive[int](value=8, next=None))


def test_list_head_explicit():
    assert list_head(_recursive_list(), _types={"T": int}) == 7


# --- choose<T>(left: T, right: T) -> T : unification across two args ---------


def test_choose_explicit():
    assert choose(1, 2, _types={"T": int}) == 1
    assert choose("a", "b", _types={"T": str}) == "a"


# --- read_items<T>(shape: ContainerShapes<T>) -> T[] : one T, many fields ----


def _container_shape():
    return ContainerShapes[int](
        item=1,
        items=[1, 2, 3],
        by_key={"k": 4},
        maybe=None,
        mixed=None,
    )


def test_read_items_explicit():
    assert read_items(_container_shape(), _types={"T": int}) == [1, 2, 3]


# ===========================================================================
# outbound generics
# ===========================================================================


# --- wrap<T>(x: T) -> GenericBox<T> : infer `T`, return a generic over it ----


def test_wrap_explicit():
    w = wrap(5, _types={"T": int})
    assert isinstance(w, GenericBox)
    assert w.value == 5
