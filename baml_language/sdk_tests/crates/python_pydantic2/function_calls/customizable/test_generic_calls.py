"""Generic *function-call* coverage (ns_generic_tests).

Pins the host->engine call path for generic functions/methods whose TypeVars
must be bound from the call. Cases use the *explicit* variant: the caller
specifies the bindings — `_types=` for a function's / method's own TypeVars,
and a parameterized `GenericBox[int]` receiver for a generic class's TypeVars
(recovered host-side from the Pydantic generic metadata). This is the path the
bridge implements, so these pass — except `tag_or_value`, whose baml body is
still a null-returning stub.

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
)

# xfail reason for the variant that can't produce the right answer yet.
TAG_OR_VALUE = (
    "bridge-generics2: `tag_or_value` must discriminate whether `x` bound to "
    "`T` vs. the string/null arms, which needs a `let v: T` match arm (rejected "
    "today as an unreachable arm; see baml-runtime-type-tests-limits). Its baml "
    "body is a null-returning stub, so it fails — even `_types=` binds "
    "`T` but the stub still returns null."
)


# ===========================================================================
# basic cases, free functions
# ===========================================================================


# --- identity<T>(x: T) -> T : single TypeVar, infer from one argument -------


def test_identity_explicit():
    assert identity(5, _types=int) == 5
    assert identity("hi", _types=str) == "hi"
    pair = StringIntPair(my_string="a", my_int=1)
    assert identity(pair, _types=StringIntPair) == pair


async def test_identity_async_explicit():
    from baml_sdk.generic_tests import identity_async

    assert await identity_async(7, _types=int) == 7


# --- tag_or_value<T>(x: T | string | null) -> T? : TypeVar in a union -------


@pytest.mark.xfail(reason=TAG_OR_VALUE, strict=True)
def test_tag_or_value_explicit():
    pair = StringIntPair(my_string="b", my_int=2)
    assert tag_or_value(pair, _types=StringIntPair) == pair
    assert tag_or_value("plain", _types=StringIntPair) is None
    assert tag_or_value(None, _types=StringIntPair) is None


# --- make_triple<A, B, C>(...) -> GenericTriple<A, B, C> : multiple TypeVars -


def test_make_triple_explicit():
    # A=int, B=str, C=bool, in declaration order (positional tuple form).
    t = make_triple(1, ["a", "b"], {"k": True}, _types=(int, str, bool))
    assert isinstance(t, GenericTriple)
    assert t.first == 1
    assert t.second == ["a", "b"]
    assert t.third == {"k": True}


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
    assert b.pair_with("hello world", _types=str) == "int | string"


# --- extract<A, B, C, D>(a: GenericPair<GenericPair<A,B>, GenericPair<C,D>>) --
# Nested generic. Binding-sensitive: body is `reflect.type_of<A|B|C|D>()`.


def _nested_pair():
    return GenericPair[GenericPair[int, str], GenericPair[bool, float]](
        first=GenericPair[int, str](first=1, second="a"),
        second=GenericPair[bool, float](first=True, second=1.5),
    )


def test_extract_explicit():
    assert extract(_nested_pair(), _types=(int, str, bool, float)) == (
        "int | string | bool | float"
    )


# ===========================================================================
# basic case, `T` in return position only
# ===========================================================================


# --- parse_as<T>(source: string) -> T : return-position-only TypeVar --------


def test_parse_as_explicit():
    # `T` bound by the host via `_types=` (Python surface for `$types`).
    pair = parse_as('{"my_string": "x", "my_int": 3}', _types=StringIntPair)
    assert pair == StringIntPair(my_string="x", my_int=3)
    assert parse_as("42", _types=int) == 42


# ===========================================================================
# complex cases
# ===========================================================================


# --- second_of<T>(p: GenericPair<int, T>) -> T : partially-bound class param -


def test_second_of_explicit():
    assert second_of(GenericPair[int, str](first=1, second="hi"), _types=str) == "hi"
    pair = StringIntPair(my_string="z", my_int=9)
    p = GenericPair[int, StringIntPair](first=0, second=pair)
    assert second_of(p, _types=StringIntPair) == pair


# --- list_head<T>(list: GenericRecursive<T>) -> T : recursive generic arg ----


def _recursive_list():
    return GenericRecursive[int](value=7, next=GenericRecursive[int](value=8, next=None))


def test_list_head_explicit():
    assert list_head(_recursive_list(), _types=int) == 7


# --- choose<T>(left: T, right: T) -> T : unification across two args ---------


def test_choose_explicit():
    assert choose(1, 2, _types=int) == 1
    assert choose("a", "b", _types=str) == "a"


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
    assert read_items(_container_shape(), _types=int) == [1, 2, 3]


# ===========================================================================
# outbound generics
# ===========================================================================


# --- wrap<T>(x: T) -> GenericBox<T> : infer `T`, return a generic over it ----


def test_wrap_explicit():
    w = wrap(5, _types=int)
    assert isinstance(w, GenericBox)
    assert w.value == 5
