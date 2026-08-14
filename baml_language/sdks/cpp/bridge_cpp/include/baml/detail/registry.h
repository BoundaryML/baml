#ifndef BAML_DETAIL_REGISTRY_H_
#define BAML_DETAIL_REGISTRY_H_

#include <baml/detail/loader.h>
#include <baml_cffi.h>

#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <memory>
#include <mutex>
#include <unordered_map>
#include <utility>
#include <vector>

extern "C" inline void baml_cpp_result_trampoline(uint32_t call_id,
                                                  const int8_t* content,
                                                  size_t length);

namespace baml {
namespace detail {

// Per-call completion state shared between the issuing thread (via
// baml::future) and the engine callback thread. A plain mutex/condvar cell
// rather than std::promise so a continuation can run at fulfillment time:
// std::future has no completion hook, and the co_await awaiter needs the
// dispatcher thread to resume the suspended coroutine when the envelope
// lands. The continuation is a C++17-clean function pointer (the coroutine
// glue lives behind the feature-gate in future.h).
struct call_state {
  std::mutex mu;
  std::condition_variable cv;
  bool ready = false;
  std::vector<uint8_t> envelope;
  void (*continuation)(void*) = nullptr;
  void* continuation_arg = nullptr;

  // Called from the engine callback thread. Publishes the envelope, wakes
  // blocked waiters, and runs the registered continuation (outside the
  // lock: it may resume a coroutine that immediately touches this state).
  void fulfill(const uint8_t* bytes, size_t length) {
    void (*resume)(void*) = nullptr;
    void* resume_arg = nullptr;
    {
      std::lock_guard<std::mutex> lock(mu);
      envelope.assign(bytes, bytes + length);
      ready = true;
      resume = continuation;
      resume_arg = continuation_arg;
    }
    cv.notify_all();
    if (resume != nullptr) {
      resume(resume_arg);
    }
  }

  // Blocks until the envelope arrives. The reference stays valid for the
  // life of this state; only fulfill writes it, exactly once.
  const std::vector<uint8_t>& wait() {
    std::unique_lock<std::mutex> lock(mu);
    cv.wait(lock, [this] { return ready; });
    return envelope;
  }

  template <class Clock, class Duration>
  bool wait_until(const std::chrono::time_point<Clock, Duration>& deadline) {
    std::unique_lock<std::mutex> lock(mu);
    return cv.wait_until(lock, deadline, [this] { return ready; });
  }
};

// Correlates async results with in-flight calls. The C ABI has exactly one
// process-global result callback; this registry owns it and fans results
// out to per-call call_state cells keyed by a bridge-issued correlation id.
class call_registry {
 public:
  struct started {
    uint32_t correlation_id;
    std::shared_ptr<call_state> state;
  };

  static call_registry& instance() {
    // Intentionally leaked (never destroyed): engine callback threads may
    // fire during process teardown, after static destructors would have
    // run a function-local static's destructor.
    static call_registry* registry = new call_registry();
    return *registry;
  }

  started begin() {
    uint32_t id = next_id_.fetch_add(1, std::memory_order_relaxed);
    if (id == 0) {
      id = next_id_.fetch_add(1, std::memory_order_relaxed);
    }
    auto state = std::make_shared<call_state>();
    {
      std::lock_guard<std::mutex> lock(mu_);
      pending_.emplace(id, state);
    }
    return started{id, std::move(state)};
  }

  // Called from the C callback, on an engine thread. The payload is only
  // valid for the duration of the call, so it is copied out here.
  void complete(uint32_t call_id, const int8_t* content, size_t length) {
    std::shared_ptr<call_state> state;
    {
      std::lock_guard<std::mutex> lock(mu_);
      auto it = pending_.find(call_id);
      if (it == pending_.end()) {
        std::fprintf(stderr, "baml: result for unknown call id %u dropped\n",
                     call_id);
        return;
      }
      state = std::move(it->second);
      pending_.erase(it);
    }
    state->fulfill(reinterpret_cast<const uint8_t*>(content), length);
  }

 private:
  call_registry() { api().register_callback(&baml_cpp_result_trampoline); }

  std::mutex mu_;
  std::unordered_map<uint32_t, std::shared_ptr<call_state>> pending_;
  std::atomic<uint32_t> next_id_{1};
};

}  // namespace detail
}  // namespace baml

extern "C" inline void baml_cpp_result_trampoline(uint32_t call_id,
                                                  const int8_t* content,
                                                  size_t length) {
  // No C++ exception may cross the C ABI (bridge contract).
  try {
    baml::detail::call_registry::instance().complete(call_id, content, length);
  } catch (...) {
  }
}

#endif  // BAML_DETAIL_REGISTRY_H_
