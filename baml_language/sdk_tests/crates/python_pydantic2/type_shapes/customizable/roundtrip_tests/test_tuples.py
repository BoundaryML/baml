"""Roundtrip coverage for `baml_sdk.tuples`: fixed-arity tuples as Python's
native `tuple`, in every position the generated SDK reaches."""

import asyncio
import typing

import pytest

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.baml import BamlError
from baml_sdk.tuples import (
    TupleBox,
    TupleHolder,
    call_tuple_callback,
    call_tuple_callback_async,
    call_unknown_tuple_callback,
    make_pair,
    pair_first,
    round_trip_holder,
    round_trip_nested,
    round_trip_optional_slot,
    round_trip_optional_tuple,
    round_trip_pair,
    round_trip_pair_async,
    round_trip_repeated,
    round_trip_single,
    round_trip_tuple_arms,
    round_trip_tuple_box,
    round_trip_tuple_list,
    round_trip_tuple_map,
    round_trip_tuple_or_string,
    round_trip_wide,
    swap_pair,
    throw_pair,
    tuple_as_unknown,
    tuple_generic_identity,
    unknown_has_carried_tuple_type,
    value_has_type,
)


# SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
def test_tuples_round_trip_pair():
    result = round_trip_pair(t=(1, "a"))
    assert result == (1, "a")
    assert isinstance(result, tuple)


# SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
def test_tuples_round_trip_single_and_repeated():
    assert round_trip_single(t=("x",)) == ("x",)
    assert round_trip_repeated(t=(1, 2, 3)) == (1, 2, 3)


# SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
def test_tuples_round_trip_nested():
    assert round_trip_nested(t=((1, "a"), True)) == ((1, "a"), True)


# SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
def test_tuples_round_trip_optional_slot_and_optional_tuple():
    assert round_trip_optional_slot(t=(1, None)) == (1, None)
    assert round_trip_optional_tuple(t=None) is None
    assert round_trip_optional_tuple(t=(2, "b")) == (2, "b")


# SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
def test_tuples_round_trip_wide():
    wide = tuple(i if i % 2 == 0 else str(i) for i in range(13))
    assert round_trip_wide(t=wide) == wide  # type: ignore[arg-type]


# SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
def test_tuples_round_trip_pair_async():
    assert asyncio.run(round_trip_pair_async(t=(1, "a"))) == (1, "a")


# SDK_PARITY_LINT(skip): only Python and Java accept a plain list at a tuple slot
def test_tuples_plain_list_stands_in_at_a_tuple_slot():
    assert round_trip_pair(t=[1, "a"]) == (1, "a")  # type: ignore[arg-type]


# SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
def test_tuples_wrong_arity_and_slots_rejected():
    with pytest.raises(TypeError):
        round_trip_pair(t=(1,))  # type: ignore[arg-type]
    with pytest.raises(TypeError):
        round_trip_pair(t=(1, "a", 2))  # type: ignore[arg-type]
    with pytest.raises(TypeError):
        round_trip_pair(t=("a", 1))  # type: ignore[arg-type]


# SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
def test_tuples_round_trip_containers():
    assert round_trip_tuple_list(xs=[("a", 1), ("b", 2)]) == [("a", 1), ("b", 2)]
    assert round_trip_tuple_map(m={"k": (1, True)}) == {"k": (1, True)}


# SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
def test_tuples_round_trip_union_arms():
    assert round_trip_tuple_or_string(u=(1, "a")) == (1, "a")
    assert round_trip_tuple_or_string(u="s") == "s"
    assert round_trip_tuple_arms(u=(1, "a")) == (1, "a")
    assert round_trip_tuple_arms(u=("a", 1)) == ("a", 1)


# SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
def test_tuples_round_trip_class_fields():
    holder = TupleHolder(
        pair=(1, "a"),
        nested=((2, "b"), False),
        maybe=(3, None),
        pairs=[("c", 4)],
    )
    assert round_trip_holder(h=holder) == holder


# SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
def test_tuples_round_trip_generic_argument():
    box = TupleBox[typing.Tuple[int, str]](value=(1, "a"))
    assert round_trip_tuple_box(b=box).value == (1, "a")


# SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
def test_tuples_make_swap_and_index():
    assert make_pair(n=1, s="a") == (1, "a")
    assert swap_pair(t=(1, "a")) == ("a", 1)
    assert pair_first(t=(7, "a")) == 7
    # A tuple BAML made goes back as the tuple it is.
    assert round_trip_pair(t=make_pair(n=2, s="b")) == (2, "b")


# SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
def test_tuples_host_callback_args_and_results():
    seen = []

    def callback(t):
        seen.append(t)
        return (t[1], t[0])

    assert call_tuple_callback(callback=callback, t=(1, "a")) == ("a", 1)
    assert seen == [(1, "a")]
    assert isinstance(seen[0], tuple)

    async def async_callback(t):
        await asyncio.sleep(0)
        return (t[1], t[0])

    assert asyncio.run(
        call_tuple_callback_async(callback=async_callback, t=(2, "b"))
    ) == ("b", 2)


# SDK_PARITY_LINT(skip): Python's untyped callbacks can return a wrong-shaped tuple
def test_tuples_callback_rejects_a_wrong_tuple_result():
    with pytest.raises(BaseException):
        call_tuple_callback(callback=lambda t: (t[0], t[1]), t=(1, "a"))  # type: ignore[arg-type]


# SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
def test_tuples_thrown_value():
    with pytest.raises(BamlError) as raised:
        throw_pair(t=(1, "a"))
    assert raised.value.value == (1, "a")


# SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
def test_tuples_dynamic_value_keeps_carried_type():
    received = tuple_as_unknown(t=(1, True))
    assert received == (1, True)
    # `(1, True)` alone would state `(int, bool)`; the carried type is kept.
    assert unknown_has_carried_tuple_type(v=received) is True
    assert unknown_has_carried_tuple_type(v=(1, True)) is False
    # A callback at an open slot echoing what it received keeps it too.
    assert call_unknown_tuple_callback(callback=lambda v: v, t=(1, True)) is True


# SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
def test_tuples_reflected_type_token():
    assert (
        value_has_type(v=(1, "a"), _types={"T": typing.Tuple[int, str]}) is True
    )
    assert value_has_type(v=[1, "a"], _types={"T": typing.Tuple[int, str]}) is False
    result = tuple_generic_identity(x=(1, "a"))
    assert result == (1, "a")
    assert isinstance(result, tuple)
    assert tuple_generic_identity(x=[1, "a"]) == [1, "a"]
