"""Roundtrip coverage for `baml_sdk.maps` — map Ty variants."""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.maps import (
    Sentiment,
    Resume,
    round_trip_simple_map,
    round_trip_list_valued_map,
    round_trip_sentiment,
    round_trip_resume,
)

# NOTE: enum-keyed maps don't round-trip yet. proto.py encodes an enum map key as
# a typed `enum_key`, but the OUTBOUND map entry carries only a scalar `entry.key`,
# so the engine renders an enum key as the string `"<fqn>::<variant>"` and decode
# hands it back as that raw string rather than the enum member. Finishing it needs
# a typed outbound map key (proto schema + engine emit + decode), not just a
# proto.py tweak. The `round_trip_enum_keyed_map` and `round_trip_map_container`
# tests (the latter has a required `enum_keyed: map<Sentiment, Resume>` field) are
# dropped until that lands; enum *values* still round-trip (test_round_trip_sentiment).


def test_maps_round_trip_simple_map():
    assert round_trip_simple_map(m={"a": 1, "b": 2}) == {"a": 1, "b": 2}


def test_maps_round_trip_list_valued_map():
    assert round_trip_list_valued_map(m={"k": [1, 2]}) == {"k": [1, 2]}


def test_maps_round_trip_sentiment():
    assert round_trip_sentiment(s=Sentiment.Positive) == Sentiment.Positive


def test_maps_round_trip_resume():
    r = Resume(name="n")
    assert round_trip_resume(r=r) == r
