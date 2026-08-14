"""Roundtrip coverage for the `lorem` stream-type / stdlib-routing suite.

These are the riskiest shapes in the fixture: `$stream` companion types
(`Resume$stream`, `Foo$stream`) are normally engine-internal *partial*
values. Whether a host-constructed `stream_types.*` pydantic model can be
encoded and round-tripped through a `$stream`-typed parameter is what these
tests probe.

`baml.http.Response`-backed parameters (bare, in a list, or as a `$stream`)
can't be driven from pure Python — the `_body: _BamlPyHandle` is engine-minted
— so they're omitted here; the handle round-trip is covered by
`test_handles.py` instead.

If a `$stream` probe fails with an encode/decode/type-mismatch error from the
bridge, that's recorded in 35b as a bridge-surface limitation rather than a
test-authoring bug.
"""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.lorem import (
    Box,
    Resume,
    round_trip_resume_stream,
    round_trip_root_foo_stream,
    round_trip_box_of_resume_stream,
    round_trip_resume_or_http_response,
    round_trip_resume_or_resume_stream,
)
from baml_sdk.stream_types.lorem import Resume as StreamResume
from baml_sdk.stream_types import Foo as StreamFoo


def test_streams_round_trip_resume_stream():
    r = StreamResume(name="ada", email=None)
    assert round_trip_resume_stream(r=r) == r


def test_streams_round_trip_root_foo_stream():
    f = StreamFoo(v=3)
    assert round_trip_root_foo_stream(f=f) == f


def test_streams_round_trip_box_of_resume_stream():
    b = Box(v=StreamResume(name="grace", email=None))
    assert round_trip_box_of_resume_stream(b=b) == b


def test_streams_round_trip_resume_or_resume_stream():
    # Union arm `Resume` (the non-stream side) is host-constructible.
    r = Resume(name="hopper", email=None)
    assert round_trip_resume_or_resume_stream(u=r) == r


def test_streams_round_trip_resume_or_http_response():
    # Pass the `Resume` arm; the `baml.http.Response` arm isn't
    # host-constructible.
    r = Resume(name="lovelace", email="a@x.com")
    assert round_trip_resume_or_http_response(u=r) == r
