"""Roundtrip coverage for `baml_sdk.aliases` — type aliases (incl. recursive)."""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.aliases import (
    AliasContainer,
    round_trip_string_list,
    round_trip_rec_list,
    round_trip_alias_container,
)


def test_aliases_round_trip_string_list():
    assert round_trip_string_list(s=["a", "b"]) == ["a", "b"]


def test_aliases_round_trip_rec_list():
    # RecList = int | RecList[]
    assert round_trip_rec_list(r=1) == 1
    assert round_trip_rec_list(r=[1, [2, 3]]) == [1, [2, 3]]


def test_aliases_round_trip_alias_container():
    c = AliasContainer(list_field=["x"], rec_field=[1, [2]])
    assert round_trip_alias_container(c=c) == c
