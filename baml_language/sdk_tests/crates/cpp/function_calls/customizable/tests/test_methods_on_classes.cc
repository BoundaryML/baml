// Static + instance method coverage (ns_methods_on_classes.Greeter).
// Port of function_calls/customizable/test_methods_on_classes.py. Python's
// bindings-exist reflection test is compile-time in C++ (calling them below
// proves existence) and needs no runtime port.
#include <baml_sdk.h>
#include <baml_test.h>

using baml_sdk::methods_on_classes::Greeter;

BAML_TEST(methods_on_classes_static_create_round_trips) {
  const Greeter g = Greeter::create("ada");
  BAML_ASSERT_EQ(g.name, std::string("ada"));
}

BAML_TEST(methods_on_classes_static_create_async_round_trips) {
  const Greeter g = Greeter::create_async("grace").get();
  BAML_ASSERT_EQ(g.name, std::string("grace"));
}

BAML_TEST(methods_on_classes_instance_who_round_trips) {
  const Greeter g = Greeter::create("hopper");
  BAML_ASSERT_EQ(g.who(), std::string("hopper"));
}

BAML_TEST(methods_on_classes_instance_who_async_round_trips) {
  const Greeter g = Greeter::create_async("hopper").get();
  BAML_ASSERT_EQ(g.who_async().get(), std::string("hopper"));
}

BAML_TEST(methods_on_classes_instance_greet_with_arg_round_trips) {
  const Greeter g = Greeter::create("lovelace");
  BAML_ASSERT_EQ(g.greet("hi"), std::string("hi"));
}

BAML_TEST(methods_on_classes_instance_greet_async_with_arg_round_trips) {
  const Greeter g = Greeter::create_async("lovelace").get();
  BAML_ASSERT_EQ(g.greet_async("hi").get(), std::string("hi"));
}
