// Roundtrip coverage for baml_sdk::class_refs - class composition.
// Port of roundtrip_tests/test_class_refs.py.
#include <baml_sdk.h>
#include <baml_test.h>

using baml_sdk::class_refs::Inner;
using baml_sdk::class_refs::Outer;

BAML_TEST(class_refs_make_outer) {
  const Outer o = baml_sdk::class_refs::make_outer(5);
  BAML_ASSERT_EQ(o.inner.value, 5);
}

BAML_TEST(class_refs_round_trip_inner) {
  const Inner i{3};
  BAML_ASSERT(baml_sdk::class_refs::round_trip_inner(i) == i);
}

BAML_TEST(class_refs_round_trip_outer) {
  const Outer o{Inner{9}};
  BAML_ASSERT(baml_sdk::class_refs::round_trip_outer(o) == o);
}
