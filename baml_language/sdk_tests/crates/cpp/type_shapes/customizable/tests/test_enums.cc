// Roundtrip coverage for baml_sdk::enums - enums + EnumVariant-as-type.
// Port of
// python_pydantic2/type_shapes/customizable/roundtrip_tests/test_enums.py.
#include <baml_sdk.h>
#include <baml_test.h>

using baml_sdk::enums::Enums;
using baml_sdk::enums::Sentiment;

BAML_TEST(enums_pick_sentiment) {
  BAML_ASSERT(baml_sdk::enums::pick_sentiment(true) == Sentiment::Positive);
  BAML_ASSERT(baml_sdk::enums::pick_sentiment(false) == Sentiment::Negative);
}

BAML_TEST(enums_pick_positive) {
  BAML_ASSERT(baml_sdk::enums::pick_positive() == Sentiment::Positive);
}

BAML_TEST(enums_round_trip_sentiment) {
  BAML_ASSERT(baml_sdk::enums::round_trip_sentiment(Sentiment::Negative) ==
              Sentiment::Negative);
}

BAML_TEST(enums_round_trip_sentiment_positive) {
  // EnumVariant-as-type is a singleton Lit: only the tagged variant fits,
  // and the value converts back to the enum implicitly.
  const Sentiment round_tripped =
      baml_sdk::enums::round_trip_sentiment_positive(
          BAML_LIT(Sentiment::Positive){});
  BAML_ASSERT(round_tripped == Sentiment::Positive);
}

BAML_TEST(enums_round_trip_enums) {
  const Enums e{Sentiment::Positive, BAML_LIT(Sentiment::Positive){}};
  BAML_ASSERT(baml_sdk::enums::round_trip_enums(e) == e);
}
