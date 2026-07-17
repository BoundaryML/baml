// co_await coverage for baml::Future (this file builds as a separate
// C++20 executable; the C++17 blocking surface is covered by
// test_cancellation.cc). No python analog: python awaits are event-loop
// native, while C++20 ships coroutines without a task type - a minimal
// completion-latch TestTask drives the coroutines here, and the bridge
// dispatcher thread resumes them.
//
// Coroutines take their inputs by value and their outputs via pointers to
// test-owned locals (never lambda captures: a capturing lambda coroutine
// dangles once the lambda temporary dies at the end of the launch
// statement).
#include <baml_sdk.h>
#include <baml_test.h>

#include <chrono>
#include <condition_variable>
#include <exception>
#include <memory>
#include <mutex>
#include <thread>
#include <utility>
#include <variant>

#if !defined(BAML_HAS_COROUTINES)
#error "the cxx20 test target must enable baml::Future's awaiter"
#endif

#include <coroutine>

namespace throws_test = baml_sdk::throws_test;
using baml_sdk::throws_test::MyError;

using SleepFuture = decltype(throws_test::SleepMsAsync(0));
using IntFuture = decltype(baml_sdk::round_trip_intAsync(0));
using ThrowFuture = decltype(throws_test::ThrowMyErrorAsync());

namespace {

// Minimal eager coroutine with a completion latch: the test thread blocks
// in Join() until the coroutine finishes (possibly resumed on the bridge
// dispatcher thread) and an escaped exception rethrows there.
struct TestTask {
  struct Latch {
    std::mutex mu;
    std::condition_variable cv;
    bool done = false;
    std::exception_ptr error;
  };

  struct promise_type {
    std::shared_ptr<Latch> latch = std::make_shared<Latch>();
    TestTask get_return_object() { return TestTask{latch}; }
    std::suspend_never initial_suspend() { return {}; }
    std::suspend_never final_suspend() noexcept { return {}; }
    void return_void() { Finish(nullptr); }
    void unhandled_exception() { Finish(std::current_exception()); }
    void Finish(std::exception_ptr error) {
      {
        std::lock_guard<std::mutex> lock(latch->mu);
        latch->error = std::move(error);
        latch->done = true;
      }
      latch->cv.notify_all();
    }
  };

  void Join() {
    std::unique_lock<std::mutex> lock(latch->mu);
    latch->cv.wait(lock, [this] { return latch->done; });
    if (latch->error) {
      std::rethrow_exception(latch->error);
    }
  }

  std::shared_ptr<Latch> latch;
};

TestTask AwaitSleep(SleepFuture fut, bool* completed) {
  (void)co_await std::move(fut);
  *completed = true;
}

TestTask AwaitInt(IntFuture fut, int64_t* out) {
  *out = co_await std::move(fut);
}

TestTask AwaitExpectMyError(ThrowFuture fut, bool* caught) {
  try {
    (void)co_await std::move(fut);
  } catch (const baml::BamlThrown<baml::Union<MyError>>& e) {
    *caught = e.get<MyError>() == MyError{42, "boom"};
  }
}

TestTask AwaitExpectCancelled(SleepFuture fut, bool* cancelled) {
  try {
    (void)co_await std::move(fut);
  } catch (const baml::BamlCancelled&) {
    *cancelled = true;
  }
}

TestTask AwaitUncaught(ThrowFuture fut) { (void)co_await std::move(fut); }

}  // namespace

BAML_TEST(co_await_pending_future_resumes) {
  // A genuinely in-flight call: the coroutine suspends and the dispatcher
  // thread resumes it when the envelope lands.
  bool completed = false;
  TestTask task = AwaitSleep(throws_test::SleepMsAsync(100), &completed);
  task.Join();
  BAML_ASSERT(completed);
}

BAML_TEST(co_await_yields_the_decoded_value) {
  int64_t got = 0;
  TestTask task = AwaitInt(baml_sdk::round_trip_intAsync(7), &got);
  task.Join();
  BAML_ASSERT(got == 7);
}

BAML_TEST(co_await_completed_future_fast_path) {
  // await_ready() true: no suspension, the coroutine runs straight through.
  auto fut = baml_sdk::round_trip_intAsync(7);
  fut.wait();
  int64_t got = 0;
  TestTask task = AwaitInt(std::move(fut), &got);
  task.Join();
  BAML_ASSERT(got == 7);
}

BAML_TEST(co_await_throws_typed_into_the_coroutine) {
  bool caught = false;
  TestTask task = AwaitExpectMyError(throws_test::ThrowMyErrorAsync(), &caught);
  task.Join();
  BAML_ASSERT(caught);
}

BAML_TEST(co_await_cancelled_call_throws_cancelled) {
  auto fut = throws_test::SleepMsAsync(2000);
  std::this_thread::sleep_for(std::chrono::milliseconds(50));
  BAML_ASSERT(fut.Cancel());
  bool cancelled = false;
  TestTask task = AwaitExpectCancelled(std::move(fut), &cancelled);
  task.Join();
  BAML_ASSERT(cancelled);
}

BAML_TEST(uncaught_coroutine_exception_reaches_join) {
  TestTask task = AwaitUncaught(throws_test::ThrowMyErrorAsync());
  bool rethrown = false;
  try {
    task.Join();
  } catch (const baml::BamlThrown<baml::Union<MyError>>&) {
    rethrown = true;
  }
  BAML_ASSERT(rethrown);
}

BAML_TEST_MAIN()
