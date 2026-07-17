// Roundtrip coverage for baml_sdk::literals - literal Ty variants, widened
// to their base types (spec parity with Python's Literal handling).
// Port of roundtrip_tests/test_literals.py.
#include <baml_sdk.h>
#include <baml_test.h>

using baml_sdk::literals::Literals;

BAML_TEST(return_literals) {
  BAML_ASSERT_EQ(baml_sdk::literals::return_literal42(), 42);
  BAML_ASSERT_EQ(baml_sdk::literals::return_literal_neg_one(), -1);
  BAML_ASSERT_EQ(baml_sdk::literals::return_literal_draft(),
                 std::string("draft"));
  BAML_ASSERT_EQ(baml_sdk::literals::return_literal_escaped(),
                 std::string("has \"quotes\""));
  BAML_ASSERT_EQ(baml_sdk::literals::return_literal_true(), true);
  BAML_ASSERT_EQ(baml_sdk::literals::return_literal_false(), false);
}

BAML_TEST(round_trip_literal42) {
  BAML_ASSERT_EQ(baml_sdk::literals::round_trip_literal42(42), 42);
}

BAML_TEST(round_trip_literal_draft) {
  BAML_ASSERT_EQ(baml_sdk::literals::round_trip_literal_draft("draft"),
                 std::string("draft"));
}

BAML_TEST(round_trip_literal_escaped) {
  BAML_ASSERT_EQ(
      baml_sdk::literals::round_trip_literal_escaped("has \"quotes\""),
      std::string("has \"quotes\""));
}

BAML_TEST(round_trip_literal_true) {
  BAML_ASSERT_EQ(baml_sdk::literals::round_trip_literal_true(true), true);
}

BAML_TEST(round_trip_literal_false) {
  BAML_ASSERT_EQ(baml_sdk::literals::round_trip_literal_false(false), false);
}

BAML_TEST(round_trip_literals) {
  const Literals lit{42, "draft", "has \"quotes\"", true, false};
  BAML_ASSERT(baml_sdk::literals::round_trip_literals(lit) == lit);
}
