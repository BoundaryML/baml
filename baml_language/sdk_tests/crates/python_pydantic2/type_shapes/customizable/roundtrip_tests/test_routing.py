"""Roundtrip coverage for the cross-namespace routing-rules suite:
root (`baml_sdk`), `a`, `a.b`, `lorem`, and `ipsum` leaves.

The `baml.http.Response` arms in `lorem` need an engine-minted handle and
can't be built host-side; the handle round trip is covered by
`test_handles.py`.
"""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk import Foo, make_foo_sync, round_trip_foo_sync
from baml_sdk.a import round_trip_deep_thing_from_a_sync
from baml_sdk.a.b import Thing, round_trip_thing_from_ab_sync, round_trip_root_foo_from_ab_sync
from baml_sdk.lorem import (
    Resume,
    round_trip_resume_sync,
    round_trip_root_foo_sync,
    round_trip_deep_thing_from_lorem_sync,
    round_trip_resume_or_http_response_sync,
)
from baml_sdk.ipsum import round_trip_lorem_resume_from_ipsum_sync


def test_routing_make_foo():
    assert make_foo_sync(v=3).v == 3


def test_routing_round_trip_foo():
    f = Foo(v=10)
    assert round_trip_foo_sync(f=f) == f


def test_routing_round_trip_thing_from_ab():
    t = Thing(v=1)
    assert round_trip_thing_from_ab_sync(t=t) == t


def test_routing_round_trip_root_foo_from_ab():
    f = Foo(v=2)
    assert round_trip_root_foo_from_ab_sync(f=f) == f


def test_routing_round_trip_deep_thing_from_a():
    t = Thing(v=4)
    assert round_trip_deep_thing_from_a_sync(t=t) == t


def test_routing_round_trip_deep_thing_from_lorem():
    t = Thing(v=5)
    assert round_trip_deep_thing_from_lorem_sync(t=t) == t


def test_routing_round_trip_resume():
    r = Resume(name="ada", email=None)
    assert round_trip_resume_sync(r=r) == r


def test_routing_round_trip_root_foo():
    f = Foo(v=6)
    assert round_trip_root_foo_sync(f=f) == f


def test_routing_round_trip_lorem_resume_from_ipsum():
    r = Resume(name="grace", email="g@x.com")
    assert round_trip_lorem_resume_from_ipsum_sync(r=r) == r


def test_routing_round_trip_resume_or_http_response():
    # Pass the `Resume` arm; the `baml.http.Response` arm isn't
    # host-constructible.
    r = Resume(name="lovelace", email="a@x.com")
    assert round_trip_resume_or_http_response_sync(u=r) == r
