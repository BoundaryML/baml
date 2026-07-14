// Roundtrip coverage for the aliases suite -- type aliases (incl. recursive).
// Port of type_shapes/customizable/roundtrip_tests/test_aliases.py.
//
// Non-recursive aliases resolve transparently (StringList is
// std::vector<std::string>); recursive aliases become named wrapper
// structs whose self-references are boxed, so `RecList = int | RecList[]`
// is `struct RecList { variant<int64_t, vector<Box<RecList>>> value; }`.
#include <baml_sdk.h>
#include <baml_test.h>

#include <cstdint>
#include <string>
#include <variant>
#include <vector>

namespace aliases = baml_sdk::aliases;
using aliases::RecList;
using RecItems = std::vector<::baml::Box<RecList>>;

BAML_TEST(round_trip_string_list) {
  const std::vector<std::string> s = {"a", "b"};
  BAML_ASSERT(aliases::round_trip_string_list(s) == s);
}

BAML_TEST(round_trip_rec_list) {
  // RecList = int | RecList[]
  const RecList leaf{int64_t{1}};
  BAML_ASSERT(aliases::round_trip_rec_list(leaf) == leaf);

  // [1, [2, 3]]
  const RecList nested{RecItems{
      RecList{int64_t{1}},
      RecList{RecItems{RecList{int64_t{2}}, RecList{int64_t{3}}}},
  }};
  BAML_ASSERT(aliases::round_trip_rec_list(nested) == nested);
}

BAML_TEST(round_trip_alias_container) {
  const aliases::AliasContainer c{
      {"x"},
      RecList{RecItems{RecList{int64_t{1}},
                       RecList{RecItems{RecList{int64_t{2}}}}}},
  };
  BAML_ASSERT(aliases::round_trip_alias_container(c) == c);
}
