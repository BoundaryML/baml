// Roundtrip coverage for the forward-references suite.
// Port of type_shapes/customizable/roundtrip_tests/test_forward_refs.py.
//
// round_trip_node is intentionally NOT exercised: `class Node { next Node }`
// has a *required* (non-optional) self-reference, so no finite value can be
// constructed from the host side. It still emits and type-checks; the
// reference below proves the symbol exists.
#include <baml_sdk.h>
#include <baml_test.h>

#include <cstdint>
#include <variant>
#include <vector>

namespace forward_refs = baml_sdk::forward_refs;
using forward_refs::GNode;
using forward_refs::Other;

BAML_TEST(round_trip_other) {
  const Other o{7};
  BAML_ASSERT(forward_refs::round_trip_other(o) == o);
}

BAML_TEST(round_trip_node_symbol_exists) {
  // Uninhabitable (required self-ref); reference-only, like Python's
  // import-only assertion.
  (void)&forward_refs::round_trip_node;
}

BAML_TEST(round_trip_g_node_int) {
  // The leaf node carries children=[]; this exercises the empty-list
  // round trip.
  using Children = std::vector<::baml::Box<GNode<int64_t>>>;
  const GNode<int64_t> g{Children{GNode<int64_t>{Children{}}}};
  BAML_ASSERT(forward_refs::round_trip_g_node_int(g) == g);
}
