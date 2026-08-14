// Smoke tests for the type_shapes sdk-test crate. Python's version asserts
// each generated namespace imports cleanly; the C++ analog names a symbol
// from every in-scope generated namespace so a codegen regression that
// drops one fails this translation unit. Namespaces from removed
// post-step-8 features (media, generics, aliases_consumer) do not emit and
// are not named here.
#include <baml_sdk.h>
#include <baml_test.h>

BAML_TEST(main_root_foo_reachable) {
  (void)sizeof(baml_sdk::Foo);
  BAML_ASSERT(true);
}

BAML_TEST(main_lorem_resume_reachable) {
  (void)sizeof(baml_sdk::lorem::Resume);
  BAML_ASSERT(true);
}

BAML_TEST(main_deep_namespace_thing_reachable) {
  (void)sizeof(baml_sdk::a::b::Thing);
  BAML_ASSERT(true);
}

BAML_TEST(main_all_namespaces_reachable) {
  // One symbol per in-scope namespace (python parity:
  // test_all_namespaces_reachable). Sibling test files exercise most of
  // these behaviorally; naming them here pins pure existence, which the
  // fixture's compile check alone cannot (a dropped namespace still yields
  // a valid header).
  (void)sizeof(baml_sdk::primitives::Primitives);
  (void)sizeof(baml_sdk::enums::Enums);
  (void)sizeof(baml_sdk::literals::Literals);
  (void)sizeof(baml_sdk::class_refs::Inner);
  (void)sizeof(baml_sdk::aliases::StringList);
  (void)sizeof(baml_sdk::optional::Resume);
  (void)&baml_sdk::lists::round_trip_ints;
  (void)sizeof(baml_sdk::maps::Resume);
  (void)sizeof(baml_sdk::unions::T);
  (void)sizeof(baml_sdk::recursion::A);
  (void)sizeof(baml_sdk::forward_refs::Node);
  (void)sizeof(baml_sdk::complex_models::AuditEvent);
  BAML_ASSERT(true);
}

BAML_TEST_MAIN()
