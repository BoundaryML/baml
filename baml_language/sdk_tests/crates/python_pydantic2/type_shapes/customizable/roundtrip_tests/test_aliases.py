"""Roundtrip coverage for `baml_sdk.aliases` — type aliases (incl. recursive)."""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.aliases import (
    AliasContainer,
    round_trip_string_list_sync,
    round_trip_rec_list_sync,
    round_trip_alias_container_sync,
)


def test_aliases_round_trip_string_list():
    assert round_trip_string_list_sync(s=["a", "b"]) == ["a", "b"]


def test_aliases_round_trip_rec_list():
    # RecList = int | RecList[]
    assert round_trip_rec_list_sync(r=1) == 1
    assert round_trip_rec_list_sync(r=[1, [2, 3]]) == [1, [2, 3]]


def test_aliases_round_trip_alias_container():
    c = AliasContainer(list_field=["x"], rec_field=[1, [2]])
    assert round_trip_alias_container_sync(c=c) == c
