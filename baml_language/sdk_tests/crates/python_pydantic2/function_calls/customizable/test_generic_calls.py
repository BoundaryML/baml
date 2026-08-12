"""Generic *function-call* coverage (ns_generic_tests).

Pins the host->engine call path for generic functions/methods whose TypeVars
must be bound from the call. Cases use the *explicit subscript* form: the caller
binds each TypeVar positionally via `fn[T1, T2](...)` (the callee's own generic
params, in declaration order), and a parameterized `GenericBox[int]` receiver
supplies a generic class's TypeVars (recovered host-side from the Pydantic
generic metadata). `fn[...]` is sugar over the engine's named binding wire; no
test passes `_types=` directly.

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
    make_int_box,
    make_int_container,
    make_nested_box,
    make_int_str_bool_triple,
)


# ===========================================================================
# basic cases, free functions
# ===========================================================================


# --- identity<T>(x: T) -> T : single TypeVar --------------------------------


def test_generic_calls_identity_explicit():
    assert identity[int](5) == 5
    assert identity[str]("hi") == "hi"

    pair = StringIntPair(my_string="a", my_int=1)
    assert identity[StringIntPair](pair) == pair

    box = GenericBox[GenericBox[str]](value=GenericBox[str](value="hello"))
    assert identity[GenericBox[GenericBox[str]]](box) == box

    triple = GenericTriple[GenericBox[str], float, bool](
        first=GenericBox[str](value="hello"),
        second=[1.1, 2.2],
        third={"lorem": True, "ipsum": False},
    )
    assert identity[GenericTriple[GenericBox[str], float, bool]](triple) == triple


async def test_generic_calls_identity_async_explicit():
    from baml_sdk.generic_tests import identity_async

    assert await identity_async[int](7) == 7


# --- tag_or_value<T>(x: T | string | null) -> string : TypeVar in a union ---


def test_generic_calls_tag_or_value_explicit():
    # `tag_or_value` reflects its bound `T` back as a string; `x` must inhabit
    # the substituted `T | string | null`. Proves `T` is bound from the
    # subscript.
    assert tag_or_value[int](5) == "int"
    assert tag_or_value[str]("plain") == "string"
    pair = StringIntPair(my_string="b", my_int=2)
    assert "StringIntPair" in tag_or_value[StringIntPair](pair)


# --- make_triple<A, B, C>(...) -> GenericTriple<A, B, C> : multiple TypeVars -


def test_generic_calls_make_triple_explicit():
    # A=int, B=str, C=bool, bound positionally by the subscript.
    t = make_triple[int, str, bool](1, ["a", "b"], {"k": True})
    assert isinstance(t, GenericTriple)
    assert t.first == 1
    assert t.second == ["a", "b"]
    assert t.third == {"k": True}


# --- one_type_arg<T>() / two_type_args<A,B>() : return-position-only TypeVars -
# No argument carries `T`; the binding can only come from the subscript. The
# cleanest proof the inbound path does not rely on argument inference.


def test_generic_calls_one_type_arg_explicit():
    assert one_type_arg[int]() == "int"
    assert one_type_arg[str]() == "string"
    # Nested generic binding must encode fully (base class + concrete arg).
    nested = one_type_arg[GenericBox[int]]()
    assert "GenericBox" in nested and "int" in nested


def test_generic_calls_two_type_args_explicit():
    assert two_type_args[int, str]() == "int | string"


def test_generic_calls_generic_free_fn_requires_binding():
    # Inbound-inference is now on, so a bare generic call no longer raises in the
    # SDK. But `one_type_arg<T>()` / `two_type_args<A,B>()` are return/body-only:
    # NO argument carries the TypeVar, so inference finds no evidence and the
    # *engine* (Gate A) rejects the call. The rejection is a value/type mismatch
    # (`EngineError::TypeMismatch` ⇒ `baml.errors.TypeMismatch`), surfaced to the
    # client as a native Python `TypeError` whose message names the function and
    # explains the type parameter couldn't be inferred.
    # (Binding via `_types=` / subscript remains the way to call these — see
    # test_one_type_arg_explicit / test_two_type_args_explicit. Inference of
    # value-carried TypeVars lives in test_generic_inference.py.)
    with pytest.raises(TypeError) as exc_one:
        one_type_arg()
    assert "could not infer a type" in str(exc_one.value)
    assert "one_type_arg" in str(exc_one.value)
    with pytest.raises(TypeError) as exc_two:
        two_type_args()
    assert "could not infer a type" in str(exc_two.value)


def test_generic_calls_subscript_wrong_arity_raises():
    with pytest.raises(TypeError) as exc:
        two_type_args[int]()  # needs two type args
    assert str(exc.value) == "expected 2 type argument(s) for ['A', 'B'], got 1"


# ===========================================================================
# basic cases, generic classes
# ===========================================================================


# --- consume_int_wrapper(x: GenericBox<int>) -> int : fully-bound TypeVar ----


def test_generic_calls_consume_int_wrapper_baseline():
    # No binding of any kind: a concretely-instantiated `GenericBox<int>` flows
    # in and the `int` field flows back out. Anchors the suite — if this breaks,
    # the generic *class* boundary regressed independent of TypeVar binding.
    assert consume_int_wrapper(GenericBox[int](value=9)) == 9


# --- GenericBox<T>.get(self) -> string : class TypeVar from the receiver -----
# Binding-sensitive: `get` is `reflect.type_of<T>()`. `get` has no own TypeVars,
# so there's nothing to subscript — `T` rides on the parameterized receiver.


def test_generic_calls_genericbox_get_explicit():
    # `GenericBox[int](...)` carries the type arg; the host recovers it from the
    # receiver and seeds it as the method frame's class-level `T`.
    b = GenericBox[int](value=5)
    assert b.get() == "int"


# --- GenericBox<T>.pair_with<U>(self, other: U) -> string : class T + method U


def test_generic_calls_genericbox_pair_with_explicit():
    # `T` from the `GenericBox[int]` receiver, `U` from the method subscript.
    b = GenericBox[int](value=5)
    assert b.pair_with[str]("hello world") == "int | string"


# --- GenericBox<T>.new<V>(value: V) -> GenericBox<V> : generic STATIC method --
# No receiver, so only the static's own `V` is bound (via the subscript); no
# class type args ride along.


def test_generic_calls_genericbox_new_static_explicit():
    box = GenericBox.new[int](value=5)
    assert isinstance(box, GenericBox)
    assert box.value == 5


def test_generic_calls_generic_static_infers_binding():
    # A generic static method's own `V` appears in a parameter (`value: V`), so
    # a bare call now INFERS it from the value — no subscript needed. (Was a hard
    # SDK error pre-inference; see test_generic_inference.py for the inference
    # suite. The explicit subscript form still works above.)
    box = GenericBox.new(value=5)
    assert isinstance(box, GenericBox)
    assert box.value == 5


# --- NamedStatic<A,B,C>.make<D,E> : static TypeVar names DIFFER from the class -
# Proves the named `TyArg` wire slots each binding by TypeVar *name* into the
# static frame's own params (`[D, E]`) — no phantom class params. This is the
# case the named wire exists for (01pt5).


def test_generic_calls_named_static_distinct_typevar_names():
    assert NamedStatic.make[int, str](1, "x") == "int | string"


# --- Negative: an instance method needing class args on an UN-parameterized
# receiver must raise (the class TypeVars can't be recovered).


def test_generic_calls_instance_method_unparameterized_receiver_raises():
    # `GenericBox(value=5)` (no `[int]`) carries no concrete class type args, so
    # `pair_with`'s class `T` can't be recovered host-side.
    with pytest.raises(TypeError) as exc:
        GenericBox(value=5).pair_with[str]("x")
    assert str(exc.value) == (
        "_types= on a generic method requires a Pydantic generic receiver so "
        "the class type args can be recovered"
    )


# --- extract<A, B, C, D>(a: GenericPair<GenericPair<A,B>, GenericPair<C,D>>) --
# Nested generic. Binding-sensitive: body is `reflect.type_of<A|B|C|D>()`.


def test_generic_calls_extract_explicit():
    pair = GenericPair[GenericPair[int, str], GenericPair[bool, float]](
        first=GenericPair[int, str](first=1, second="a"),
        second=GenericPair[bool, float](first=True, second=1.5),
    )
    assert extract[int, str, bool, float](pair) == ("int | string | bool | float")


# ===========================================================================
# basic case, `T` in return position only
# ===========================================================================


# --- parse_as<T>(source: string) -> T : return-position-only TypeVar --------


def test_generic_calls_parse_as_explicit():
    # `T` bound by the host via the subscript (Python surface for `$types`).
    pair = parse_as[StringIntPair]('{"my_string": "x", "my_int": 3}')
    assert pair == StringIntPair(my_string="x", my_int=3)
    assert parse_as[int]("42") == 42


# ===========================================================================
# complex cases
# ===========================================================================


# --- second_of<T>(p: GenericPair<int, T>) -> T : partially-bound class param -


def test_generic_calls_second_of_explicit():
    assert second_of[str](GenericPair[int, str](first=1, second="hi")) == "hi"
    pair = StringIntPair(my_string="z", my_int=9)
    p = GenericPair[int, StringIntPair](first=0, second=pair)
    assert second_of[StringIntPair](p) == pair


# --- list_head<T>(list: GenericRecursive<T>) -> T : recursive generic arg ----


def test_generic_calls_list_head_explicit():
    linked_list = GenericRecursive[int](
        value=7, next=GenericRecursive[int](value=8, next=None)
    )
    assert list_head[int](linked_list) == 7


# --- choose<T>(left: T, right: T) -> T : unification across two args ---------


def test_generic_calls_choose_explicit():
    assert choose[int](1, 2) == 1
    assert choose[str]("a", "b") == "a"


# --- read_items<T>(shape: ContainerShapes<T>) -> T[] : one T, many fields ----


def test_generic_calls_read_items_explicit():
    container = ContainerShapes[int](
        item=1,
        items=[1, 2, 3],
        by_key={"k": 4},
        maybe=None,
        mixed=None,
    )
    assert read_items[int](container) == [1, 2, 3]


# ===========================================================================
# outbound generics
# ===========================================================================


# --- wrap<T>(x: T) -> GenericBox<T> : bind `T`, return a generic over it ------


def test_generic_calls_wrap_explicit():
    w = wrap[int](5)
    assert isinstance(w, GenericBox)
    assert w.value == 5


# ===========================================================================
# reified generics returned by NON-generic functions
# ===========================================================================
# The outbound mirror of `consume_int_wrapper`: no TypeVar binding and no
# inference — the callee's return type pins the class type args at the
# definition site, so the host only has to decode a fully-concrete generic
# instance flowing back out. One test per reification shape.


def _type_args(obj):
    """The concrete type args bound on a returned Pydantic generic instance.

    A reified generic comes back as a *parametrized* subclass (`GenericBox[int]`),
    whose `__pydantic_generic_metadata__["args"]` holds the bound args. A bare,
    unbound instance instead has empty `args` and a leftover `~T` in `parameters`
    — so a non-empty tuple here proves the host actually bound the TypeVars.
    """
    return type(obj).__pydantic_generic_metadata__["args"]


# --- make_int_box() -> GenericBox<int> : one TypeVar, used once -------------


def test_generic_calls_make_int_box_reified():
    box = make_int_box()
    assert isinstance(box, GenericBox)
    assert _type_args(box) == (int,)
    assert box.value == 7


# --- make_int_container() -> ContainerShapes<int> : one TypeVar, many fields -
# The single `int` binding is reified into every field shape (bare, list, map,
# optional, union) — the host must decode all of them off one instance.


def test_generic_calls_make_int_container_reified():
    c = make_int_container()
    assert isinstance(c, ContainerShapes)
    assert _type_args(c) == (int,)
    assert c.item == 1
    assert c.items == [1, 2, 3]
    assert c.by_key == {"k": 4}
    assert c.maybe is None
    assert c.mixed == 5


# --- make_nested_box() -> GenericBox<GenericBox<int>> : nested generic arg ---
# The type arg is itself a generic instance; the host must decode the inner
# GenericBox out of the outer one's field.


def test_generic_calls_make_nested_box_reified():
    outer = make_nested_box()
    assert isinstance(outer, GenericBox)
    # The outer box's single type arg is itself the parametrized `GenericBox[int]`.
    assert _type_args(outer) == (GenericBox[int],)
    assert isinstance(outer.value, GenericBox)
    assert _type_args(outer.value) == (int,)
    assert outer.value.value == 9


# --- make_int_str_bool_triple() -> GenericTriple<int, string, bool> ---------
# Multiple TypeVars reified across mixed field shapes: scalar int, string list,
# bool-valued map.


def test_generic_calls_make_int_str_bool_triple_reified():
    t = make_int_str_bool_triple()
    assert isinstance(t, GenericTriple)
    assert _type_args(t) == (int, str, bool)
    assert t.first == 1
    assert t.second == ["a", "b"]
    assert t.third == {"k": True}
