// Roundtrip coverage for baml_sdk::lists - list Ty variants.
// Port of roundtrip_tests/test_lists.py.
#include <baml_sdk.h>
#include <baml_test.h>

using baml_sdk::lists::ListContainer;
using IntOrString = baml::variant<int64_t, std::string>;

BAML_TEST(lists_round_trip_ints) {
  const std::vector<int64_t> xs{1, 2, 3};
  BAML_ASSERT(baml_sdk::lists::round_trip_ints(xs) == xs);
}

BAML_TEST(lists_round_trip_empty_list) {
  // Regression for Bug A (35b): an empty list must encode as a present
  // (empty) list_value message, not an absent oneof the engine reads as
  // null. The C++ encoder writes the submessage unconditionally.
  BAML_ASSERT(baml_sdk::lists::round_trip_ints({}).empty());
}

BAML_TEST(lists_round_trip_optional_strings) {
  const std::vector<std::optional<std::string>> xs{"a", std::nullopt, "b"};
  BAML_ASSERT(baml_sdk::lists::round_trip_optional_strings(xs) == xs);
}

BAML_TEST(lists_round_trip_union_list) {
  const std::vector<IntOrString> xs{int64_t{1}, std::string("two"), int64_t{3}};
  BAML_ASSERT(baml_sdk::lists::round_trip_union_list(xs) == xs);
}

BAML_TEST(lists_round_trip_list_container) {
  const ListContainer c{
      {1, 2},                            // ints
      {std::nullopt, std::string("z")},  // optional_strings
      {IntOrString{int64_t{1}}, IntOrString{std::string("x")}},  // union_list
  };
  BAML_ASSERT(baml_sdk::lists::round_trip_list_container(c) == c);
}
