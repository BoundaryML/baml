// Cancellation coverage. Port of function_calls/test_cancellation.py.
// Deviations: asyncio task/TaskGroup/timeout idioms have no C++ analog, and
// there is no BamlCallContext in the C++ bridge yet (sync calls are
// uncancellable) - the portable core is the null-return baseline and
// engine-side cancellation of an in-flight async call via Future::Cancel.
#include <baml_sdk.h>
#include <baml_test.h>

#include <chrono>
#include <thread>

namespace throws_test = baml_sdk::throws_test;

BAML_TEST(sync_call_returns_none) {
  BAML_ASSERT(throws_test::SleepMs(1) == std::monostate{});
}

BAML_TEST(async_call_returns_none) {
  BAML_ASSERT(throws_test::SleepMs_async(1).get() == std::monostate{});
}

BAML_TEST(async_cancel_via_future_cancel) {
  const auto start = std::chrono::steady_clock::now();
  auto fut = throws_test::SleepMs_async(2000);
  std::this_thread::sleep_for(std::chrono::milliseconds(50));
  BAML_ASSERT(fut.Cancel());
  bool cancelled = false;
  try {
    fut.get();
  } catch (const baml::BamlCancelled&) {
    cancelled = true;
  }
  BAML_ASSERT(cancelled);
  const auto elapsed = std::chrono::steady_clock::now() - start;
  BAML_ASSERT(elapsed < std::chrono::milliseconds(500));
}
