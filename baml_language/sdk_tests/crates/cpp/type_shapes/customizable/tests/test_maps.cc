// Roundtrip coverage for baml_sdk::maps - map Ty variants.
// Port of roundtrip_tests/test_maps.py. Enum-keyed maps are dropped for the
// same outbound-schema reason recorded in the python file.
#include <baml_sdk.h>
#include <baml_test.h>

using baml_sdk::maps::Resume;
using baml_sdk::maps::Sentiment;

BAML_TEST(maps_round_trip_simple_map) {
  const std::unordered_map<std::string, int64_t> m{{"a", 1}, {"b", 2}};
  BAML_ASSERT(baml_sdk::maps::round_trip_simple_map(m) == m);
}

BAML_TEST(maps_round_trip_list_valued_map) {
  const std::unordered_map<std::string, std::vector<int64_t>> m{{"k", {1, 2}}};
  BAML_ASSERT(baml_sdk::maps::round_trip_list_valued_map(m) == m);
}

BAML_TEST(maps_round_trip_sentiment) {
  BAML_ASSERT(baml_sdk::maps::round_trip_sentiment(Sentiment::Positive) ==
              Sentiment::Positive);
}

BAML_TEST(maps_round_trip_resume) {
  const Resume r{"n"};
  BAML_ASSERT(baml_sdk::maps::round_trip_resume(r) == r);
}
