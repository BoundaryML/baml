"""Roundtrip coverage for `baml_sdk.lists` — list Ty variants."""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.lists import (
    ListContainer,
    round_trip_ints_sync,
    round_trip_optional_strings_sync,
    round_trip_union_list_sync,
    round_trip_list_container_sync,
)


def test_lists_round_trip_ints():
    assert round_trip_ints_sync(xs=[1, 2, 3]) == [1, 2, 3]


def test_lists_round_trip_empty_list():
    # Regression for Bug A (35b), fixed by `SetInParent()` on the
    # `list_value` oneof arm in proto.py: an empty list used to encode as
    # an unset oneof, which the engine read as null and returned as `None`.
    assert round_trip_ints_sync(xs=[]) == []


def test_lists_round_trip_optional_strings():
    assert round_trip_optional_strings_sync(xs=["a", None, "b"]) == ["a", None, "b"]


def test_lists_round_trip_union_list():
    assert round_trip_union_list_sync(xs=[1, "two", 3]) == [1, "two", 3]


def test_lists_round_trip_list_container():
    c = ListContainer(
        ints=[1, 2],
        optional_strings=[None, "z"],
        union_list=[1, "x"],
    )
    assert round_trip_list_container_sync(c=c) == c
