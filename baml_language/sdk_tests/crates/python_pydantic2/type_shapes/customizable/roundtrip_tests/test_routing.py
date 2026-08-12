"""Roundtrip coverage for the cross-namespace routing-rules suite:
root (`baml_sdk`), `a`, `a.b`, `lorem`, and `ipsum` leaves.

The `baml.http.Response`-typed round trips in `lorem` are covered in
`test_streams.py` (they need an engine-minted handle and can't be built
host-side).
"""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk import Foo, make_foo, round_trip_foo
from baml_sdk.a import round_trip_deep_thing_from_a
from baml_sdk.a.b import Thing, round_trip_thing_from_ab, round_trip_root_foo_from_ab
from baml_sdk.lorem import (
    Resume,
    round_trip_resume,
    round_trip_root_foo,
    round_trip_deep_thing_from_lorem,
)
from baml_sdk.ipsum import round_trip_lorem_resume_from_ipsum


def test_routing_make_foo():
    assert make_foo(v=3).v == 3


def test_routing_round_trip_foo():
    f = Foo(v=10)
    assert round_trip_foo(f=f) == f


def test_routing_round_trip_thing_from_ab():
    t = Thing(v=1)
    assert round_trip_thing_from_ab(t=t) == t


def test_routing_round_trip_root_foo_from_ab():
    f = Foo(v=2)
    assert round_trip_root_foo_from_ab(f=f) == f


def test_routing_round_trip_deep_thing_from_a():
    t = Thing(v=4)
    assert round_trip_deep_thing_from_a(t=t) == t


def test_routing_round_trip_deep_thing_from_lorem():
    t = Thing(v=5)
    assert round_trip_deep_thing_from_lorem(t=t) == t


def test_routing_round_trip_resume():
    r = Resume(name="ada", email=None)
    assert round_trip_resume(r=r) == r


def test_routing_round_trip_root_foo():
    f = Foo(v=6)
    assert round_trip_root_foo(f=f) == f


def test_routing_round_trip_lorem_resume_from_ipsum():
    r = Resume(name="grace", email="g@x.com")
    assert round_trip_lorem_resume_from_ipsum(r=r) == r
