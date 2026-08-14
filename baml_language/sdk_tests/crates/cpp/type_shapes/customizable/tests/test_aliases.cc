// Roundtrip coverage for the aliases suite.
// Port of type_shapes/customizable/roundtrip_tests/test_aliases.py.
// Non-recursive aliases emit `using` declarations; recursive aliases emit
// named wrapper structs whose self-references are boxed, so
// `RecList = int | RecList[]` is
// `struct RecList { baml::variant<int64_t, vector<Box<RecList>>> value; }`.
#include <baml_sdk.h>
#include <baml_test.h>

#include <string>
#include <type_traits>
#include <vector>

namespace aliases = baml_sdk::aliases;
using aliases::AliasContainer;
using aliases::MaybeRec;
using aliases::RecList;
using RecChildren = std::vector<baml::box<RecList>>;

BAML_TEST(aliases_round_trip_string_list) {
  // StringList is a generated `using` alias; the codegen pool preserves
  // alias identity at use sites, so the signature spells StringList too.
  static_assert(
      std::is_same<aliases::StringList, std::vector<std::string>>::value,
      "StringList must alias its resolved type");
  const aliases::StringList s = {"a", "b"};
  BAML_ASSERT(aliases::round_trip_string_list(s) == s);
}

BAML_TEST(aliases_round_trip_rec_list) {
  // RecList = int | RecList[]
  const RecList leaf{int64_t{1}};
  BAML_ASSERT(aliases::round_trip_rec_list(leaf) == leaf);

  // [1, [2, 3]]
  const RecList nested{RecChildren{
      baml::box<RecList>(RecList{int64_t{1}}),
      baml::box<RecList>(RecList{RecChildren{
          baml::box<RecList>(RecList{int64_t{2}}),
          baml::box<RecList>(RecList{int64_t{3}}),
      }}),
  }};
  BAML_ASSERT(aliases::round_trip_rec_list(nested) == nested);
}

BAML_TEST(aliases_round_trip_alias_container) {
  const AliasContainer c{
      {"x"},  // list_field: StringList
      RecList{RecChildren{
          baml::box<RecList>(RecList{int64_t{1}}),
          baml::box<RecList>(RecList{RecChildren{
              baml::box<RecList>(RecList{int64_t{2}}),
          }}),
      }},  // rec_field: [1, [2]]
  };
  BAML_ASSERT(aliases::round_trip_alias_container(c) == c);
}

BAML_TEST(aliases_round_trip_maybe_rec) {
  // C++-specific: a nullable recursive-alias reference folds the null into
  // the box (optional_box<RecList>; optional<Box<T>> needs a complete T).
  const MaybeRec none{{}};
  BAML_ASSERT(aliases::round_trip_maybe_rec(none) == none);
  const MaybeRec some{baml::optional_box<RecList>(RecList{int64_t{6}})};
  BAML_ASSERT(aliases::round_trip_maybe_rec(some) == some);
}
