// Roundtrip coverage for baml_sdk::recursion - recursive classes / SCCs.
// Port of roundtrip_tests/test_recursion.py. Recursive children are
// baml::optional_box<T>; finite values terminate with std::nullopt.
#include <baml_sdk.h>
#include <baml_test.h>

#include <cstddef>
#include <optional>
#include <string>
#include <utility>

namespace recursion = baml_sdk::recursion;
using recursion::A;
using recursion::B;
using recursion::IntBinaryTree;
using recursion::T1;
using recursion::T2;
using recursion::T3;
using recursion::T4;
using recursion::T5;
using recursion::T6;

namespace {

// A chain of `nodes` trees, each the left child of the next.
IntBinaryTree left_chain(std::size_t nodes) {
  IntBinaryTree tree{0, std::nullopt, std::nullopt};
  for (std::size_t i = 1; i < nodes; ++i) {
    tree =
        IntBinaryTree{static_cast<int64_t>(i), std::move(tree), std::nullopt};
  }
  return tree;
}

}  // namespace

BAML_TEST(recursion_round_trip_int_binary_tree) {
  const IntBinaryTree t{
      1,
      IntBinaryTree{2, std::nullopt, std::nullopt},
      std::nullopt,
  };
  BAML_ASSERT(recursion::round_trip_int_binary_tree(t) == t);
}

BAML_TEST(recursion_round_trip_mutual_recursion) {
  const A a{B{std::nullopt}};
  const B b{A{std::nullopt}};
  BAML_ASSERT(recursion::round_trip_a(a) == a);
  BAML_ASSERT(recursion::round_trip_b(b) == b);
}

BAML_TEST(recursion_round_trip_scc_t1_t2_t3) {
  const T1 t1{T2{std::nullopt, std::nullopt}, std::nullopt};
  const T2 t2{std::nullopt, T3{std::nullopt, std::nullopt}};
  const T3 t3{std::nullopt, std::nullopt};
  BAML_ASSERT(recursion::round_trip_t1(t1) == t1);
  BAML_ASSERT(recursion::round_trip_t2(t2) == t2);
  BAML_ASSERT(recursion::round_trip_t3(t3) == t3);
}

BAML_TEST(recursion_round_trip_scc_t4_t5_t6) {
  const T4 t4{T5{std::nullopt, std::nullopt}, std::nullopt};
  const T5 t5{std::nullopt, T6{std::nullopt, std::nullopt}};
  const T6 t6{std::nullopt, std::nullopt};
  BAML_ASSERT(recursion::round_trip_t4(t4) == t4);
  BAML_ASSERT(recursion::round_trip_t5(t5) == t5);
  BAML_ASSERT(recursion::round_trip_t6(t6) == t6);
}

BAML_TEST(recursion_int_binary_tree_deeper_than_the_wire_is_refused) {
  // A class level costs the wire three of the 100 nested messages it carries
  // (the class, its field's entry, the field's value), and an argument sits
  // two messages below the call: a chain of 32 trees crosses, and 33 is
  // refused before the call with the SDK's argument error, never a panic.
  const IntBinaryTree at_the_limit = left_chain(32);
  BAML_ASSERT(recursion::round_trip_int_binary_tree(at_the_limit) ==
              at_the_limit);
  for (const std::size_t nodes : {std::size_t{33}, std::size_t{1000}}) {
    bool refused = false;
    try {
      (void)recursion::round_trip_int_binary_tree(left_chain(nodes));
    } catch (const baml::panic& e) {
      baml_test::fail(
          std::string("expected the argument refusal, got a panic: ") +
          e.what());
    } catch (const baml::error& e) {
      refused =
          e.message().find("deeper than the 100 nested messages the wire") !=
          std::string::npos;
      if (!refused)
        baml_test::fail(std::string("expected the argument refusal, got: ") +
                        e.what());
    }
    BAML_ASSERT(refused);
  }
}
