// Smoke tests for plain (non-LLM) expression functions.
// Port of python_pydantic2/function_calls/customizable/test_main.py.
#include <baml_sdk.hpp>
#include <baml_test.hpp>

BAML_TEST(hello_world_returns_literal) {
    BAML_ASSERT_EQ(baml_sdk::hello_world(), std::string("hello world"));
}

BAML_TEST(single_required_arg_round_trips) {
    // The next step up from the nullary case: one required positional
    // argument round-trips through the engine unchanged.
    BAML_ASSERT_EQ(baml_sdk::single_required_arg("hi"), std::string("hi"));
}

BAML_TEST_MAIN()
