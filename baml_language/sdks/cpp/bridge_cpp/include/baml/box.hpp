#ifndef BAML_BOX_HPP
#define BAML_BOX_HPP

#include <memory>
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

    friend bool operator==(const Box& a, const Box& b) { return *a.ptr_ == *b.ptr_; }
    friend bool operator!=(const Box& a, const Box& b) { return !(a == b); }

private:
    std::unique_ptr<T> ptr_;
};

}  // namespace baml

#endif  // BAML_BOX_HPP
