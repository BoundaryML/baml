#ifndef BAML_BUFFER_H_
#define BAML_BUFFER_H_

#include <baml/detail/loader.h>
#include <baml_cffi.h>

#include <string>

namespace baml {
namespace detail {

// RAII owner for a BamlBuffer returned by the C ABI. Frees with free_buffer().
class owned_buffer {
 public:
  explicit owned_buffer(BamlBuffer buf) : buf_(buf) {}

  owned_buffer(owned_buffer&& other) noexcept : buf_(other.buf_) {
    other.buf_ = BamlBuffer{nullptr, 0};
  }
  owned_buffer& operator=(owned_buffer&& other) noexcept {
    if (this != &other) {
      Release();
      buf_ = other.buf_;
      other.buf_ = BamlBuffer{nullptr, 0};
    }
    return *this;
  }
  owned_buffer(const owned_buffer&) = delete;
  owned_buffer& operator=(const owned_buffer&) = delete;

  ~owned_buffer() { Release(); }

  bool empty() const { return buf_.ptr == nullptr || buf_.len == 0; }

  std::string to_string() const {
    return empty()
               ? std::string()
               : std::string(reinterpret_cast<const char*>(buf_.ptr), buf_.len);
  }

 private:
  void Release() {
    if (buf_.ptr != nullptr) {
      api().free_buffer(buf_);
      buf_ = BamlBuffer{nullptr, 0};
    }
  }

  BamlBuffer buf_;
};

}  // namespace detail
}  // namespace baml

#endif  // BAML_BUFFER_H_
