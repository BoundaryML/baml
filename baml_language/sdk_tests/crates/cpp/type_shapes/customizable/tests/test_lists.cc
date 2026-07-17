// Roundtrip coverage for baml_sdk::lists - list Ty variants.
// Port of roundtrip_tests/test_lists.py.
#include <baml_sdk.h>
#include <baml_test.h>

BAML_TEST(round_trip_ints) {
  const std::vector<int64_t> xs{1, 2, 3};
  BAML_ASSERT(baml_sdk::lists::round_trip_ints(xs) == xs);
}

BAML_TEST(round_trip_empty_list) {
  // Regression for Bug A (35b): an empty list must encode as a present
  // (empty) list_value message, not an absent oneof the engine reads as
  // null. The C++ encoder writes the submessage unconditionally.
  BAML_ASSERT(baml_sdk::lists::round_trip_ints({}).empty());
}

BAML_TEST(round_trip_optional_strings) {
  const std::vector<std::optional<std::string>> xs{"a", std::nullopt, "b"};
  BAML_ASSERT(baml_sdk::lists::round_trip_optional_strings(xs) == xs);
}
