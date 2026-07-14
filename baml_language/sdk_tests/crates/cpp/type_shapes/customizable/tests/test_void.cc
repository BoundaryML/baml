// Roundtrip coverage for baml_sdk::void_ - void return lowers to C++ void.
// Port of roundtrip_tests/test_void.py.
#include <baml_sdk.h>
#include <baml_test.h>

BAML_TEST(no_op) {
  baml_sdk::void_::no_op();  // returns void; completing without throwing is the
                             // assertion
  baml_sdk::void_::no_op_async().get();
}
