// Optional-argument matrix through generated opts structs (spec D4).
// Port of function_calls/customizable/test_optional_args.py. Python's
// negative runtime cases (unknown kwarg, missing required, duplicate arg)
// are compile errors in C++ and need no runtime port.
#include <baml_sdk.h>
#include <baml_test.h>

using baml_sdk::optional_args_probe_opts;

using Probe = std::vector<std::optional<int64_t>>;

BAML_TEST(optional_args_runtime_matrix) {
  BAML_ASSERT(baml_sdk::optional_args_probe(1) == (Probe{1, 5, 99}));
  BAML_ASSERT(baml_sdk::optional_args_probe(1, optional_args_probe_opts{}
                                                   .set_opt1(baml::unset)
                                                   .set_opt2(baml::unset)) ==
              (Probe{1, 5, 99}));
  BAML_ASSERT(baml_sdk::optional_args_probe(
                  1, optional_args_probe_opts{}.set_opt1(std::nullopt)) ==
              (Probe{1, std::nullopt, 99}));
  BAML_ASSERT(baml_sdk::optional_args_probe(
                  1, optional_args_probe_opts{}.set_opt1(int64_t{8})) ==
              (Probe{1, 8, 99}));
  BAML_ASSERT(baml_sdk::optional_args_probe(
                  1, optional_args_probe_opts{}.set_opt2(std::nullopt)) ==
              (Probe{1, 5, std::nullopt}));
  BAML_ASSERT(baml_sdk::optional_args_probe(
                  1, optional_args_probe_opts{}.set_opt2(int64_t{9})) ==
              (Probe{1, 5, 9}));
  BAML_ASSERT(baml_sdk::optional_args_probe(1, optional_args_probe_opts{}
                                                   .set_opt1(std::nullopt)
                                                   .set_opt2(std::nullopt)) ==
              (Probe{1, std::nullopt, std::nullopt}));
  BAML_ASSERT(baml_sdk::optional_args_probe(1, optional_args_probe_opts{}
                                                   .set_opt1(int64_t{8})
                                                   .set_opt2(int64_t{9})) ==
              (Probe{1, 8, 9}));
}

BAML_TEST(optional_args_async_samples) {
  BAML_ASSERT(baml_sdk::optional_args_probe_async(1).get() ==
              (Probe{1, 5, 99}));
  BAML_ASSERT(baml_sdk::optional_args_probe_async(
                  1, optional_args_probe_opts{}.set_opt1(std::nullopt))
                  .get() == (Probe{1, std::nullopt, 99}));
  BAML_ASSERT(baml_sdk::optional_args_probe_async(
                  1, optional_args_probe_opts{}.set_opt2(int64_t{9}))
                  .get() == (Probe{1, 5, 9}));
}

BAML_TEST(optional_args_opt_box_method_matrix) {
  using baml_sdk::OptBox;
  const OptBox box = OptBox::make(10);
  BAML_ASSERT_EQ(box.base, int64_t{17});

  const OptBox box2 =
      OptBox::make(10, OptBox::make_opts{}.set_opt1(int64_t{0}));
  BAML_ASSERT_EQ(box2.base, int64_t{10});
  BAML_ASSERT(box2.probe(1) == (Probe{10, 1, 5}));
  BAML_ASSERT(box2.probe(1, OptBox::probe_opts{}.set_opt1(int64_t{8})) ==
              (Probe{10, 1, 8}));
}

BAML_TEST(optional_args_unset_and_null_differ_in_one_call) {
  // unset means "omit this argument"; nullopt means "pass an explicit
  // null". The two must stay distinct within a single call.
  BAML_ASSERT(baml_sdk::optional_args_probe(1, optional_args_probe_opts{}
                                                   .set_opt1(baml::unset)
                                                   .set_opt2(std::nullopt)) ==
              (Probe{1, 5, std::nullopt}));
  BAML_ASSERT(baml_sdk::optional_args_probe(1, optional_args_probe_opts{}
                                                   .set_opt1(std::nullopt)
                                                   .set_opt2(baml::unset)) ==
              (Probe{1, std::nullopt, 99}));
}
