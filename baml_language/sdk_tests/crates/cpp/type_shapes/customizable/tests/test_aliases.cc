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
#include <type_traits>
#include <variant>
#include <vector>

namespace aliases = baml_sdk::aliases;

BAML_TEST(round_trip_string_list) {
  // StringList is a generated `using` alias; the codegen pool preserves
  // alias identity at use sites, so the signature spells StringList too.
  static_assert(
      std::is_same<aliases::StringList, std::vector<std::string>>::value,
      "StringList must alias its resolved type");
  const aliases::StringList s = {"a", "b"};
  BAML_ASSERT(aliases::round_trip_string_list(s) == s);
}
