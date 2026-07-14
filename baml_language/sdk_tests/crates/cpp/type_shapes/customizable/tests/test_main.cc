// Smoke tests for the type_shapes sdk-test crate. Python's version asserts
// each generated namespace imports cleanly; the C++ analog is that this
// translation unit compiles while naming a symbol from each namespace
// (the fixture's compile check covers full-header validity).
#include <baml_sdk.h>
#include <baml_test.h>

BAML_TEST(root_foo_reachable) {
  (void)sizeof(baml_sdk::Foo);
  BAML_ASSERT(true);
}

BAML_TEST(lorem_resume_reachable) {
  (void)sizeof(baml_sdk::lorem::Resume);
  BAML_ASSERT(true);
}

BAML_TEST(deep_namespace_thing_reachable) {
  (void)sizeof(baml_sdk::a::b::Thing);
  BAML_ASSERT(true);
}

BAML_TEST_MAIN()
