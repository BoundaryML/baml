// Static + instance method coverage (ns_methods_on_classes.Greeter).
// Port of function_calls/customizable/test_methods_on_classes.py. Python's
// bindings-exist reflection test is compile-time in C++ (calling them below
// proves existence) and needs no runtime port.
#include <baml_sdk.hpp>
#include <baml_test.hpp>

using baml_sdk::methods_on_classes::Greeter;

BAML_TEST(static_create_round_trips) {
    const Greeter g = Greeter::create("ada");
    BAML_ASSERT_EQ(g.name, std::string("ada"));
}

BAML_TEST(static_create_async_round_trips) {
    const Greeter g = Greeter::create_async("grace").get();
    BAML_ASSERT_EQ(g.name, std::string("grace"));
}

BAML_TEST(instance_who_round_trips) {
    const Greeter g = Greeter::create("hopper");
    BAML_ASSERT_EQ(g.who(), std::string("hopper"));
}

BAML_TEST(instance_who_async_round_trips) {
    const Greeter g = Greeter::create_async("hopper").get();
    BAML_ASSERT_EQ(g.who_async().get(), std::string("hopper"));
}

BAML_TEST(instance_greet_with_arg_round_trips) {
    const Greeter g = Greeter::create("lovelace");
    BAML_ASSERT_EQ(g.greet("hi"), std::string("hi"));
}

BAML_TEST(instance_greet_async_with_arg_round_trips) {
    const Greeter g = Greeter::create_async("lovelace").get();
    BAML_ASSERT_EQ(g.greet_async("hi").get(), std::string("hi"));
}
