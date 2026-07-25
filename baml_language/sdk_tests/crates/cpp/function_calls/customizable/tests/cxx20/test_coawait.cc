// co_await coverage for baml::future (this file builds as a separate
// C++20 executable; the C++17 blocking surface is covered by
// test_cancellation.cc). No python analog: python awaits are event-loop
// native, while C++20 ships coroutines without a task type - a minimal
// completion-latch test_task drives the coroutines here, and the bridge
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
#error "the cxx20 test target must enable baml::future's awaiter"
#endif

#include <coroutine>

namespace throws_test = baml_sdk::throws_test;
using baml_sdk::throws_test::MyError;

using sleep_future = decltype(throws_test::SleepMs_async(0));
using int_future = decltype(baml_sdk::round_trip_int_async(0));
using throw_future = decltype(throws_test::ThrowMyError_async());

namespace {

// Minimal eager coroutine with a completion latch: the test thread blocks
// in join() until the coroutine finishes (possibly resumed on the bridge
// dispatcher thread) and an escaped exception rethrows there.
struct test_task {
  struct completion_latch {
    std::mutex mu;
    std::condition_variable cv;
    bool done = false;
    std::exception_ptr error;
  };

  struct promise_type {
    std::shared_ptr<completion_latch> latch =
        std::make_shared<completion_latch>();
    test_task get_return_object() { return test_task{latch}; }
    std::suspend_never initial_suspend() { return {}; }
    std::suspend_never final_suspend() noexcept { return {}; }
    void return_void() { finish(nullptr); }
    void unhandled_exception() { finish(std::current_exception()); }
    void finish(std::exception_ptr error) {
      {
        std::lock_guard<std::mutex> lock(latch->mu);
        latch->error = std::move(error);
        latch->done = true;
      }
      latch->cv.notify_all();
    }
  };

  void join() {
    std::unique_lock<std::mutex> lock(latch->mu);
    latch->cv.wait(lock, [this] { return latch->done; });
    if (latch->error) {
      std::rethrow_exception(latch->error);
    }
  }

  std::shared_ptr<completion_latch> latch;
};

test_task await_sleep(sleep_future fut, bool* completed) {
  (void)co_await std::move(fut);
  *completed = true;
}

test_task await_int(int_future fut, int64_t* out) {
  *out = co_await std::move(fut);
}

test_task await_expect_my_error(throw_future fut, bool* caught) {
  try {
    (void)co_await std::move(fut);
  } catch (const baml::thrown<baml::variant<MyError>>& e) {
    *caught = e.get<MyError>() == MyError{42, "boom"};
  }
}

test_task await_expect_cancelled(sleep_future fut, bool* cancelled) {
  try {
    (void)co_await std::move(fut);
  } catch (const baml::cancelled&) {
    *cancelled = true;
  }
}

test_task await_uncaught(throw_future fut) { (void)co_await std::move(fut); }

}  // namespace

BAML_TEST(coawait_co_await_pending_future_resumes) {
  // A genuinely in-flight call: the coroutine suspends and the dispatcher
  // thread resumes it when the envelope lands.
  bool completed = false;
  test_task task = await_sleep(throws_test::SleepMs_async(100), &completed);
  task.join();
  BAML_ASSERT(completed);
}

BAML_TEST(coawait_co_await_yields_the_decoded_value) {
  int64_t got = 0;
  test_task task = await_int(baml_sdk::round_trip_int_async(7), &got);
  task.join();
  BAML_ASSERT(got == 7);
}

BAML_TEST(coawait_co_await_completed_future_fast_path) {
  // await_ready() true: no suspension, the coroutine runs straight through.
  auto fut = baml_sdk::round_trip_int_async(7);
  fut.wait();
  int64_t got = 0;
  test_task task = await_int(std::move(fut), &got);
  task.join();
  BAML_ASSERT(got == 7);
}

BAML_TEST(coawait_co_await_throws_typed_into_the_coroutine) {
  bool caught = false;
  test_task task =
      await_expect_my_error(throws_test::ThrowMyError_async(), &caught);
  task.join();
  BAML_ASSERT(caught);
}

BAML_TEST(coawait_co_await_cancelled_call_throws_cancelled) {
  auto fut = throws_test::SleepMs_async(2000);
  std::this_thread::sleep_for(std::chrono::milliseconds(50));
  BAML_ASSERT(fut.cancel());
  bool cancelled = false;
  test_task task = await_expect_cancelled(std::move(fut), &cancelled);
  task.join();
  BAML_ASSERT(cancelled);
}

BAML_TEST(coawait_uncaught_coroutine_exception_reaches_join) {
  test_task task = await_uncaught(throws_test::ThrowMyError_async());
  bool rethrown = false;
  try {
    task.join();
  } catch (const baml::thrown<baml::variant<MyError>>&) {
    rethrown = true;
  }
  BAML_ASSERT(rethrown);
}

BAML_TEST_MAIN()
