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

#include <cstddef>
#include <exception>
#include <functional>
#include <string>
#include <utility>
#include <vector>

using baml_sdk::host_callable_tests::Person;
using baml_sdk::host_callable_tests::ValidationError;
using baml_sdk::throws_test::MyError;

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

BAML_TEST(host_callable_hof_receives_a_baml_closure) {
  // BAML hands the callback a closure of its own (`x + 1`); it arrives as a
  // baml::function like any BAML closure returned to C++, stands in for a
  // std::function, and calls back into BAML each time (twice here, so the
  // handle outlives a call); its result flows back through `hof`.
  int calls = 0;
  const int64_t result = baml_sdk::host_callable_tests::hof(
      [&calls](const baml::function<int64_t(int64_t)>& f) {
        ++calls;
        const std::function<int64_t(int64_t)> plus_one = f;
        return plus_one(41) * 100 + f(1) * 2;
      });
  BAML_ASSERT_EQ(result, int64_t{4204});
  BAML_ASSERT_EQ(calls, 1);
}

BAML_TEST(host_callable_two_cbs_callbacks_named_f_and_f_returns_both_run) {
  // Two callback parameters whose names differ by a `_returns` suffix each
  // keep their own callable: `f(1) + f_returns(2)`.
  const int64_t result = baml_sdk::host_callable_tests::two_cbs(
      [](int64_t x) { return x * 10; }, [](int64_t x) { return x * 100; });
  BAML_ASSERT_EQ(result, int64_t{210});
}

BAML_TEST(host_callable_void_signatures) {
  // A unit callable has no result to carry in either direction: a host
  // callback is a void std::function whatever its arity, and so is a BAML
  // closure returned to the host.
  std::vector<std::string> seen;

  BAML_ASSERT_EQ(
      baml_sdk::host_callable_tests::call_void_callback(
          [&seen](int64_t value) { seen.push_back(std::to_string(value)); }, 1),
      int64_t{1});
  const auto two = [&seen](int64_t value, std::string label) {
    seen.push_back(std::to_string(value) + ":" + label);
  };
  BAML_ASSERT_EQ(
      baml_sdk::host_callable_tests::call_void_callback_two_args(two, 2, "two"),
      int64_t{2});
  BAML_ASSERT_EQ(baml_sdk::host_callable_tests::call_void_callback_three_args(
                     [&seen](int64_t value, std::string label, bool flag) {
                       seen.push_back(std::to_string(value) + ":" + label +
                                      ":" + (flag ? "true" : "false"));
                     },
                     3, "three", true),
                 int64_t{3});

  baml_sdk::host_callable_tests::call_throwing_void_callback(
      [&seen](int64_t value) { seen.push_back(std::to_string(value)); }, 4);
  bool threw = false;
  try {
    baml_sdk::host_callable_tests::call_throwing_void_callback(
        [](int64_t) { throw std::runtime_error("void failed"); }, 4);
    baml_test::fail("call_throwing_void_callback did not throw");
  } catch (const std::runtime_error& e) {
    threw = true;
    BAML_ASSERT_EQ(std::string(e.what()), std::string("void failed"));
  }
  BAML_ASSERT(threw);

  const auto nothing = baml_sdk::host_callable_tests::make_void_callback();
  nothing();

  const auto forward = baml_sdk::host_callable_tests::make_void_forwarder(two);
  forward(5, "five");
  forward(6, "six");

  BAML_ASSERT((seen == std::vector<std::string>{"1", "2:two", "3:three:true",
                                                "4", "5:five", "6:six"}));
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

// ---------------------------------------------------------------------------
// How a callback's call completes.
//
// The outcome the bridge sends decides what BAML sees: a value BAML threw
// and the callback let through goes back as that value (a panic stays a
// panic whatever the callback declares), an exception of the callback's own
// becomes a catchable baml.errors.HostCallable, a value that does not fit
// the declared type is the callback's breach of contract
// (baml.panics.HostContractViolation), and a value the bridge itself cannot
// carry fails the call as the bridge's own (baml.panics.SdkPanic).
// ---------------------------------------------------------------------------

namespace {

// `1` inside `levels` single-item lists.
baml_sdk::baml::json::json nested_list(std::size_t levels) {
  baml_sdk::baml::json::json value{int64_t{1}};
  for (std::size_t i = 0; i < levels; ++i) {
    std::vector<::baml::box<baml_sdk::baml::json::json>> list;
    list.emplace_back(std::move(value));
    value = baml_sdk::baml::json::json{std::move(list)};
  }
  return value;
}

// Runs `call`, which must fail with the panic `class_name`, its message
// containing `detail`.
template <typename Call>
void expect_panic(const Call& call, const std::string& class_name,
                  const std::string& detail) {
  try {
    call();
  } catch (const baml::panic& e) {
    if (e.class_name() != class_name ||
        e.message().find(detail) == std::string::npos)
      baml_test::fail("expected " + class_name + " (" + detail + "), got " +
                      e.class_name() + ": " + e.what());
    return;
  } catch (const std::exception& e) {
    baml_test::fail("expected " + class_name +
                    ", got an exception: " + e.what());
  }
  baml_test::fail("expected " + class_name + ", the call returned");
}

}  // namespace

// SDK_PARITY_LINT(skip): C++ can return only a malformed bigint as unfit.
BAML_TEST(host_callables_unfit_callback_result_is_a_contract_violation) {
  // C++'s types admit every value the slot does, except a bigint whose text
  // is not a decimal integer: the bridge sees it does not fit and reports
  // the callback's breach, never a catchable host exception.
  expect_panic(
      [] {
        baml_sdk::host_callable_tests::call_float_bigint_union_callback([] {
          return ::baml::variant<::baml::bigint, double>(
              ::baml::bigint("twelve"));
        });
      },
      "baml.panics.HostContractViolation", "malformed decimal bigint");
}

// SDK_PARITY_LINT(skip): C++ throws a BAML value with baml::host_throw.
BAML_TEST(host_callables_typed_throw_too_deep_is_a_bridge_failure) {
  // The thrown value goes through the same encoder and the same measure as
  // a result: past what the wire carries it is the bridge's own failure,
  // reported as such rather than as a fault outside the callable.
  expect_panic(
      [] {
        baml_sdk::host_callable_tests::call_with_callback(
            [](int64_t) -> std::string {
              throw baml::host_throw<baml_sdk::baml::json::json>(
                  nested_list(60));
            },
            1);
      },
      "baml.panics.SdkPanic", "deeper than the 100 nested messages the wire");
}

// SDK_PARITY_LINT(skip): pins a forwarded BAML error completed as thrown
BAML_TEST(host_callables_forwarded_baml_error_is_thrown_as_itself) {
  // A callback that lets a nested BAML call's error through sends that
  // value back: BAML sees the class it threw, not a host exception.
  bool threw = false;
  try {
    baml_sdk::host_callable_tests::call_with_callback(
        [](int64_t) -> std::string {
          baml_sdk::throws_test::ThrowMyError();
          return "unreachable";
        },
        1);
  } catch (const baml::panic& e) {
    baml_test::fail(std::string("a forwarded error became a panic: ") +
                    e.what());
  } catch (const baml::error& e) {
    threw = true;
    BAML_ASSERT_EQ(e.class_name(), std::string("user.throws_test.MyError"));
    BAML_ASSERT(e.is<MyError>());
    BAML_ASSERT((e.get<MyError>() == MyError{42, "boom"}));
    // The value can be read again: reading decodes a copy of its own.
    BAML_ASSERT((e.get<MyError>() == MyError{42, "boom"}));
  }
  BAML_ASSERT(threw);
}

// SDK_PARITY_LINT(skip): C++ surfaces BAML cancellation as baml::cancelled.
BAML_TEST(host_callables_forwarded_cancellation_stays_a_panic_under_throws) {
  // A nested call's cancellation is a panic: forwarded through a callback
  // that declares `throws ValidationError` it is still a panic, never held
  // to the declared type as the callback's breach of contract.
  bool cancelled = false;
  try {
    baml_sdk::host_callable_tests::call_with_typed_throws_propagating(
        [](int64_t) -> std::string {
          auto sleeping = baml_sdk::throws_test::SleepMs_async(60000);
          sleeping.cancel();
          sleeping.get();
          return "unreachable";
        },
        1);
  } catch (const baml::cancelled& e) {
    cancelled = true;
    BAML_ASSERT_EQ(e.class_name(), std::string("baml.panics.Cancelled"));
  } catch (const std::exception& e) {
    baml_test::fail(std::string("expected the forwarded cancellation, got ") +
                    e.what());
  }
  BAML_ASSERT(cancelled);
}
