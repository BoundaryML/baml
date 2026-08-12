// Cross-namespace routing-rules coverage: root, a, a::b, lorem, ipsum.
// Port of roundtrip_tests/test_routing.py (the baml.http.Response round
// trips are post-step-8, as in Python).
#include <baml_sdk.h>
#include <baml_test.h>

using baml_sdk::Foo;
using baml_sdk::a::b::Thing;
using baml_sdk::lorem::Resume;

BAML_TEST(routing_make_foo) { BAML_ASSERT_EQ(baml_sdk::make_foo(3).v, 3); }

BAML_TEST(routing_round_trip_foo) {
  const Foo f{10};
  BAML_ASSERT(baml_sdk::round_trip_foo(f) == f);
}

BAML_TEST(routing_round_trip_thing_from_ab) {
  const Thing t{1};
  BAML_ASSERT(baml_sdk::a::b::round_trip_thing_from_ab(t) == t);
}

BAML_TEST(routing_round_trip_root_foo_from_ab) {
  const Foo f{2};
  BAML_ASSERT(baml_sdk::a::b::round_trip_root_foo_from_ab(f) == f);
}

BAML_TEST(routing_round_trip_deep_thing_from_a) {
  const Thing t{4};
  BAML_ASSERT(baml_sdk::a::round_trip_deep_thing_from_a(t) == t);
}

BAML_TEST(routing_round_trip_deep_thing_from_lorem) {
  const Thing t{5};
  BAML_ASSERT(baml_sdk::lorem::round_trip_deep_thing_from_lorem(t) == t);
}

BAML_TEST(routing_round_trip_resume) {
  const Resume r{"ada", std::nullopt};
  BAML_ASSERT(baml_sdk::lorem::round_trip_resume(r) == r);
}

BAML_TEST(routing_round_trip_root_foo) {
  const Foo f{6};
  BAML_ASSERT(baml_sdk::lorem::round_trip_root_foo(f) == f);
}

BAML_TEST(routing_round_trip_lorem_resume_from_ipsum) {
  const Resume r{"grace", std::string("g@x.com")};
  BAML_ASSERT(baml_sdk::ipsum::round_trip_lorem_resume_from_ipsum(r) == r);
}
