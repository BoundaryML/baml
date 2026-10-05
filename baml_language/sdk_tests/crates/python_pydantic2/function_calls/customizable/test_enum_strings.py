"""A plain `str` where a BAML enum is declared.

A host often holds an enum's variant as a string: a name read from JSON, a
command-line flag or a configuration file. A generated Python enum is a
`(str, enum.Enum)`, so a member already *is* a `str`, and the engine reads a
plain string as the variant it names:

  - a string that names a variant becomes that variant, where the declared
    type is the enum (alone or beside types that do not take a string);
  - a string that names no variant is a `TypeError` that lists the variants;
  - beside a member that takes a string as it is (`string`), the string stays
    a string.

The type-mismatch messages name types as a user writes them (`int | null`),
never in the Rust debug form (`[Int, Null]`).

The fixture functions have an enum parameter declared as the enum itself, so
the calls below go through `typing.cast`: the annotations of the generated SDK
are not widened to `str`.
"""

from __future__ import annotations

import typing

import pytest

import baml_sdk  # noqa: F401  — importing initializes the BAML runtime
from baml_sdk.host_callable_tests import (
    CallbackMood,
    Person,
    call_enum_roundtrip_callback,
    call_nominal_union_roundtrip_callback,
)
from baml_sdk.static_method_edges import Edge, StaticMood


def test_enum_strings_plain_string_becomes_the_variant():
    """`Edge.enum_value(value: StaticMood) -> StaticMood` returns what it is
    given. A variant comes back as a member of the generated enum, so the
    string was a variant inside BAML and not a string in an enum's place."""
    for name, member in [("HAPPY", StaticMood.HAPPY), ("SAD", StaticMood.SAD)]:
        result = Edge.enum_value(typing.cast(StaticMood, name))
        assert result is member
    # A member of the generated enum takes the same path as before.
    assert Edge.enum_value(StaticMood.SAD) is StaticMood.SAD


async def test_enum_strings_plain_string_becomes_the_variant_async():
    assert await Edge.enum_value_async(typing.cast(StaticMood, "HAPPY")) is StaticMood.HAPPY


def test_enum_strings_string_that_names_no_variant_is_a_type_error():
    with pytest.raises(TypeError) as excinfo:
        Edge.enum_value(typing.cast(StaticMood, "GRUMPY"))
    message = str(excinfo.value)
    assert (
        'the string "GRUMPY" is not a variant of enum `static_method_edges.StaticMood`; '
        "its variants are HAPPY, SAD"
    ) in message
    # A variant's name is matched as it is written.
    with pytest.raises(TypeError):
        Edge.enum_value(typing.cast(StaticMood, "happy"))


def test_enum_strings_string_stays_a_string_beside_a_string_member():
    """`Person | CallbackMood | string`: `string` takes the value as it is, so
    a string that happens to name a variant is still a string."""
    seen: list[object] = []

    def echo(value: Person | CallbackMood | str) -> Person | CallbackMood | str:
        seen.append(value)
        return value

    result = call_nominal_union_roundtrip_callback(echo, "HAPPY")
    assert type(result) is str and result == "HAPPY"
    assert type(seen[0]) is str

    # A member of the enum is still the enum.
    result = call_nominal_union_roundtrip_callback(echo, CallbackMood.HAPPY)
    assert result is CallbackMood.HAPPY
    assert seen[1] is CallbackMood.HAPPY


def test_enum_strings_callback_receives_the_variant():
    """A plain string for the enum argument reaches a host callback as a
    member of the generated enum: it crossed BAML as a variant."""
    seen: list[CallbackMood] = []

    def record(mood: CallbackMood) -> CallbackMood:
        seen.append(mood)
        return mood

    result = call_enum_roundtrip_callback(record, typing.cast(CallbackMood, "SAD"))
    assert result is CallbackMood.SAD
    assert seen == [CallbackMood.SAD] and seen[0] is CallbackMood.SAD


def test_enum_strings_union_mismatch_names_types_as_written():
    """`Edge.nullable(value: int?)` and `Edge.union_value(value: string | int)`
    with a value no member takes."""
    with pytest.raises(TypeError) as excinfo:
        Edge.nullable(typing.cast(int, "seven"))
    assert "Value of type 'string' does not match any member of union `int | null`" in str(excinfo.value)

    with pytest.raises(TypeError) as excinfo:
        Edge.union_value(typing.cast(str, [1, 2]))
    assert "Value of type 'array' does not match any member of union `string | int`" in str(excinfo.value)
