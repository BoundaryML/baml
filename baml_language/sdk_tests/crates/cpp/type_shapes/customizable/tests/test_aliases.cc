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

BAML_TEST(round_trip_string_list) {
  const std::vector<std::string> s = {"a", "b"};
  BAML_ASSERT(aliases::round_trip_string_list(s) == s);
}
