// Roundtrip coverage for baml_sdk::optional - optional Ty variants.
// Port of roundtrip_tests/test_optional.py.
#include <baml_sdk.h>
#include <baml_test.h>

using baml_sdk::optional::OptionalContainer;
using baml_sdk::optional::Resume;

BAML_TEST(optional_round_trip_optional_int) {
  BAML_ASSERT(baml_sdk::optional::round_trip_optional_int(int64_t{5}) ==
              int64_t{5});
  BAML_ASSERT(
      !baml_sdk::optional::round_trip_optional_int(std::nullopt).has_value());
}

BAML_TEST(optional_round_trip_optional_resume) {
  const Resume r{"ada"};
  BAML_ASSERT(baml_sdk::optional::round_trip_optional_resume(r) == r);
  BAML_ASSERT(!baml_sdk::optional::round_trip_optional_resume(std::nullopt)
                   .has_value());
}

BAML_TEST(optional_round_trip_resume) {
  const Resume r{"grace"};
  BAML_ASSERT(baml_sdk::optional::round_trip_resume(r) == r);
}

BAML_TEST(optional_round_trip_optional_union) {
  using U = baml::variant<int64_t, std::string>;
  BAML_ASSERT(baml_sdk::optional::round_trip_optional_union(U{int64_t{3}}) ==
              U{int64_t{3}});
  BAML_ASSERT(baml_sdk::optional::round_trip_optional_union(
                  U{std::string("s")}) == U{std::string("s")});
  BAML_ASSERT(
      !baml_sdk::optional::round_trip_optional_union(std::nullopt).has_value());
}

BAML_TEST(optional_round_trip_optional_container) {
  const OptionalContainer c{
      std::nullopt,                                           // optional_int
      Resume{"x"},                                            // optional_class
      baml::variant<int64_t, std::string>{std::string("y")},  // optional_union
  };
  BAML_ASSERT(baml_sdk::optional::round_trip_optional_container(c) == c);
}
