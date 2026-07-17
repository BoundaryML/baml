// Roundtrip coverage for baml_sdk::unions - union normalization variants.
// Port of roundtrip_tests/test_unions.py, plus the C++-only canonical-form
// guarantees: baml::Union is an order-canonical std::variant alias
// (variant<A, B> and variant<B, A> are the SAME type, matching BAML's
// set-semantic unions), readable with baml::Match and every std::variant
// API.
#include <baml_sdk.h>
#include <baml_test.h>

#include <string>
#include <type_traits>
#include <variant>

using baml_sdk::unions::T;
using baml_sdk::unions::UnionContainer;
using IntOrString = baml::Union<int64_t, std::string>;

// Canonicalization: both spellings resolve to one instantiation, singles
// collapse to plain variants, and the generated field type is reachable
// from either spelling.
static_assert(std::is_same<baml::Union<int64_t, std::string>,
                           baml::Union<std::string, int64_t>>::value,
              "baml::Union must be order-canonical");
static_assert(std::is_same<baml::Union<T, std::string>,
                           baml::Union<std::string, T>>::value,
              "baml::Union must be order-canonical for generated types");
static_assert(std::is_same<baml::Union<int64_t>, std::variant<int64_t>>::value,
              "a one-alternative baml::Union is a plain std::variant");

BAML_TEST(round_trip_null_to_end) {
  using U = std::optional<IntOrString>;
  BAML_ASSERT(baml_sdk::unions::round_trip_null_to_end(
                  U{IntOrString{int64_t{1}}}) == U{IntOrString{int64_t{1}}});
  BAML_ASSERT(baml_sdk::unions::round_trip_null_to_end(U{IntOrString{
                  std::string("s")}}) == U{IntOrString{std::string("s")}});
  BAML_ASSERT(
      !baml_sdk::unions::round_trip_null_to_end(std::nullopt).has_value());
}

BAML_TEST(round_trip_dedup) {
  BAML_ASSERT(baml_sdk::unions::round_trip_dedup(IntOrString{int64_t{2}}) ==
              IntOrString{int64_t{2}});
  BAML_ASSERT(baml_sdk::unions::round_trip_dedup(IntOrString{
                  std::string("x")}) == IntOrString{std::string("x")});
}

BAML_TEST(round_trip_singleton_unwrap) {
  // `int | int` collapses to plain int64_t.
  BAML_ASSERT_EQ(baml_sdk::unions::round_trip_singleton_unwrap(7), int64_t{7});
}

BAML_TEST(round_trip_optional_plus_null) {
  using TOrString = baml::Union<T, std::string>;
  using U = std::optional<TOrString>;
  BAML_ASSERT(baml_sdk::unions::round_trip_optional_plus_null(
                  U{TOrString{T{1}}}) == U{TOrString{T{1}}});
  BAML_ASSERT(baml_sdk::unions::round_trip_optional_plus_null(U{TOrString{
                  std::string("s")}}) == U{TOrString{std::string("s")}});
  BAML_ASSERT(!baml_sdk::unions::round_trip_optional_plus_null(std::nullopt)
                   .has_value());
}

BAML_TEST(round_trip_t) {
  BAML_ASSERT(baml_sdk::unions::round_trip_t(T{4}) == T{4});
}

BAML_TEST(round_trip_union_container) {
  const UnionContainer c{
      std::nullopt,                       // null_to_end
      IntOrString{std::string("d")},      // dedup
      5,                                  // singleton_unwrap
      baml::Union<std::string, T>{T{2}},  // optional_plus_null:
                                          // reversed spelling on purpose
  };
  BAML_ASSERT(baml_sdk::unions::round_trip_union_container(c) == c);
}

BAML_TEST(match_dispatches_by_type) {
  const IntOrString got =
      baml_sdk::unions::round_trip_dedup(IntOrString{int64_t{21}});
  const int64_t doubled = baml::Match(
      got,  //
      [](int64_t i) { return i * 2; },
      [](const std::string& s) { return static_cast<int64_t>(s.size()); });
  BAML_ASSERT_EQ(doubled, int64_t{42});

  const IntOrString str =
      baml_sdk::unions::round_trip_dedup(IntOrString{std::string("hello")});
  const int64_t len = baml::Match(
      str,  //
      [](const std::string& s) { return static_cast<int64_t>(s.size()); },
      [](const auto&) { return int64_t{-1}; });
  BAML_ASSERT_EQ(len, int64_t{5});
}

BAML_TEST(union_is_a_plain_std_variant) {
  IntOrString u = baml_sdk::unions::round_trip_dedup(IntOrString{int64_t{9}});
  BAML_ASSERT(std::holds_alternative<int64_t>(u));
  BAML_ASSERT_EQ(std::get<int64_t>(u), int64_t{9});
  // Assign across spellings: same type, plain copy.
  baml::Union<std::string, int64_t> v = u;
  BAML_ASSERT_EQ(std::get<int64_t>(v), int64_t{9});
}
