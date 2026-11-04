//! Roundtrip coverage for `baml_sdk::enums` — enums + EnumVariant-as-type.

use baml_sdk::enums::{
    Enums, Sentiment, pick_positive_sync, pick_sentiment_sync, round_trip_enums_sync,
    round_trip_sentiment_positive_sync, round_trip_sentiment_sync,
};

#[test]
fn test_enums_pick_sentiment() {
    assert_eq!(pick_sentiment_sync(true).unwrap(), Sentiment::Positive);
    assert_eq!(pick_sentiment_sync(false).unwrap(), Sentiment::Negative);
}

#[test]
fn test_enums_pick_positive() {
    assert_eq!(pick_positive_sync().unwrap(), Sentiment::Positive);
}

#[test]
fn test_enums_round_trip_sentiment() {
    assert_eq!(
        round_trip_sentiment_sync(Sentiment::Negative).unwrap(),
        Sentiment::Negative
    );
}

#[test]
fn test_enums_round_trip_sentiment_positive() {
    // EnumVariant-as-type: the variant tag is dropped during TIR→codegen,
    // so the Rust type is just `Sentiment`.
    assert_eq!(
        round_trip_sentiment_positive_sync(Sentiment::Positive).unwrap(),
        Sentiment::Positive
    );
}

#[test]
fn test_enums_round_trip_enums() {
    let e = Enums {
        bare_enum: Sentiment::Positive,
        variant_as_type: Sentiment::Positive,
    };
    assert_eq!(round_trip_enums_sync(e.clone()).unwrap(), e);
}
