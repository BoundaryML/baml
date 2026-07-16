// Roundtrip coverage for the aliases suite.
// Port of type_shapes/customizable/roundtrip_tests/test_aliases.py.
// Non-recursive aliases emit `using` declarations; the fixture's recursive
// alias (RecList) has a union body, so it is skipped this slice.
#include <baml_sdk.h>
#include <baml_test.h>

#include <string>
#include <type_traits>
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
