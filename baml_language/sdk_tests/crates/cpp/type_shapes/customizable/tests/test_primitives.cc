// Roundtrip coverage for the primitives suite.
// Port of type_shapes/customizable/roundtrip_tests/test_primitives.py.
// return_* functions exercise decode-only; round_trip_* exercise the full
// encode/decode pair.
//
// Deviation from the Python file: round_trip_float_accepts_int has no
// boundary behavior to pin in C++ -- an integer argument converts to
// double in the caller, before the wire -- so it just documents the same
// value outcome.
#include <baml_sdk.h>
#include <baml_test.h>

#include <cstdint>
#include <string>
#include <variant>
#include <vector>

namespace primitives = baml_sdk::primitives;

BAML_TEST(primitives_return_int) {
  BAML_ASSERT_EQ(primitives::return_int(), int64_t{42});
}

BAML_TEST(primitives_return_float) {
  BAML_ASSERT_EQ(primitives::return_float(), 3.14);
}

BAML_TEST(primitives_return_string) {
  BAML_ASSERT_EQ(primitives::return_string(), std::string("hello"));
}

BAML_TEST(primitives_return_bool) {
  BAML_ASSERT(primitives::return_bool() == true);
}

BAML_TEST(primitives_return_null) { (void)primitives::return_null(); }

BAML_TEST(primitives_round_trip_int) {
  BAML_ASSERT_EQ(primitives::round_trip_int(7), int64_t{7});
}

BAML_TEST(primitives_round_trip_float) {
  BAML_ASSERT_EQ(primitives::round_trip_float(2.5), 2.5);
}

BAML_TEST(primitives_round_trip_float_accepts_int) {
  BAML_ASSERT_EQ(primitives::round_trip_float(7), 7.0);
}

BAML_TEST(primitives_round_trip_string) {
  BAML_ASSERT_EQ(primitives::round_trip_string("hi"), std::string("hi"));
}

BAML_TEST(primitives_round_trip_bool) {
  BAML_ASSERT(primitives::round_trip_bool(false) == false);
}

BAML_TEST(primitives_round_trip_null) {
  (void)primitives::round_trip_null(std::monostate{});
}

BAML_TEST(primitives_round_trip_uint8_array) {
  const std::vector<uint8_t> bytes = {0x00, 0x01, 0x02};
  BAML_ASSERT(primitives::round_trip_uint8_array(bytes) == bytes);
}

BAML_TEST(primitives_round_trip_primitives) {
  const primitives::Primitives p{
      1, 1.5, "s", true, std::monostate{}, std::vector<uint8_t>{'a', 'b'}};
  BAML_ASSERT(primitives::round_trip_primitives(p) == p);
}

BAML_TEST(primitives_round_trip_primitives_float_field_accepts_int) {
  // An integer into the float field converts at construction in C++, so
  // it reaches the wire as a float already -- same contract as Python's
  // pydantic coercion.
  const primitives::Primitives p{
      1, 2, "s", true, std::monostate{}, std::vector<uint8_t>{'a', 'b'}};
  BAML_ASSERT_EQ(primitives::round_trip_primitives(p).float_field, 2.0);
}
