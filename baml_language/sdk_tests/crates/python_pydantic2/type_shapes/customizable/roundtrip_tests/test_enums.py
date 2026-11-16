"""Roundtrip coverage for `baml_sdk.enums` — enums + EnumVariant-as-type."""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.enums import (
    Sentiment,
    Enums,
    pick_sentiment_sync,
    pick_positive_sync,
    round_trip_sentiment_sync,
    round_trip_sentiment_positive_sync,
    round_trip_enums_sync,
)


def test_enums_pick_sentiment():
    assert pick_sentiment_sync(b=True) == Sentiment.Positive
    assert pick_sentiment_sync(b=False) == Sentiment.Negative


def test_enums_pick_positive():
    assert pick_positive_sync() == Sentiment.Positive


def test_enums_round_trip_sentiment():
    assert round_trip_sentiment_sync(s=Sentiment.Negative) == Sentiment.Negative


def test_enums_round_trip_sentiment_positive():
    # EnumVariant-as-type: the variant tag is dropped during TIR→codegen,
    # so the Python type is just `Sentiment`.
    assert round_trip_sentiment_positive_sync(s=Sentiment.Positive) == Sentiment.Positive


def test_enums_round_trip_enums():
    e = Enums(bare_enum=Sentiment.Positive, variant_as_type=Sentiment.Positive)
    assert round_trip_enums_sync(e=e) == e
