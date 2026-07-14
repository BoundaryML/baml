// Roundtrip coverage for the generics suite -- generic classes over <int>.
// Port of type_shapes/customizable/roundtrip_tests/test_generics.py. The
// generic instance-method path is covered separately in test_generic.cpp;
// here we cover the concretely-instantiated generic class round trips.
//
// Recursive generic references are boxed in C++ (GenericLinkedList.next is
// OptionalBox, NestedGenerics.wr wraps Box<GenericLinkedList>), so the
// construction shapes differ from Python only in those Box spellings.
#include <baml_sdk.h>
#include <baml_test.h>

#include <cstdint>
#include <optional>
#include <vector>

namespace generics = baml_sdk::generics;
using generics::GenericBinaryTree;
using generics::GenericLinkedList;
using generics::Wrapper;

BAML_TEST(round_trip_wrapper_int) {
  const Wrapper<int64_t> w{5};
  BAML_ASSERT(generics::round_trip_wrapper_int(w) == w);
}

BAML_TEST(round_trip_generic_linked_list_int) {
  const GenericLinkedList<int64_t> ll{
      1, GenericLinkedList<int64_t>{
             2, ::baml::OptionalBox<GenericLinkedList<int64_t>>()}};
  BAML_ASSERT(generics::round_trip_generic_linked_list_int(ll) == ll);
}

BAML_TEST(round_trip_generic_binary_tree_int) {
  const GenericBinaryTree<int64_t> t{
      1, ::baml::OptionalBox<GenericBinaryTree<int64_t>>(),
      ::baml::OptionalBox<GenericBinaryTree<int64_t>>()};
  BAML_ASSERT(generics::round_trip_generic_binary_tree_int(t) == t);
}

BAML_TEST(round_trip_box_int) {
  const generics::Box<int64_t> b{3, Wrapper<int64_t>{4}};
  BAML_ASSERT(generics::round_trip_box_int(b) == b);
}

BAML_TEST(round_trip_nested_generics) {
  const generics::NestedGenerics n{
      Wrapper<Wrapper<int64_t>>{Wrapper<int64_t>{1}},
      Wrapper<std::vector<int64_t>>{{1, 2}},
      Wrapper<::baml::Box<GenericLinkedList<int64_t>>>{
          GenericLinkedList<int64_t>{
              9, ::baml::OptionalBox<GenericLinkedList<int64_t>>()}},
  };
  BAML_ASSERT(generics::round_trip_nested_generics(n) == n);
}

BAML_TEST(round_trip_differing_instantiation) {
  const generics::DifferingInstantiation d{GenericLinkedList<Wrapper<int64_t>>{
      Wrapper<int64_t>{1},
      ::baml::OptionalBox<GenericLinkedList<Wrapper<int64_t>>>()}};
  BAML_ASSERT(generics::round_trip_differing_instantiation(d) == d);
}
