"""Roundtrip coverage for `baml_sdk.primitives` (35-series goal).

Calls each emitted BAML function for the primitive Ty variants from
Python and asserts the value survives the encode → FFI → decode round
trip. `return_*` functions exercise decode-only; `round_trip_*` exercise
the full encode/decode pair.
"""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.primitives import (
    Primitives,
    return_int,
    return_float,
    return_string,
    return_bool,
    return_bigint,
    return_null,
    round_trip_uint8_array,
    round_trip_int,
    round_trip_bigint,
    round_trip_float,
    round_trip_string,
    round_trip_bool,
    round_trip_null,
    round_trip_primitives,
)


def test_return_int():
    assert return_int() == 42


def test_return_float():
    assert return_float() == 3.14


def test_return_string():
    assert return_string() == "hello"


def test_return_bool():
    assert return_bool() is True


def test_return_null():
    assert return_null() is None


def test_return_bigint():
    assert return_bigint() == 9223372036854775808


def test_round_trip_int():
    assert round_trip_int(x=7) == 7


def test_round_trip_bigint_zero():
    assert round_trip_bigint(x=0) == 0


def test_round_trip_bigint_negative():
    assert round_trip_bigint(x=-9223372036854775809) == -9223372036854775809


def test_round_trip_bigint_larger_than_i64():
    assert round_trip_bigint(x=2**100) == 2**100


def test_round_trip_float():
    assert round_trip_float(x=2.5) == 2.5


def test_round_trip_float_accepts_int():
    # A Python int into a float param widens at the FFI boundary (the wire
    # encoder is value-shaped, so `7` arrives Rust-side as an int). The
    # declared `-> float` must hand back a genuine float, not the int riding
    # through unconverted.
    result = round_trip_float(x=7)
    assert isinstance(result, float)
    assert result == 7.0


def test_round_trip_float_accepts_large_integral_value():
    value = 2**54
    result = round_trip_float(x=value)
    assert isinstance(result, float)
    assert result == float(value)


def test_round_trip_string():
    assert round_trip_string(x="hi") == "hi"


def test_round_trip_bool():
    assert round_trip_bool(x=False) is False


def test_round_trip_null():
    assert round_trip_null(x=None) is None


def test_round_trip_uint8_array():
    assert round_trip_uint8_array(b=b"\x00\x01\x02") == b"\x00\x01\x02"


def test_round_trip_primitives():
    p = Primitives(
        int_field=1,
        float_field=1.5,
        string_field="s",
        bool_field=True,
        null_field=None,
        uint8array_field=b"ab",
        bigint_field=2**100,
    )
    assert round_trip_primitives(p=p) == p


def test_round_trip_primitives_float_field_accepts_int():
    # An int into a float *field* is coerced by pydantic at construction, so
    # it reaches the wire as a float already — pin that contract alongside the
    # param-level widening above.
    p = Primitives(
        int_field=1,
        float_field=2,
        string_field="s",
        bool_field=True,
        null_field=None,
        uint8array_field=b"ab",
        bigint_field=2**100,
    )
    assert isinstance(p.float_field, float)
    result = round_trip_primitives(p=p)
    assert result.float_field == 2.0
