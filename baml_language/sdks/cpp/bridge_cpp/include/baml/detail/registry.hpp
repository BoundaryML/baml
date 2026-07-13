#ifndef BAML_DETAIL_REGISTRY_HPP
#define BAML_DETAIL_REGISTRY_HPP

#include <atomic>
#include <cstdint>
#include <cstdio>
#include <future>
#include <mutex>
#include <unordered_map>
#include <utility>
#include <vector>

#include <baml_cffi.h>

extern "C" inline void baml_cpp_result_trampoline(uint32_t call_id, const int8_t* content,
                                                  uintptr_t length);

namespace baml {
namespace detail {

// Correlates async results with in-flight calls. The C ABI has exactly one
// process-global result callback; this registry owns it and fans results out
// to per-call promises keyed by a bridge-issued correlation id.
class CallRegistry {
public:
    struct Started {
        uint32_t correlation_id;
        std::future<std::vector<uint8_t>> envelope;
    };

    static CallRegistry& instance() {
        static CallRegistry registry;
        return registry;
    }

    Started begin() {
        uint32_t id = next_id_.fetch_add(1, std::memory_order_relaxed);
        if (id == 0) {
            id = next_id_.fetch_add(1, std::memory_order_relaxed);
        }
        std::promise<std::vector<uint8_t>> promise;
        std::future<std::vector<uint8_t>> future = promise.get_future();
        {
            std::lock_guard<std::mutex> lock(mu_);
            pending_.emplace(id, std::move(promise));
        }
        return Started{id, std::move(future)};
    }

    // Called from the C callback, on an engine thread. The payload is only
    // valid for the duration of the call, so it is copied out here.
    void complete(uint32_t call_id, const int8_t* content, size_t length) {
        std::promise<std::vector<uint8_t>> promise;
        {
            std::lock_guard<std::mutex> lock(mu_);
            auto it = pending_.find(call_id);
            if (it == pending_.end()) {
                std::fprintf(stderr, "baml: result for unknown call id %u dropped\n", call_id);
                return;
            }
            promise = std::move(it->second);
            pending_.erase(it);
        }
        const uint8_t* bytes = reinterpret_cast<const uint8_t*>(content);
        promise.set_value(std::vector<uint8_t>(bytes, bytes + length));
    }

private:
    CallRegistry() { register_callback(&baml_cpp_result_trampoline); }

    std::mutex mu_;
    std::unordered_map<uint32_t, std::promise<std::vector<uint8_t>>> pending_;
    std::atomic<uint32_t> next_id_{1};
};

}  // namespace detail
}  // namespace baml

extern "C" inline void baml_cpp_result_trampoline(uint32_t call_id, const int8_t* content,
                                                  uintptr_t length) {
    baml::detail::CallRegistry::instance().complete(call_id, content,
                                                    static_cast<size_t>(length));
}

#endif  // BAML_DETAIL_REGISTRY_HPP
