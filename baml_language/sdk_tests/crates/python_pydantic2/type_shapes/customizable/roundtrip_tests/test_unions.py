"""Roundtrip coverage for `baml_sdk.unions` — union normalization variants."""

import pytest

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.unions import (
    T,
    UnionContainer,
    round_trip_null_to_end_sync,
    round_trip_dedup_sync,
    round_trip_singleton_unwrap_sync,
    round_trip_optional_plus_null_sync,
    round_trip_str_or_int_list_sync,
    round_trip_t_sync,
    round_trip_union_container_sync,
)


def test_unions_round_trip_null_to_end():
    assert round_trip_null_to_end_sync(u=1) == 1
    assert round_trip_null_to_end_sync(u="s") == "s"
    assert round_trip_null_to_end_sync(u=None) is None


def test_unions_round_trip_dedup():
    assert round_trip_dedup_sync(u=2) == 2
    assert round_trip_dedup_sync(u="x") == "x"


def test_unions_round_trip_singleton_unwrap():
    # `int | int` collapses to plain `int`.
    assert round_trip_singleton_unwrap_sync(u=7) == 7


def test_unions_round_trip_optional_plus_null():
    assert round_trip_optional_plus_null_sync(u=T(v=1)) == T(v=1)
    assert round_trip_optional_plus_null_sync(u="s") == "s"
    assert round_trip_optional_plus_null_sync(u=None) is None


def test_unions_round_trip_str_or_int_list():
    # Differently-typed list arms. The wire's `item_type` (populated by the
    # engine even for an empty list) is what selects the arm; Python drops the
    # union wrapper and returns the bare list, so these assert the value
    # round-trip. A non-empty list of each element type round-trips faithfully.
    assert round_trip_str_or_int_list_sync(x=[1, 2, 3]) == [1, 2, 3]
    assert round_trip_str_or_int_list_sync(x=["a", "b"]) == ["a", "b"]
    # An empty list fits both arms as distinct types, and the bridge never
    # picks a union member by declaration order: the call is refused until the
    # value says which member it is.
    with pytest.raises(TypeError, match="fits more than one"):
        round_trip_str_or_int_list_sync(x=[])


def test_unions_round_trip_t():
    assert round_trip_t_sync(t=T(v=4)) == T(v=4)


def test_unions_round_trip_union_container():
    c = UnionContainer(
        null_to_end=None,
        dedup="d",
        singleton_unwrap=5,
        optional_plus_null=T(v=2),
    )
    assert round_trip_union_container_sync(c=c) == c
