#include <cstddef>
#include <cstdint>
#include <thread>

#if defined(_WIN32)
#  define TEST_EXPORT extern "C" __declspec(dllexport)
#else
#  define TEST_EXPORT extern "C" __attribute__((visibility("default")))
#endif

using TestCallback = void (*)(uint32_t call_id, const uint8_t *content, size_t length);

TEST_EXPORT void baml_test_invoke_on_native_thread(
    TestCallback callback,
    uint32_t call_id,
    const uint8_t *content,
    size_t length) {
  std::thread worker([=]() { callback(call_id, content, length); });
  worker.join();
}
