// Roundtrip coverage for baml_sdk::enums - enums + EnumVariant-as-type.
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/test_enums.py.
#include <baml_sdk.hpp>
#include <baml_test.hpp>

using baml_sdk::enums::Enums;
using baml_sdk::enums::Sentiment;

BAML_TEST(pick_sentiment) {
    BAML_ASSERT(baml_sdk::enums::pick_sentiment(true) == Sentiment::Positive);
    BAML_ASSERT(baml_sdk::enums::pick_sentiment(false) == Sentiment::Negative);
}

BAML_TEST(pick_positive) {
    BAML_ASSERT(baml_sdk::enums::pick_positive() == Sentiment::Positive);
}

BAML_TEST(round_trip_sentiment) {
    BAML_ASSERT(baml_sdk::enums::round_trip_sentiment(Sentiment::Negative) ==
                Sentiment::Negative);
}

BAML_TEST(round_trip_sentiment_positive) {
    // EnumVariant-as-type: the variant tag is dropped during TIR->codegen,
    // so the C++ type is just Sentiment.
    BAML_ASSERT(baml_sdk::enums::round_trip_sentiment_positive(Sentiment::Positive) ==
                Sentiment::Positive);
}

BAML_TEST(round_trip_enums) {
    const Enums e{Sentiment::Positive, Sentiment::Positive};
    BAML_ASSERT(baml_sdk::enums::round_trip_enums(e) == e);
}
