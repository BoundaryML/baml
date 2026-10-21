#include <baml_sdk.h>
#include <baml_test.h>

#include <atomic>
#include <future>
#include <thread>

namespace host = baml_sdk::host_callable_tests;
using baml_sdk::baml_options;
using baml_sdk::baml::spawn::CancelToken;

BAML_TEST(invocation_options_four_call_forms) {
  auto options = baml_options{}.timeout_ms(1000);
  BAML_ASSERT_EQ(baml_sdk::optional_args_probe(1),
                 (std::vector<std::optional<int64_t>>{1, 5, 99}));
  BAML_ASSERT_EQ(baml_sdk::optional_args_probe(
                     1, baml_sdk::optional_args_probe_opts{}.set_opt1(
                            std::optional<int64_t>{7})),
                 (std::vector<std::optional<int64_t>>{1, 7, 99}));
  BAML_ASSERT_EQ(baml_sdk::optional_args_probe(1, {}, options),
                 (std::vector<std::optional<int64_t>>{1, 5, 99}));
  BAML_ASSERT_EQ(baml_sdk::optional_args_probe(
                     1,
                     baml_sdk::optional_args_probe_opts{}.set_opt1(
                         std::optional<int64_t>{7}),
                     options),
                 (std::vector<std::optional<int64_t>>{1, 7, 99}));
}
BAML_TEST(invocation_options_four_call_forms_async) {
  auto options = baml_options{}.timeout_ms(1000);
  BAML_ASSERT_EQ(baml_sdk::optional_args_probe_async(1).get(),
                 (std::vector<std::optional<int64_t>>{1, 5, 99}));
  BAML_ASSERT_EQ(baml_sdk::optional_args_probe_async(
                     1, baml_sdk::optional_args_probe_opts{}.set_opt1(
                            std::optional<int64_t>{7}))
                     .get(),
                 (std::vector<std::optional<int64_t>>{1, 7, 99}));
  BAML_ASSERT_EQ(baml_sdk::optional_args_probe_async(1, {}, options).get(),
                 (std::vector<std::optional<int64_t>>{1, 5, 99}));
  BAML_ASSERT_EQ(baml_sdk::optional_args_probe_async(
                     1,
                     baml_sdk::optional_args_probe_opts{}.set_opt1(
                         std::optional<int64_t>{7}),
                     options)
                     .get(),
                 (std::vector<std::optional<int64_t>>{1, 7, 99}));
}
BAML_TEST(invocation_options_invalid_timeout_rejected) {
  for (const auto timeout : {int64_t{-1}, int64_t{2147483648}}) {
    bool rejected = false;
    try {
      baml_sdk::hello_world(baml_options{}.timeout_ms(timeout));
    } catch (const baml::error&) {
      rejected = true;
    }
    BAML_ASSERT(rejected);
  }
}
BAML_TEST(invocation_surfaces_returned_callable_accepts_controls) {
  const auto add = host::make_adder(3);
  BAML_ASSERT_EQ(add.call(4, baml_options{}.timeout_ms(1000)), int64_t{7});
  BAML_ASSERT_EQ(add.call_async(4, baml_options{}.timeout_ms(1000)).get(),
                 int64_t{7});
}
BAML_TEST(invocation_lifecycle_pre_cancelled_call_does_not_enter_callback) {
  const auto token = CancelToken::new_();
  token.cancel();
  std::atomic<bool> entered{false};
  bool rejected = false;
  try {
    host::call_int_callback(
        [&](int64_t value) {
          entered = true;
          return value;
        },
        1, baml_options{}.cancel(token));
  } catch (const baml::error&) {
    rejected = true;
  }
  BAML_ASSERT(rejected);
  BAML_ASSERT(!entered);
}
BAML_TEST(invocation_lifecycle_zero_timeout_does_not_enter_callback) {
  std::atomic<bool> entered{false};
  bool rejected = false;
  try {
    host::call_int_callback(
        [&](int64_t value) {
          entered = true;
          return value;
        },
        1, baml_options{}.timeout_ms(0));
  } catch (const baml::error&) {
    rejected = true;
  }
  BAML_ASSERT(rejected);
  BAML_ASSERT(!entered);
}
BAML_TEST(invocation_lifecycle_reservation_is_single_use) {
  const auto reserved = baml_sdk::trace::hidden().reserve();
  BAML_ASSERT_EQ(
      std::string(baml_sdk::hello_world(baml_options{}.trace(reserved))),
      std::string("hello world"));
  bool rejected = false;
  try {
    baml_sdk::hello_world(baml_options{}.trace(reserved));
  } catch (const baml::error&) {
    rejected = true;
  }
  BAML_ASSERT(rejected);
}
BAML_TEST(invocation_lifecycle_failed_admission_does_not_consume_reservation) {
  const auto reserved = baml_sdk::trace::hidden().reserve();
  bool rejected = false;
  try {
    baml_sdk::hello_world(baml_options{}.trace(reserved).timeout_ms(0));
  } catch (const baml::error&) {
    rejected = true;
  }
  BAML_ASSERT(rejected);
  BAML_ASSERT_EQ(
      std::string(baml_sdk::hello_world(baml_options{}.trace(reserved))),
      std::string("hello world"));
}
BAML_TEST(
    invocation_lifecycle_retained_effective_token_observes_late_parent_cancellation) {
  const auto source = CancelToken::new_();
  std::optional<baml_sdk::invocation::Invocation> retained;
  host::call_int_callback(
      [&](int64_t value) {
        retained = baml_sdk::invocation::current();
        return value;
      },
      1, baml_options{}.cancel(source));
  BAML_ASSERT(!baml_sdk::invocation::current());
  source.cancel();
  BAML_ASSERT(retained->cancel().is_cancelled());
  bool rejected = false;
  try {
    retained->run([] { baml_sdk::hello_world(); });
  } catch (const baml::error&) {
    rejected = true;
  }
  BAML_ASSERT(rejected);
}
BAML_TEST(
    invocation_inheritance_multi_layer_context_patch_inherits_and_restores) {
  auto outer = baml_sdk::trace::context(
      baml_sdk::trace::context_opts{}.set_distinct_id(std::string("A")));
  auto patch = baml_sdk::trace::context(
      baml_sdk::trace::context_opts{}.set_distinct_id(std::string("C")));
  for (int empty = 0; empty < 4; ++empty) {
    host::call_int_callback(
        [&](int64_t value) {
          BAML_ASSERT_EQ(*baml_sdk::trace::current_context().distinct_id,
                         std::string("A"));
          auto layer_b = [&](int64_t value) {
            host::call_int_callback(
                [&](int64_t value) {
                  auto layer_d = [&](int64_t value) {
                    BAML_ASSERT_EQ(
                        *baml_sdk::trace::current_context().distinct_id,
                        std::string("C"));
                    const auto active = *baml_sdk::invocation::current();
                    auto thread = std::async(std::launch::async, [active] {
                      return active.run([] {
                        return baml_sdk::trace::current_context().distinct_id;
                      });
                    });
                    BAML_ASSERT_EQ(*thread.get(), std::string("C"));
                    return value;
                  };
                  if (empty & 2)
                    host::call_int_callback(layer_d, value, {});
                  else
                    host::call_int_callback(layer_d, value);
                  return value;
                },
                value, baml_options{}.trace(patch));
            BAML_ASSERT_EQ(*baml_sdk::trace::current_context().distinct_id,
                           std::string("A"));
            return value;
          };
          if (empty & 1)
            host::call_int_callback(layer_b, value, {});
          else
            host::call_int_callback(layer_b, value);
          return value;
        },
        1, baml_options{}.trace(outer));
  }
  BAML_ASSERT(!baml_sdk::invocation::current());
  BAML_ASSERT(!baml_sdk::trace::current_context().distinct_id);
}
