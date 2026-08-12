// Roundtrip coverage for the symbol-collision suite -- three distinct Bar
// classes at different namespace depths plus the consumers (Ipsum, Deep)
// that compose all three.
// Port of type_shapes/customizable/roundtrip_tests/test_symbol_collisions.py.
#include <baml_sdk.h>
#include <baml_test.h>

namespace sc = baml_sdk::symbol_collisions;

BAML_TEST(symbol_collisions_round_trip_foo_bar) {
  const auto bar = sc::foo::make_foo_bar("hi", 2);
  BAML_ASSERT(sc::foo::round_trip_foo_bar(bar) == bar);
}

BAML_TEST(symbol_collisions_round_trip_fizz_foo_bar) {
  const auto bar = sc::fizz::foo::make_fizz_foo_bar("t", 1.5);
  BAML_ASSERT(sc::fizz::foo::round_trip_fizz_foo_bar(bar) == bar);
}

BAML_TEST(symbol_collisions_round_trip_fizz_buzz_foo_bar) {
  const auto bar = sc::fizz::buzz::foo::make_fizz_buzz_foo_bar("f", 2.5, true);
  BAML_ASSERT(sc::fizz::buzz::foo::round_trip_fizz_buzz_foo_bar(bar) == bar);
}

BAML_TEST(symbol_collisions_round_trip_ipsum) {
  const auto ipsum = sc::lorem::make_ipsum(
      sc::foo::make_foo_bar("a", 1), sc::fizz::foo::make_fizz_foo_bar("b", 2.0),
      sc::fizz::buzz::foo::make_fizz_buzz_foo_bar("c", 3.0, false));
  BAML_ASSERT(sc::lorem::round_trip_ipsum(ipsum) == ipsum);
}

BAML_TEST(symbol_collisions_round_trip_deep) {
  const auto ipsum = sc::lorem::make_ipsum(
      sc::foo::make_foo_bar("a", 1), sc::fizz::foo::make_fizz_foo_bar("b", 2.0),
      sc::fizz::buzz::foo::make_fizz_buzz_foo_bar("c", 3.0, false));
  const auto deep = sc::a::b::c::d::make_deep(
      sc::foo::make_foo_bar("h", 9),
      sc::fizz::foo::make_fizz_foo_bar("th", 4.0),
      sc::fizz::buzz::foo::make_fizz_buzz_foo_bar("fu", 5.0, true), ipsum);
  BAML_ASSERT(sc::a::b::c::d::round_trip_deep(deep) == deep);
}
