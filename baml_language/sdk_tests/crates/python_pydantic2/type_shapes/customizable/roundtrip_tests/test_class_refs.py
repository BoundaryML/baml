"""Roundtrip coverage for `baml_sdk.class_refs` — class composition."""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.class_refs import (
    Inner,
    Outer,
    make_outer,
    round_trip_inner,
    round_trip_outer,
)


def test_class_refs_make_outer():
    o = make_outer(value=5)
    assert o.inner.value == 5


def test_class_refs_round_trip_inner():
    i = Inner(value=3)
    assert round_trip_inner(i=i) == i


def test_class_refs_round_trip_outer():
    o = Outer(inner=Inner(value=9))
    assert round_trip_outer(o=o) == o
