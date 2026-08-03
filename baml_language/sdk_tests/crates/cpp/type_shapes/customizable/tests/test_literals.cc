// Roundtrip coverage for baml_sdk::literals - literal Ty variants as
// singleton ::baml::lit types. Port of roundtrip_tests/test_literals.py:
// python keeps the value set in typing.Literal annotations; C++ makes each
// value a distinct type, so the round trips construct through the
// BAML_LIT macro family and read back through Lit's implicit value
// conversions.
#include <baml_sdk.h>
#include <baml_test.h>

#include <string>
#include <string_view>
#include <type_traits>
#include <variant>

using baml_sdk::literals::Literals;

// The generated surface is typed per value, not widened.
static_assert(std::is_same<decltype(baml_sdk::literals::return_literal42()),
                           BAML_LIT(42)>::value,
              "int literal return must be a Lit");
static_assert(std::is_same<decltype(baml_sdk::literals::return_literal_draft()),
                           BAML_LIT("draft")>::value,
              "string literal return must be a Lit");
static_assert(std::is_same<decltype(baml_sdk::literals::return_literal_true()),
                           BAML_LIT(true)>::value,
              "bool literal return must be a Lit");

BAML_TEST(literals_return_literals) {
  BAML_ASSERT(baml_sdk::literals::return_literal42() == BAML_LIT(42){});
  BAML_ASSERT(baml_sdk::literals::return_literal_neg_one() == BAML_LIT(-1){});
  BAML_ASSERT(baml_sdk::literals::return_literal_draft() ==
              BAML_LIT("draft"){});
  BAML_ASSERT(baml_sdk::literals::return_literal_escaped() ==
              BAML_LIT("has \"quotes\""){});
  BAML_ASSERT(baml_sdk::literals::return_literal_true() == BAML_LIT(true){});
  BAML_ASSERT(baml_sdk::literals::return_literal_false() == BAML_LIT(false){});
}

BAML_TEST(literals_literal_values_convert_implicitly) {
  const int64_t n = baml_sdk::literals::return_literal42();
  BAML_ASSERT(n == 42);
  const std::string_view s = baml_sdk::literals::return_literal_draft();
  BAML_ASSERT(s == "draft");
  const bool b = baml_sdk::literals::return_literal_true();
  BAML_ASSERT(b);
  BAML_ASSERT(BAML_LIT("has \"quotes\"")::value == "has \"quotes\"");
}

BAML_TEST(literals_round_trip_literal42) {
  BAML_ASSERT(baml_sdk::literals::round_trip_literal42(BAML_LIT(42){}) ==
              BAML_LIT(42){});
}

BAML_TEST(literals_round_trip_literal_draft) {
  BAML_ASSERT(baml_sdk::literals::round_trip_literal_draft(
                  BAML_LIT("draft"){}) == BAML_LIT("draft"){});
}

BAML_TEST(literals_round_trip_literal_escaped) {
  BAML_ASSERT(baml_sdk::literals::round_trip_literal_escaped(BAML_LIT(
                  "has \"quotes\""){}) == BAML_LIT("has \"quotes\""){});
}

BAML_TEST(literals_round_trip_literal_true) {
  BAML_ASSERT(baml_sdk::literals::round_trip_literal_true(BAML_LIT(true){}) ==
              BAML_LIT(true){});
}

BAML_TEST(literals_round_trip_literal_false) {
  BAML_ASSERT(baml_sdk::literals::round_trip_literal_false(BAML_LIT(false){}) ==
              BAML_LIT(false){});
}

BAML_TEST(literals_round_trip_literals) {
  const Literals lit{BAML_LIT(42){}, BAML_LIT("draft"){},
                     BAML_LIT("has \"quotes\""){}, BAML_LIT(true){},
                     BAML_LIT(false){}};
  BAML_ASSERT(baml_sdk::literals::round_trip_literals(lit) == lit);
}

BAML_TEST(literals_round_trip_flag_mixed_literal_union) {
  // Mixed-base literal union ("active" | 1 | true): the union codec's
  // literal pass must route each wire arm to its exact Lit alternative.
  using baml_sdk::literals::Flag;
  Flag f = baml_sdk::literals::round_trip_flag(BAML_LIT("active"){});
  BAML_ASSERT(std::holds_alternative<BAML_LIT("active")>(f));
  f = baml_sdk::literals::round_trip_flag(BAML_LIT(1){});
  BAML_ASSERT(std::holds_alternative<BAML_LIT(1)>(f));
  f = baml_sdk::literals::round_trip_flag(BAML_LIT(true){});
  const int which = baml::match(
      f,                                                                    //
      [](BAML_LIT("active")) { return 0; }, [](BAML_LIT(1)) { return 1; },  //
      [](BAML_LIT(true)) { return 2; });
  BAML_ASSERT(which == 2);
}
