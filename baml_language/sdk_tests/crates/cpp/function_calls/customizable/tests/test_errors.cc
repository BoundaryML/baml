// Typed error-union delivery. Port of the union-relevant subset of
// function_calls/customizable/test_errors.py (panic / cancellation /
// os-exit / traceback-splicing coverage stays post-step-8):
// - a declared throw surfaces as BamlThrown<baml::Union<...>> carrying the
//   decoded value, readable with baml::match (the analog of Python's
//   BamlError.value), while catch(BamlError&) keeps working;
// - single-member and multi-member `throws` agree on class_name (the
//   engine wraps multi-member throws in union_variant_value);
// - the untyped is<T>()/get<T>() probes still work on the same exception.
#include <baml_sdk.h>
#include <baml_test.h>

#include <string>

using baml_sdk::baml::json::JsonParseError;
using baml_sdk::raises_test::ParseError;
using baml_sdk::raises_test::TimeoutError;
using baml_sdk::throws_test::MyError;

BAML_TEST(stdlib_error_surfaces_typed) {
  // `baml.json.parse` on bad input -> BamlThrown whose value decodes to a
  // JsonParseError (a plain generated struct). Proves stdlib error classes
  // surface structured, independent of any user `throws` clause.
  try {
    baml_sdk::throws_test::ParseJson("{not valid json");
    baml_test::Fail("ParseJson did not throw");
  } catch (const baml::BamlThrown<baml::Union<JsonParseError>>& e) {
    BAML_ASSERT(std::holds_alternative<JsonParseError>(e.value));
  }
}

BAML_TEST(user_throw_surfaces_declared_instance) {
  // A user throw of a declared error -> the declared user error instance
  // itself, typed.
  try {
    baml_sdk::throws_test::ThrowMyError();
    baml_test::Fail("ThrowMyError did not throw");
  } catch (const baml::BamlThrown<baml::Union<MyError>>& e) {
    const MyError got = baml::match(e.value,  //
                                    [](const MyError& m) { return m; });
    BAML_ASSERT((got == MyError{42, "boom"}));
    // The untyped probes still work on the same exception.
    BAML_ASSERT(e.is<MyError>());
    BAML_ASSERT(!e.is<ParseError>());
  }
}

BAML_TEST(union_throws_preserves_class_name) {
  // Single-member (Reparse: throws ParseError) and multi-member (LoadDoc:
  // throws ParseError | TimeoutError) must agree on class_name: the engine
  // wraps multi-member throws in union_variant_value, and the decoder must
  // still surface the thrown value's FQN.
  std::string single_name;
  try {
    baml_sdk::raises_test::Reparse("x");
    baml_test::Fail("Reparse did not throw");
  } catch (const baml::BamlThrown<baml::Union<ParseError>>& e) {
    single_name = e.class_name();
  }
  try {
    baml_sdk::raises_test::LoadDoc("x");
    baml_test::Fail("LoadDoc did not throw");
  } catch (const baml::BamlThrown<baml::Union<TimeoutError, ParseError>>& e) {
    // NOTE: the catch spells the union REVERSED from the declaration
    // (throws ParseError | TimeoutError) -- baml::Union is order-canonical,
    // so both spellings are the same catchable type.
    BAML_ASSERT_EQ(single_name, std::string("user.raises_test.ParseError"));
    BAML_ASSERT_EQ(e.class_name(), single_name);
    const bool is_parse = baml::match(
        e.value,  //
        [](const ParseError&) { return true; },
        [](const TimeoutError&) { return false; });
    BAML_ASSERT(is_parse);
  }
}

BAML_TEST(typed_throw_is_still_a_baml_error) {
  // Backward compatibility: an untyped catch site sees the same throw.
  try {
    baml_sdk::throws_test::ThrowMyError();
    baml_test::Fail("ThrowMyError did not throw");
  } catch (const baml::BamlError& e) {
    BAML_ASSERT_EQ(e.class_name(), std::string("user.throws_test.MyError"));
    BAML_ASSERT(e.is<MyError>());
    BAML_ASSERT((e.get<MyError>() == MyError{42, "boom"}));
  }
}
