// Cancellation coverage. Port of function_calls/test_cancellation.py.
// Deviations: the asyncio task/TaskGroup/timeout idioms and BamlCallContext
// have no C++ analog (sync calls are uncancellable; there is no ambient
// call context) - the portable core is the null-return baseline and
// engine-side cancellation of an in-flight call via Future::Cancel. The
// trailing future-semantics cases are C++-specific (python awaitables are
// single-consume by construction).
#include <baml_sdk.h>
#include <baml_test.h>

#include <chrono>
#include <future>
#include <thread>
#include <variant>

namespace throws_test = baml_sdk::throws_test;

BAML_TEST(cancellation_sync_call_returns_none) {
  BAML_ASSERT(throws_test::SleepMs(1) == std::monostate{});
}

BAML_TEST(cancellation_async_call_returns_none) {
  BAML_ASSERT(throws_test::SleepMs_async(1).get() == std::monostate{});
}

BAML_TEST(cancellation_async_cancel_via_future_cancel) {
  const auto start = std::chrono::steady_clock::now();
  auto fut = throws_test::SleepMs_async(2000);
  std::this_thread::sleep_for(std::chrono::milliseconds(50));
  BAML_ASSERT(fut.cancel());
  bool cancelled = false;
  try {
    fut.get();
  } catch (const baml::cancelled&) {
    cancelled = true;
  }
  BAML_ASSERT(cancelled);
  const auto elapsed = std::chrono::steady_clock::now() - start;
  BAML_ASSERT(elapsed < std::chrono::milliseconds(500));
}

BAML_TEST(cancellation_future_wait_then_get) {
  auto fut = throws_test::SleepMs_async(1);
  BAML_ASSERT(fut.valid());
  BAML_ASSERT(fut.wait_for(std::chrono::seconds(30)) ==
              std::future_status::ready);
  BAML_ASSERT(fut.get() == std::monostate{});
  BAML_ASSERT(!fut.valid());
}

BAML_TEST(cancellation_future_second_get_throws_future_error) {
  auto fut = throws_test::SleepMs_async(1);
  (void)fut.get();
  bool threw = false;
  try {
    (void)fut.get();
  } catch (const std::future_error&) {
    threw = true;
  }
  BAML_ASSERT(threw);
}

BAML_TEST(cancellation_future_wait_for_times_out_while_in_flight) {
  auto fut = throws_test::SleepMs_async(2000);
  BAML_ASSERT(fut.wait_for(std::chrono::milliseconds(1)) ==
              std::future_status::timeout);
  BAML_ASSERT(fut.cancel());
  bool cancelled = false;
  try {
    fut.get();
  } catch (const baml::cancelled&) {
    cancelled = true;
  }
  BAML_ASSERT(cancelled);
}

BAML_TEST(cancellation_future_destruction_detaches) {
  // The temporary future is dropped at the end of the statement: neither
  // blocks nor cancels, and later calls are unaffected.
  (void)throws_test::SleepMs_async(1);
  BAML_ASSERT(throws_test::SleepMs(1) == std::monostate{});
}
