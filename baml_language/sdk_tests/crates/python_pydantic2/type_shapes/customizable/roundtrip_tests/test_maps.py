"""Roundtrip coverage for `baml_sdk.maps` — map Ty variants."""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.maps import (
    Sentiment,
    Resume,
    MapContainer,
    round_trip_simple_map,
    round_trip_enum_keyed_map,
    round_trip_list_valued_map,
    round_trip_sentiment,
    round_trip_resume,
    round_trip_map_container,
)


def test_round_trip_simple_map():
    assert round_trip_simple_map(m={"a": 1, "b": 2}) == {"a": 1, "b": 2}


def test_round_trip_enum_keyed_map():
    m = {Sentiment.Positive: Resume(name="up")}
    assert round_trip_enum_keyed_map(m=m) == m


def test_round_trip_list_valued_map():
    assert round_trip_list_valued_map(m={"k": [1, 2]}) == {"k": [1, 2]}


def test_round_trip_sentiment():
    assert round_trip_sentiment(s=Sentiment.Positive) == Sentiment.Positive


def test_round_trip_resume():
    r = Resume(name="n")
    assert round_trip_resume(r=r) == r


def test_round_trip_map_container():
    c = MapContainer(
        simple={"a": 1},
        enum_keyed={Sentiment.Negative: Resume(name="dn")},
        list_valued={"k": [3]},
    )
    assert round_trip_map_container(c=c) == c
