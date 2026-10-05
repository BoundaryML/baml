"""A value of another Python type than a parameter declares.

The engine checks each argument against the declared type before the function
runs. A value of another kind is a `TypeError` that names the function, the
position of the argument, the kind of the value and the declared type. Before
this check the value entered the function as it was: `round_trip_int("7")`
ran with a string in the `int` parameter.

The generated annotations name the declared type, so the calls below go
through `typing.cast`: a type checker rejects them, and a statically typed SDK
cannot write them.
"""

from __future__ import annotations

import typing

import pytest

import baml_sdk  # noqa: F401  — importing initializes the BAML runtime
from baml_sdk import (
    Person,
    round_trip_bool,
    round_trip_float,
    round_trip_int,
    round_trip_person,
    round_trip_string,
)
from baml_sdk.host_callable_tests import call_list_roundtrip_callback


def _rejected(function: str, kind: str, declared: str) -> str:
    return (
        rf"`{function}` was called with a value that doesn't match its type: "
        rf"argument \d: Value of type '{kind}' does not match the declared type `{declared}`"
    )


@pytest.mark.parametrize(
    ("value", "kind"),
    [("7", "string"), (1.5, "float"), (True, "bool"), (None, "null"), ([1], "array")],
)
# SDK_PARITY_LINT(skip): passes a value of another Python type than the generated annotation; a statically typed SDK cannot write that call
def test_argument_types_int_rejects_a_value_of_another_kind(value: object, kind: str):
    with pytest.raises(TypeError, match=_rejected("round_trip_int", kind, "int")):
        round_trip_int(typing.cast(int, value))


# SDK_PARITY_LINT(skip): passes a value of another Python type than the generated annotation; a statically typed SDK cannot write that call
def test_argument_types_scalars_reject_a_value_of_another_kind():
    with pytest.raises(TypeError, match=_rejected("round_trip_float", "string", "float")):
        round_trip_float(typing.cast(float, "1.5"))
    with pytest.raises(TypeError, match=_rejected("round_trip_string", "int", "string")):
        round_trip_string(typing.cast(str, 7))
    with pytest.raises(TypeError, match=_rejected("round_trip_bool", "int", "bool")):
        round_trip_bool(typing.cast(bool, 1))


# SDK_PARITY_LINT(skip): passes a value of another Python type than the generated annotation; a statically typed SDK cannot write that call
async def test_argument_types_int_rejects_a_value_of_another_kind_async():
    with pytest.raises(TypeError, match=_rejected("round_trip_int", "string", "int")):
        await baml_sdk.round_trip_int_async(typing.cast(int, "7"))


# SDK_PARITY_LINT(skip): passes a value of another Python type than the generated annotation; a statically typed SDK cannot write that call
def test_argument_types_list_rejects_an_item_of_another_kind():
    def echo(values: list[int]) -> list[int]:
        return values

    with pytest.raises(TypeError, match=_rejected("call_list_roundtrip_callback", "string", "int")):
        call_list_roundtrip_callback(echo, typing.cast("list[int]", [1, "two"]))
    with pytest.raises(TypeError, match=_rejected("call_list_roundtrip_callback", "string", r"int\[\]")):
        call_list_roundtrip_callback(echo, typing.cast("list[int]", "12"))


# SDK_PARITY_LINT(skip): passes a value of another Python type than the generated annotation; a statically typed SDK cannot write that call
def test_argument_types_class_rejects_a_field_of_another_kind():
    """A `dict` stands for the class; its fields are checked like arguments."""
    fields = {"person": "p", "name": "Ada", "age": "thirty-six"}
    with pytest.raises(TypeError, match=_rejected("round_trip_person", "string", "int")):
        round_trip_person(typing.cast(Person, fields))
    with pytest.raises(TypeError, match=_rejected("round_trip_person", "string", "Person")):
        round_trip_person(typing.cast(Person, "Ada"))


# SDK_PARITY_LINT(skip): passes a Python dict and a Python int where the generated annotations are a class and a float
def test_argument_types_values_that_the_boundary_converts_are_accepted():
    """An `int` is a `float` where a float is declared, and a `dict` with the
    fields of a class is that class."""
    half = round_trip_float(typing.cast(float, 3))
    assert type(half) is float and half == 3.0
    fields = {"person": "p", "name": "Ada", "age": 36}
    assert round_trip_person(typing.cast(Person, fields)) == Person(person="p", name="Ada", age=36)
