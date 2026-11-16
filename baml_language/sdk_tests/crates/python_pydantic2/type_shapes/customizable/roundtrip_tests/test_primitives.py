"""Roundtrip coverage for `baml_sdk.primitives` (35-series goal).

Calls each emitted BAML function for the primitive Ty variants from
Python and asserts the value survives the encode → FFI → decode round
trip. `return_*` functions exercise decode-only; `round_trip_*` exercise
the full encode/decode pair.
"""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.primitives import (
    Primitives,
    return_int_sync,
    return_float_sync,
    return_string_sync,
    return_bool_sync,
    return_bigint_sync,
    return_null_sync,
    round_trip_uint8_array_sync,
    round_trip_int_sync,
    round_trip_float_sync,
    round_trip_string_sync,
    round_trip_bool_sync,
    round_trip_bigint_sync,
    round_trip_null_sync,
    round_trip_primitives_sync,
)


def test_primitives_return_int():
    assert return_int_sync() == 42


def test_primitives_return_float():
    assert return_float_sync() == 3.14


def test_primitives_return_string():
    assert return_string_sync() == "hello"


def test_primitives_return_bool():
    assert return_bool_sync() is True


def test_primitives_return_bigint():
    # 12345678901234567890 > i64 max (9223372036854775807), so it decodes
    # through the hex bigint wire channel rather than int_value.
    assert return_bigint_sync() == 12345678901234567890


def test_primitives_return_null():
    assert return_null_sync() is None


def test_primitives_round_trip_int():
    assert round_trip_int_sync(x=7) == 7


def test_primitives_round_trip_float():
    assert round_trip_float_sync(x=2.5) == 2.5


def test_primitives_round_trip_float_accepts_int():
    # A Python int into a float param widens at the FFI boundary (the wire
    # encoder is value-shaped, so `7` arrives Rust-side as an int). The
    # declared `-> float` must hand back a genuine float, not the int riding
    # through unconverted.
    result = round_trip_float_sync(x=7)
    assert isinstance(result, float)
    assert result == 7.0


def test_primitives_round_trip_string():
    assert round_trip_string_sync(x="hi") == "hi"


def test_primitives_round_trip_bool():
    assert round_trip_bool_sync(x=False) is False


def test_primitives_round_trip_bigint():
    # Encode uses the hex bigint channel only for values outside i64 range (an
    # in-range int rides int_value), so every case here is beyond i64: a large
    # positive, its negation, and a power-of-two hex boundary (2**64 — the
    # first magnitude needing a 17th hex digit).
    big = 2**80
    assert round_trip_bigint_sync(x=big) == big
    assert round_trip_bigint_sync(x=-big) == -big
    hex_boundary = 2**64
    assert round_trip_bigint_sync(x=hex_boundary) == hex_boundary


def test_primitives_round_trip_null():
    assert round_trip_null_sync(x=None) is None


def test_primitives_round_trip_uint8_array():
    assert round_trip_uint8_array_sync(b=b"\x00\x01\x02") == b"\x00\x01\x02"


def test_primitives_round_trip_primitives():
    p = Primitives(
        int_field=1,
        float_field=1.5,
        string_field="s",
        bool_field=True,
        null_field=None,
        uint8array_field=b"ab",
    )
    assert round_trip_primitives_sync(p=p) == p


def test_primitives_round_trip_primitives_float_field_accepts_int():
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
    )
    assert isinstance(p.float_field, float)
    result = round_trip_primitives_sync(p=p)
    assert result.float_field == 2.0
