#ifndef BAML_FUTURE_H_
#define BAML_FUTURE_H_

#include <baml/detail/loader.h>
#include <baml_cffi.h>

#include <chrono>
#include <cstdint>
#include <future>
#include <utility>
#include <vector>

namespace baml {
namespace detail {

// Decodes a BamlOutboundResult envelope into T, throwing BamlError /
// BamlPanic / BamlCancelled for the non-ok arms. Defined in the codec header.
template <typename T>
T DecodeResult(const std::vector<uint8_t>& envelope);

}  // namespace detail

// Handle to an in-flight async BAML call. get() blocks and decodes; Cancel()
// requests engine-side cancellation; destruction detaches (never blocks, never
// cancels), matching the Python task model.
template <typename T>
class Future {
 public:
  Future(std::future<std::vector<uint8_t>> envelope, uint64_t engine_call_id)
      : envelope_(std::move(envelope)), engine_call_id_(engine_call_id) {}

  Future(Future&&) = default;
  Future& operator=(Future&&) = default;
  Future(const Future&) = delete;
  Future& operator=(const Future&) = delete;

  // Blocks until the result envelope arrives, then decodes it. Consumes the
  // future; calling twice is a std::future_error like std::future.
  T get() { return detail::DecodeResult<T>(envelope_.get()); }

  void wait() const { envelope_.wait(); }

  template <typename Rep, typename Period>
  std::future_status wait_for(
      const std::chrono::duration<Rep, Period>& timeout) const {
    return envelope_.wait_for(timeout);
  }

  // Requests cancellation of the in-flight call. Returns false if the call
  // is unknown or already completed. The result envelope still arrives
  // (as a BamlCancelled panic) and get() reports it.
  bool Cancel() {
    return detail::Api().cancel_function_call(engine_call_id_) == 0;
  }

  uint64_t call_id() const { return engine_call_id_; }

 private:
  std::future<std::vector<uint8_t>> envelope_;
  uint64_t engine_call_id_ = 0;
};

}  // namespace baml

#endif  // BAML_FUTURE_H_
