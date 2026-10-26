#ifndef BAML_FUTURE_H_
#define BAML_FUTURE_H_

// baml::future<T, ThrownU>: handle to an in-flight async BAML call.
//
// get() blocks until the result envelope arrives and decodes it exactly
// like the synchronous form: the ok arm returns T, declared throws arrive
// as thrown<ThrownU> (ThrownU is the function's `throws` set as a
// baml::variant; void when none is declared), panics as panic. get()
// consumes the future, std::future-style: a second get() (or a get() on a
// moved-from future) throws std::future_error.
//
// cancel() requests engine-side cancellation of the in-flight call and
// returns false if the call is unknown or already completed. The result
// envelope still arrives - as a baml.panics.Cancelled panic - and get()
// then throws cancelled. Destruction detaches: it never blocks and
// never cancels, matching the Python task model.
//
// Under C++20 a future is co_await-able: `co_await std::move(f)` suspends
// without blocking a thread and resumes when the envelope arrives, with
// get()'s exact value/throw semantics. The coroutine resumes ON THE BRIDGE
// DISPATCHER THREAD; code after co_await must not block it (offload long
// work to your own executor). The awaiter is feature-gated so this header
// stays C++17-clean.
//
// The surface mirrors std::future (get/wait/wait_for/wait_until/valid)
// plus cancel and call_id.

#include <baml/codec.h>
#include <baml/detail/loader.h>
#include <baml/detail/registry.h>

#include <chrono>
#include <cstdint>
#include <future>
#include <memory>
#include <mutex>
#include <type_traits>
#include <utility>
#include <variant>

#if defined(__cpp_impl_coroutine) && __has_include(<coroutine>)
#include <coroutine>
#if defined(__cpp_lib_coroutine)
#define BAML_HAS_COROUTINES 1
#endif
#endif

namespace baml {

template <typename T, typename ThrownU = void>
class future {
 public:
  future(std::shared_ptr<detail::call_state> state, uint64_t engine_call_id)
      : state_(std::move(state)), engine_call_id_(engine_call_id) {}
  // A call the host already ran itself: a host callable of this process
  // that BAML handed back. get() returns its result or rethrows what it
  // threw; there is nothing to wait for or cancel.
  explicit future(std::shared_ptr<detail::settled_call<T>> settled)
      : state_(std::move(settled)) {}

  future(future&&) noexcept = default;
  future& operator=(future&&) noexcept = default;
  future(const future&) = delete;
  future& operator=(const future&) = delete;

  // Blocks until the result envelope arrives, then decodes it. Consumes
  // the future.
  T get() {
    require_state();
    if (auto* settled = std::get_if<host_call>(&state_)) {
      const host_call taken = std::move(*settled);
      if (taken->thrown) std::rethrow_exception(taken->thrown);
      if constexpr (!std::is_void<T>::value)
        return std::move(*taken->value);
      else
        return;
    }
    const engine_call taken = std::move(std::get<engine_call>(state_));
    return detail::decode_result<T, ThrownU>(taken->wait());
  }

  // Blocks until the result arrives without consuming the future.
  void wait() const {
    require_state();
    if (const auto* engine = std::get_if<engine_call>(&state_))
      (*engine)->wait();
  }

  template <class Rep, class Period>
  std::future_status wait_for(
      const std::chrono::duration<Rep, Period>& timeout) const {
    return wait_until(std::chrono::steady_clock::now() + timeout);
  }

  template <class Clock, class Duration>
  std::future_status wait_until(
      const std::chrono::time_point<Clock, Duration>& deadline) const {
    require_state();
    const auto* engine = std::get_if<engine_call>(&state_);
    return engine == nullptr || (*engine)->wait_until(deadline)
               ? std::future_status::ready
               : std::future_status::timeout;
  }

  bool valid() const noexcept {
    return std::visit([](const auto& state) { return state != nullptr; },
                      state_);
  }

  // Requests cancellation of the in-flight call. Returns false if the call
  // is unknown or already completed. The result envelope still arrives (as
  // a Cancelled panic) and get() reports it.
  bool cancel() {
    const auto* engine = std::get_if<engine_call>(&state_);
    return engine != nullptr && *engine != nullptr &&
           detail::api().cancel_function_call(engine_call_id_) == 0;
  }

  uint64_t call_id() const noexcept { return engine_call_id_; }

#if defined(BAML_HAS_COROUTINES)
  bool await_ready() const {
    require_state();
    const auto* engine = std::get_if<engine_call>(&state_);
    if (engine == nullptr) return true;
    std::lock_guard<std::mutex> lock((*engine)->mu);
    return (*engine)->ready;
  }

  // Registers the coroutine as the completion continuation. Returns false
  // (resume immediately, no suspension) if the result arrived between the
  // await_ready check and here.
  bool await_suspend(std::coroutine_handle<> handle) {
    const auto* engine = std::get_if<engine_call>(&state_);
    if (engine == nullptr) return false;
    std::lock_guard<std::mutex> lock((*engine)->mu);
    if ((*engine)->ready) {
      return false;
    }
    (*engine)->continuation = [](void* address) {
      std::coroutine_handle<>::from_address(address).resume();
    };
    (*engine)->continuation_arg = handle.address();
    return true;
  }

  T await_resume() { return get(); }
#endif  // BAML_HAS_COROUTINES

 private:
  using engine_call = std::shared_ptr<detail::call_state>;
  using host_call = std::shared_ptr<detail::settled_call<T>>;

  void require_state() const {
    if (!valid()) {
      throw std::future_error(std::future_errc::no_state);
    }
  }

  std::variant<engine_call, host_call> state_;
  uint64_t engine_call_id_ = 0;
};

}  // namespace baml

#endif  // BAML_FUTURE_H_
