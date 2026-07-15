#ifndef BAML_HANDLE_H_
#define BAML_HANDLE_H_

// baml::Handle: an owned reference to an engine-side opaque value (a
// `$rust_type` field of a handle-backed stdlib class, e.g. baml.fs.File's
// `_handle`). The engine hands the host an owned key; releasing the last
// key drops the underlying value. Copying clones the key (both copies
// reference the same engine value); encoding onto the wire clones too,
// because the engine's inbound decode consumes (drains) the key it
// receives.

#include <baml/detail/loader.h>
#include <baml/errors.h>
#include <baml_cffi.h>

#include <cstdint>
#include <string>
#include <utility>

namespace baml {

class Handle {
 public:
  Handle() = default;

  // Takes ownership of an engine-issued key (wire decode path).
  Handle(uint64_t key, int32_t handle_type)
      : key_(key), handle_type_(handle_type) {}

  Handle(const Handle& other) : key_(0), handle_type_(other.handle_type_) {
    if (other.key_ != 0) {
      key_ = other.ClonedKey("baml::Handle copy");
    }
  }

  Handle& operator=(const Handle& other) {
    if (this != &other) {
      Handle copy(other);
      swap(copy);
    }
    return *this;
  }

  Handle(Handle&& other) noexcept
      : key_(other.key_), handle_type_(other.handle_type_) {
    other.key_ = 0;
  }

  Handle& operator=(Handle&& other) noexcept {
    if (this != &other) {
      Release();
      key_ = other.key_;
      handle_type_ = other.handle_type_;
      other.key_ = 0;
    }
    return *this;
  }

  ~Handle() { Release(); }

  uint64_t key() const { return key_; }
  int32_t handle_type() const { return handle_type_; }
  bool empty() const { return key_ == 0; }

  // A fresh owned key for the wire: the engine's inbound decode drains
  // the key it receives, so encoding must not spend this handle's own.
  uint64_t CloneKeyForWire() const {
    return ClonedKey("baml::Handle wire encode");
  }

  // Key identity: two handles are equal when they hold the same key.
  // Clones of the same engine value hold distinct keys and compare
  // unequal.
  friend bool operator==(const Handle& a, const Handle& b) {
    return a.key_ == b.key_;
  }
  friend bool operator!=(const Handle& a, const Handle& b) { return !(a == b); }

  void swap(Handle& other) noexcept {
    std::swap(key_, other.key_);
    std::swap(handle_type_, other.handle_type_);
  }

 private:
  uint64_t ClonedKey(const char* context) const {
    uint64_t out_key = 0;
    if (detail::Api().handle_clone(key_, &out_key) != BamlCffiStatus_Ok) {
      throw BamlError(std::string(context) + ": invalid handle key " +
                      std::to_string(key_));
    }
    return out_key;
  }

  void Release() {
    if (key_ != 0) {
      detail::Api().handle_release(key_);
      key_ = 0;
    }
  }

  uint64_t key_ = 0;
  int32_t handle_type_ = 0;
};

}  // namespace baml

#endif  // BAML_HANDLE_H_
