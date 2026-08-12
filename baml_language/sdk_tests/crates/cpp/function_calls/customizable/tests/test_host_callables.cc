// End-to-end tests for the host-callable round trip.
// Port of function_calls/customizable/test_host_callables.py. The bridge
// registers a std::function in its host-value registry and emits a
// Handle{HOST_VALUE_CALLABLE} wire entry; the engine binds it to an
// Object::HostClosure; when BAML invokes it the call_host_value sysop
// fires the dispatch trampoline, which runs the callable on a detached
// thread and completes the call.
//
// Deviations from the Python file: the async-callable tests have no C++
// analog (there is no coroutine callable; every std::function is the sync
// path); the release/weakref test is xfail in Python (engine GC heuristic)
// and has no C++ observation point, so it is not ported; Python's
// `raise error(ValidationError(...))` is baml::host_throw<ValidationError>;
// exception identity (`raised is caught`) is observed as same-object
// rehydration -- the caught exception preserves the original's dynamic
// type, message, and custom fields.
#include <baml_sdk.h>
#include <baml_test.h>

#include <exception>
#include <string>
#include <vector>

using baml_sdk::host_callable_tests::Person;
using baml_sdk::host_callable_tests::ValidationError;

BAML_TEST(host_callables_simple_sync_callable_returns_string) {
  const std::string result = baml_sdk::host_callable_tests::call_with_callback(
      [](int64_t x) { return "got " + std::to_string(x); }, 5);
  BAML_ASSERT_EQ(result, std::string("got 5"));
}

BAML_TEST(host_callables_two_arg_callable_unpacks_positional_args) {
  const std::string result = baml_sdk::host_callable_tests::call_with_two_args(
      [](int64_t x, std::string prefix) {
        return prefix + ":" + std::to_string(x);
      },
      7, "answer");
  BAML_ASSERT_EQ(result, std::string("answer:7"));
}

BAML_TEST(host_callables_int_return_callable_round_trip) {
  const int64_t result = baml_sdk::host_callable_tests::call_int_callback(
      [](int64_t x) { return x * 2; }, 21);
  BAML_ASSERT_EQ(result, int64_t{42});
}

BAML_TEST(baml_closure_is_a_native_callable_with_host_language_arguments) {
  const auto add_ten = baml_sdk::host_callable_tests::make_adder(10);
  BAML_ASSERT_EQ(add_ten(5), int64_t{15});
  BAML_ASSERT_EQ(add_ten(7), int64_t{17});
}

BAML_TEST(baml_closure_decodes_multiple_args_and_structured_return_values) {
  const auto build = baml_sdk::host_callable_tests::make_pair_builder(30);
  const Person ada = build(12, "Ada");
  BAML_ASSERT_EQ(ada.name, std::string("Ada"));
  BAML_ASSERT_EQ(ada.age, int64_t{42});
  const Person grace = build(5, "Grace");
  BAML_ASSERT_EQ(grace.name, std::string("Grace"));
  BAML_ASSERT_EQ(grace.age, int64_t{35});
}

BAML_TEST(baml_closure_is_reusable_and_retains_mutable_captures) {
  const auto next_value = baml_sdk::host_callable_tests::make_counter(40);
  BAML_ASSERT_EQ(next_value(), int64_t{41});
  BAML_ASSERT_EQ(next_value(), int64_t{42});
}

BAML_TEST(
    host_callables_throwing_callable_round_trips_original_host_exception) {
  // A native C++ exception thrown inside a host callable surfaces back to
  // the caller as the original exception (registry rehydration), not
  // flattened into a error(HostCallable(...)) wrapper.
  bool threw = false;
  try {
    baml_sdk::host_callable_tests::call_with_callback(
        [](int64_t) -> std::string { throw std::runtime_error("nope"); }, 1);
    baml_test::fail("call_with_callback did not throw");
  } catch (const std::runtime_error& e) {
    threw = true;
    BAML_ASSERT_EQ(std::string(e.what()), std::string("nope"));
  }
  BAML_ASSERT(threw);
}

BAML_TEST(
    host_callables_throwing_callable_out_of_range_round_trips_with_identity) {
  // The native-exception rehydration path is class-agnostic: any
  // exception type round-trips, not just std::runtime_error.
  bool threw = false;
  try {
    baml_sdk::host_callable_tests::call_with_callback(
        [](int64_t) -> std::string { throw std::out_of_range("missing"); }, 1);
    baml_test::fail("call_with_callback did not throw");
  } catch (const std::out_of_range& e) {
    threw = true;
    BAML_ASSERT_EQ(std::string(e.what()), std::string("missing"));
  }
  BAML_ASSERT(threw);
}

namespace {
class MyDomainError : public std::exception {
 public:
  MyDomainError(std::string message, int code)
      : message_(std::move(message)), code(code) {}
  const char* what() const noexcept override { return message_.c_str(); }

  int code;

 private:
  std::string message_;
};
}  // namespace

BAML_TEST(
    host_callables_throwing_callable_custom_host_exception_round_trips_with_identity) {
  // A user-defined exception subclass round-trips with its custom state
  // intact -- the same object comes back, so `code` survives even though
  // the bridge never learned the concrete type.
  bool threw = false;
  try {
    baml_sdk::host_callable_tests::call_with_callback(
        [](int64_t) -> std::string {
          throw MyDomainError("custom domain failure", 42);
        },
        1);
    baml_test::fail("call_with_callback did not throw");
  } catch (const MyDomainError& e) {
    threw = true;
    BAML_ASSERT_EQ(std::string(e.what()), std::string("custom domain failure"));
    BAML_ASSERT_EQ(e.code, 42);
  }
  BAML_ASSERT(threw);
}

BAML_TEST(
    host_callables_throwing_callable_hostthrow_codegenned_class_is_caught_in_baml) {
  // A baml::host_throw<ValidationError> crosses as the real BAML class on
  // the wire, so the BAML side's typed `catch (e: ValidationError)`
  // matches structurally and reads e.message as a real field.
  const std::string result =
      baml_sdk::host_callable_tests::call_with_typed_throws(
          [](int64_t) -> std::string {
            throw baml::host_throw<ValidationError>(ValidationError{
                4, "bad shape", {"name", "age", "email", "phone"}});
          },
          1);
  BAML_ASSERT_EQ(result, std::string("caught: bad shape"));
}

BAML_TEST(
    host_callables_throwing_callable_hostthrow_propagates_back_with_typed_fields) {
  // The same host_throw<ValidationError>, when not caught in BAML,
  // propagates back out as a error whose payload decodes to the
  // typed ValidationError with all fields preserved.
  bool threw = false;
  try {
    baml_sdk::host_callable_tests::call_with_typed_throws_propagating(
        [](int64_t) -> std::string {
          throw baml::host_throw<ValidationError>(
              ValidationError{7, "propagated through", {"x", "y"}});
        },
        1);
    baml_test::fail("call_with_typed_throws_propagating did not throw");
  } catch (const baml::error& e) {
    threw = true;
    BAML_ASSERT(e.is<ValidationError>());
    const ValidationError decoded = e.get<ValidationError>();
    BAML_ASSERT_EQ(decoded.code, int64_t{7});
    BAML_ASSERT_EQ(decoded.message, std::string("propagated through"));
    BAML_ASSERT((decoded.fields == std::vector<std::string>{"x", "y"}));
  }
  BAML_ASSERT(threw);
}

BAML_TEST(host_callables_multiple_throws_in_flight_do_not_collide_in_registry) {
  // Each host throw mints a fresh host-value key; calls in quick
  // succession must not see the wrong original exception.
  bool first_threw = false;
  try {
    baml_sdk::host_callable_tests::call_with_callback(
        [](int64_t) -> std::string { throw std::runtime_error("first"); }, 1);
  } catch (const std::runtime_error& e) {
    first_threw = true;
    BAML_ASSERT_EQ(std::string(e.what()), std::string("first"));
  }
  bool second_threw = false;
  try {
    baml_sdk::host_callable_tests::call_with_callback(
        [](int64_t) -> std::string { throw std::runtime_error("second"); }, 2);
  } catch (const std::runtime_error& e) {
    second_threw = true;
    BAML_ASSERT_EQ(std::string(e.what()), std::string("second"));
  }
  BAML_ASSERT(first_threw && second_threw);
}

BAML_TEST(host_callables_lambda_round_trip) {
  // A capturing lambda exercises the callable-encoding branch with
  // closure state.
  const std::string tag = "lambda";
  const std::string result = baml_sdk::host_callable_tests::call_with_callback(
      [tag](int64_t x) { return tag + "-" + std::to_string(x); }, 99);
  BAML_ASSERT_EQ(result, std::string("lambda-99"));
}

BAML_TEST(host_callables_multiple_callable_keys_are_distinct) {
  // Two separately-registered callables must produce two distinct keys;
  // invoking one must not call the other.
  int count_a = 0;
  int count_b = 0;
  const std::string a = baml_sdk::host_callable_tests::call_with_callback(
      [&count_a](int64_t x) {
        ++count_a;
        return "a:" + std::to_string(x);
      },
      1);
  const std::string b = baml_sdk::host_callable_tests::call_with_callback(
      [&count_b](int64_t x) {
        ++count_b;
        return "b:" + std::to_string(x);
      },
      2);
  BAML_ASSERT_EQ(a, std::string("a:1"));
  BAML_ASSERT_EQ(b, std::string("b:2"));
  BAML_ASSERT(count_a == 1 && count_b == 1);
}

BAML_TEST(host_callables_class_callback_round_trips_class_value) {
  // A user-defined Person crosses the callable boundary: BAML encodes it
  // for the engine->host call; the dispatcher decodes it into the
  // codegen-emitted struct; the callback receives a Person.
  const std::string result =
      baml_sdk::host_callable_tests::call_with_class_callback(
          [](Person p) { return p.name + " is " + std::to_string(p.age); },
          Person{"Ada", 37});
  BAML_ASSERT_EQ(result, std::string("Ada is 37"));
}

BAML_TEST(host_callables_call_repeatedly_invokes_callback_n_times) {
  // N round-trips through SysOp::BamlHostCallHostValue.
  std::vector<int64_t> invocations;
  const std::vector<std::string> results =
      baml_sdk::host_callable_tests::call_repeatedly(
          [&invocations](int64_t x) {
            invocations.push_back(x);
            return "item-" + std::to_string(x);
          },
          5);
  BAML_ASSERT((results == std::vector<std::string>{"item-0", "item-1", "item-2",
                                                   "item-3", "item-4"}));
  BAML_ASSERT((invocations == std::vector<int64_t>{0, 1, 2, 3, 4}));
}

BAML_TEST(host_callables_call_repeatedly_with_zero_n_returns_empty_list) {
  std::vector<int64_t> invocations;
  const std::vector<std::string> results =
      baml_sdk::host_callable_tests::call_repeatedly(
          [&invocations](int64_t x) {
            invocations.push_back(x);
            return std::string();
          },
          0);
  BAML_ASSERT(results.empty());
  BAML_ASSERT(invocations.empty());
}

BAML_TEST(
    host_callables_call_with_throwing_in_baml_catches_host_callable_error) {
  // The BAML `catch (e)` around a host-callable invocation intercepts a
  // host-thrown baml.errors.HostCallable; class_name carries the host
  // exception's (demangled) dynamic type.
  const std::string result = baml_sdk::host_callable_tests::call_with_throwing(
      [](int64_t) -> std::string {
        throw std::runtime_error("boom from host");
      },
      1);
  BAML_ASSERT_EQ(result, std::string("caught:std::runtime_error"));
}

// ---------------------------------------------------------------------------
// Optional args x host callables (the combination).
//
// A host callable whose own type carries optional parameters
// (`(x: int, y?: int, z?: int) -> int`). An omitted optional arrives as an
// unset Arg, so the host's own default is the only source of a value when
// BAML omits the arg. The callback returns x*100 + y*10 + z so each test
// can read off exactly which optionals were delivered.
// ---------------------------------------------------------------------------

static int64_t optional_args_cb(int64_t x, baml::arg<int64_t> y,
                                baml::arg<int64_t> z) {
  const int64_t y_val = y.is_set() ? y.value() : 8;
  const int64_t z_val = z.is_set() ? z.value() : 9;
  return x * 100 + y_val * 10 + z_val;
}

BAML_TEST(host_callables_optional_args_all_unset_apply_host_defaults) {
  // `callback(x)` supplies neither optional: both arrive unset and the
  // host defaults fill y/z (8 and 9), yielding 5*100 + 8*10 + 9 = 589.
  const std::vector<int64_t> results =
      baml_sdk::host_callable_tests::call_callback_with_optional_args_all_unset(
          optional_args_cb, 5);
  BAML_ASSERT((results == std::vector<int64_t>{589}));
}

BAML_TEST(host_callables_optional_args_partially_set_deliver_by_name) {
  // Two calls each supplying exactly one optional by name:
  // `callback(x, y = 2)` -> 529, then `callback(x, z = 3)` -> 583 --
  // including the case where the leading optional y is skipped while z
  // is supplied.
  const std::vector<int64_t> results = baml_sdk::host_callable_tests::
      call_callback_with_optional_args_partially_set(optional_args_cb, 5);
  BAML_ASSERT((results == std::vector<int64_t>{529, 583}));
}

BAML_TEST(host_callables_optional_args_all_set_deliver_both) {
  // `callback(x, y = 2, z = 3)` supplies both optionals; both arrive set
  // and override the host defaults, yielding 500 + 20 + 3 = 523.
  const std::vector<int64_t> results =
      baml_sdk::host_callable_tests::call_callback_with_optional_args_all_set(
          optional_args_cb, 5);
  BAML_ASSERT((results == std::vector<int64_t>{523}));
}
