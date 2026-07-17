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
class Box {
 public:
  Box(T value) : ptr_(new T(std::move(value))) {}

  Box(const Box& other) : ptr_(new T(*other.ptr_)) {}
  Box(Box&&) noexcept = default;
  Box& operator=(const Box& other) {
    if (this != &other) {
      ptr_ = std::unique_ptr<T>(new T(*other.ptr_));
    }
    return *this;
  }
  Box& operator=(Box&&) noexcept = default;

  T& operator*() { return *ptr_; }
  const T& operator*() const { return *ptr_; }
  T* operator->() { return ptr_.get(); }
  const T* operator->() const { return ptr_.get(); }

  friend bool operator==(const Box& a, const Box& b) {
    return *a.ptr_ == *b.ptr_;
  }
  friend bool operator!=(const Box& a, const Box& b) { return !(a == b); }

 private:
  std::unique_ptr<T> ptr_;
};

// Nullable deep-copying heap box: the spelling of `T | null` when T is a
// recursive class (std::optional requires a complete T; OptionalBox, like
// Box, only needs the forward declaration). Empty = BAML null.
template <typename T>
class OptionalBox {
 public:
  OptionalBox() = default;
  OptionalBox(std::nullopt_t) {}
  OptionalBox(T value) : ptr_(new T(std::move(value))) {}

  OptionalBox(const OptionalBox& other)
      : ptr_(other.ptr_ ? new T(*other.ptr_) : nullptr) {}
  OptionalBox(OptionalBox&&) noexcept = default;
  OptionalBox& operator=(const OptionalBox& other) {
    if (this != &other) {
      ptr_ = other.ptr_ ? std::unique_ptr<T>(new T(*other.ptr_)) : nullptr;
    }
    return *this;
  }
  OptionalBox& operator=(OptionalBox&&) noexcept = default;

  bool HasValue() const { return ptr_ != nullptr; }
  explicit operator bool() const { return HasValue(); }
  T& operator*() { return *ptr_; }
  const T& operator*() const { return *ptr_; }
  T* operator->() { return ptr_.get(); }
  const T* operator->() const { return ptr_.get(); }

  friend bool operator==(const OptionalBox& a, const OptionalBox& b) {
    if (a.HasValue() != b.HasValue()) {
      return false;
    }
    return !a.HasValue() || *a.ptr_ == *b.ptr_;
  }
  friend bool operator!=(const OptionalBox& a, const OptionalBox& b) {
    return !(a == b);
  }

 private:
  std::unique_ptr<T> ptr_;
};

}  // namespace baml

#endif  // BAML_BOX_H_
