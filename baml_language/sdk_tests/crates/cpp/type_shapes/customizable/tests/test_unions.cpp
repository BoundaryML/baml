// Roundtrip coverage for baml_sdk::unions - union normalization variants.
// Port of roundtrip_tests/test_unions.py.
#include <baml_sdk.hpp>
#include <baml_test.hpp>

using baml_sdk::unions::T;
using baml_sdk::unions::UnionContainer;
using IntOrString = std::variant<int64_t, std::string>;

BAML_TEST(round_trip_null_to_end) {
    using U = std::optional<IntOrString>;
    BAML_ASSERT(baml_sdk::unions::round_trip_null_to_end(U{IntOrString{int64_t{1}}}) ==
                U{IntOrString{int64_t{1}}});
    BAML_ASSERT(baml_sdk::unions::round_trip_null_to_end(U{IntOrString{std::string("s")}}) ==
                U{IntOrString{std::string("s")}});
    BAML_ASSERT(!baml_sdk::unions::round_trip_null_to_end(std::nullopt).has_value());
}

BAML_TEST(round_trip_dedup) {
    BAML_ASSERT(baml_sdk::unions::round_trip_dedup(IntOrString{int64_t{2}}) ==
                IntOrString{int64_t{2}});
    BAML_ASSERT(baml_sdk::unions::round_trip_dedup(IntOrString{std::string("x")}) ==
                IntOrString{std::string("x")});
}

BAML_TEST(round_trip_singleton_unwrap) {
    // `int | int` collapses to plain int64_t.
    BAML_ASSERT_EQ(baml_sdk::unions::round_trip_singleton_unwrap(7), int64_t{7});
}

BAML_TEST(round_trip_optional_plus_null) {
    using TOrString = std::variant<T, std::string>;
    using U = std::optional<TOrString>;
    BAML_ASSERT(baml_sdk::unions::round_trip_optional_plus_null(U{TOrString{T{1}}}) ==
                U{TOrString{T{1}}});
    BAML_ASSERT(baml_sdk::unions::round_trip_optional_plus_null(U{TOrString{std::string("s")}}) ==
                U{TOrString{std::string("s")}});
    BAML_ASSERT(!baml_sdk::unions::round_trip_optional_plus_null(std::nullopt).has_value());
}

BAML_TEST(round_trip_t) {
    BAML_ASSERT(baml_sdk::unions::round_trip_t(T{4}) == T{4});
}

BAML_TEST(round_trip_union_container) {
    const UnionContainer c{
        std::nullopt,                                       // null_to_end
        IntOrString{std::string("d")},                      // dedup
        5,                                                  // singleton_unwrap
        std::variant<T, std::string>{T{2}},                 // optional_plus_null
    };
    BAML_ASSERT(baml_sdk::unions::round_trip_union_container(c) == c);
}
