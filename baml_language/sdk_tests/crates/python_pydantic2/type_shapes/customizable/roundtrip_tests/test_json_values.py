"""Roundtrip coverage for `baml_sdk.json_values` — the stdlib `json` type.

Every BAML function here is declared to return `json` (`baml.json.json`),
but each returns a differently-shaped value. `return_json_*` functions
exercise decode-only for each JSON shape; `round_trip_json` exercises the
full encode/decode pair over the same shapes; `JsonContainer` covers
`json` in a class field position.
"""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.json_values import (
    JsonContainer,
    return_json_null,
    return_json_bool,
    return_json_int,
    return_json_float,
    return_json_string,
    return_json_array,
    return_json_object,
    return_json_nested,
    round_trip_json,
    round_trip_json_container,
)


def test_return_json_null():
    assert return_json_null() is None


def test_return_json_bool():
    assert return_json_bool() is True


def test_return_json_int():
    assert return_json_int() == 42


def test_return_json_float():
    assert return_json_float() == 3.14


def test_return_json_string():
    assert return_json_string() == "hello"


def test_return_json_array():
    assert return_json_array() == [1, 2, 3]


def test_return_json_object():
    assert return_json_object() == {"key": "value"}


def test_return_json_nested():
    assert return_json_nested() == {"a": 1, "b": [2, 3], "c": {"nested": None}}


def test_round_trip_json_null():
    assert round_trip_json(j=None) is None


def test_round_trip_json_bool():
    assert round_trip_json(j=False) is False


def test_round_trip_json_int():
    assert round_trip_json(j=7) == 7


def test_round_trip_json_float():
    assert round_trip_json(j=2.5) == 2.5


def test_round_trip_json_string():
    assert round_trip_json(j="hi") == "hi"


def test_round_trip_json_array():
    assert round_trip_json(j=[1, "two", True, None]) == [1, "two", True, None]


def test_round_trip_json_object():
    nested = {"a": 1, "b": [2, 3], "c": {"nested": None}}
    assert round_trip_json(j=nested) == nested


def test_round_trip_json_container():
    c = JsonContainer(data={"k": [1, 2, {"deep": None}]})
    assert round_trip_json_container(c=c) == c
