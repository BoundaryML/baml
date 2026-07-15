// Generic-instance-method FFI plumbing coverage. Port of
// type_shapes/customizable/test_generic.py. The C++ receiver's codec always
// writes class_ty.type_args (compile-time types), so the calls arrive fully
// bound - the very case Python defers pending outbound generic decoding.
#include <baml_sdk.h>
#include <baml_test.h>

using baml_sdk::generics::WrapperMarker;
using baml_sdk::generics::WrapperMethods;

BAML_TEST(generic) {
  // WrapperMethods<string>.get_value_or_marker(): class-level TypeVar
  // fused into a union with a concrete class (the Stream.next shape).
  const WrapperMethods<std::string> w =
      baml_sdk::generics::make_wrapper_methods("hello");
  const std::variant<WrapperMarker, std::string> got = w.get_value_or_marker();
  BAML_ASSERT(std::holds_alternative<std::string>(got));
  BAML_ASSERT_EQ(std::get<std::string>(got), std::string("hello"));
}

BAML_TEST(generic_wrapper_get_value) {
  // Plain TypeVar return on a generic receiver.
  const WrapperMethods<std::string> w =
      baml_sdk::generics::make_wrapper_methods("hello");
  BAML_ASSERT_EQ(w.get_value(), std::string("hello"));
}
