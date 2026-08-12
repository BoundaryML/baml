"""Roundtrip coverage for `baml_sdk.optional` — optional Ty variants."""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.optional import (
    Resume,
    OptionalContainer,
    round_trip_optional_int,
    round_trip_optional_resume,
    round_trip_optional_union,
    round_trip_resume,
    round_trip_optional_container,
)


def test_optional_round_trip_optional_int():
    assert round_trip_optional_int(x=5) == 5
    assert round_trip_optional_int(x=None) is None


def test_optional_round_trip_optional_resume():
    r = Resume(name="ada")
    assert round_trip_optional_resume(r=r) == r
    assert round_trip_optional_resume(r=None) is None


def test_optional_round_trip_optional_union():
    assert round_trip_optional_union(u=3) == 3
    assert round_trip_optional_union(u="s") == "s"
    assert round_trip_optional_union(u=None) is None


def test_optional_round_trip_resume():
    r = Resume(name="grace")
    assert round_trip_resume(r=r) == r


def test_optional_round_trip_optional_container():
    c = OptionalContainer(
        optional_int=None,
        optional_class=Resume(name="x"),
        optional_union="y",
    )
    assert round_trip_optional_container(c=c) == c
