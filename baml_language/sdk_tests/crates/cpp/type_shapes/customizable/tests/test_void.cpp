// Roundtrip coverage for baml_sdk::void_ - void return lowers to C++ void.
// Port of roundtrip_tests/test_void.py.
#include <baml_sdk.hpp>
#include <baml_test.hpp>

BAML_TEST(no_op) {
    baml_sdk::void_::no_op();  // returns void; completing without throwing is the assertion
    baml_sdk::void_::no_op_async().get();
}
