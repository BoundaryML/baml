#ifndef BAML_BOX_H_
#define BAML_BOX_H_

#include <memory>
#include <optional>
#include <utility>

namespace baml {

// Deep-copying heap box (spec D9). Generated code breaks recursive class
// cycles with it: unlike bare unique_ptr it keeps generated structs
// copyable, and unlike std::optional it may hold an incomplete type.
// Never null except in the moved-from state.
template <typename T>
class box {
 public:
  box(T value) : ptr_(new T(std::move(value))) {}

  box(const box& other) : ptr_(new T(*other.ptr_)) {}
  box(box&&) noexcept = default;
  box& operator=(const box& other) {
    if (this != &other) {
      ptr_ = std::unique_ptr<T>(new T(*other.ptr_));
    }
    return *this;
  }
  box& operator=(box&&) noexcept = default;

  T& operator*() { return *ptr_; }
  const T& operator*() const { return *ptr_; }
  T* operator->() { return ptr_.get(); }
  const T* operator->() const { return ptr_.get(); }

  friend bool operator==(const box& a, const box& b) {
    return *a.ptr_ == *b.ptr_;
  }
  friend bool operator!=(const box& a, const box& b) { return !(a == b); }

 private:
  std::unique_ptr<T> ptr_;
};

// Nullable deep-copying heap box: the spelling of `T | null` when T is a
// recursive class (std::optional requires a complete T; optional_box, like
// box, only needs the forward declaration). Empty = BAML null.
template <typename T>
class optional_box {
 public:
  optional_box() = default;
  optional_box(std::nullopt_t) {}
  optional_box(T value) : ptr_(new T(std::move(value))) {}

  optional_box(const optional_box& other)
      : ptr_(other.ptr_ ? new T(*other.ptr_) : nullptr) {}
  optional_box(optional_box&&) noexcept = default;
  optional_box& operator=(const optional_box& other) {
    if (this != &other) {
      ptr_ = other.ptr_ ? std::unique_ptr<T>(new T(*other.ptr_)) : nullptr;
    }
    return *this;
  }
  optional_box& operator=(optional_box&&) noexcept = default;

  bool has_value() const { return ptr_ != nullptr; }
  explicit operator bool() const { return has_value(); }
  T& operator*() { return *ptr_; }
  const T& operator*() const { return *ptr_; }
  T* operator->() { return ptr_.get(); }
  const T* operator->() const { return ptr_.get(); }

  friend bool operator==(const optional_box& a, const optional_box& b) {
    if (a.has_value() != b.has_value()) {
      return false;
    }
    return !a.has_value() || *a.ptr_ == *b.ptr_;
  }
  friend bool operator!=(const optional_box& a, const optional_box& b) {
    return !(a == b);
  }

 private:
  std::unique_ptr<T> ptr_;
};

}  // namespace baml

#endif  // BAML_BOX_H_
