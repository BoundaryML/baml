#ifndef BAML_ARG_H_
#define BAML_ARG_H_

#include <optional>
#include <stdexcept>
#include <type_traits>
#include <utility>
#include <variant>

namespace baml {

// The BAML `null` unit type is std::monostate. variant-with-null is
// std::optional, so nullability uses only std vocabulary:
//   null         -> std::monostate
//   T | null     -> std::optional<T>

// Explicit spelling for the unset state of an optional argument (Python's
// UNSET analog). Omitting the setter is equivalent.
struct unset_t {
  explicit constexpr unset_t() = default;
};
inline constexpr unset_t unset{};

namespace detail {

template <typename T>
struct is_std_optional : std::false_type {};
template <typename U>
struct is_std_optional<std::optional<U>> : std::true_type {};

}  // namespace detail

// Two-state optional-argument holder: unset (omitted; the engine evaluates
// the declared BAML default) or a value of the argument's normalized type.
// Nullability lives in T itself: a `string?` argument is
// arg<std::optional<std::string>>, so std::nullopt is a *value* that is null,
// and a non-nullable `int` argument (arg<int64_t>) rejects null at compile
// time.
template <typename T>
class arg {
 public:
  arg() = default;
  arg(unset_t) {}

  // Converting constructor so e.g. a string literal reaches
  // arg<std::optional<std::string>> in one user-defined conversion
  // (generated setters take arg<T> by value).
  template <typename U,
            typename = std::enable_if_t<
                std::is_constructible<T, U&&>::value &&
                !std::is_same<std::decay_t<U>, arg>::value &&
                !std::is_same<std::decay_t<U>, unset_t>::value &&
                !(std::is_same<std::decay_t<U>, std::monostate>::value &&
                  detail::is_std_optional<T>::value)>>
  arg(U&& value) : value_(std::in_place, std::forward<U>(value)) {}

  // A value of the null type is a null: std::monostate{} sets a nullable
  // argument to null. Excluded when T itself is the bare null type, where
  // the converting constructor already treats monostate as the value.
  template <typename U = T,
            typename = std::enable_if_t<detail::is_std_optional<U>::value>>
  arg(std::monostate) : value_(std::in_place, std::nullopt) {}

  bool is_unset() const { return !value_.has_value(); }
  bool is_set() const { return value_.has_value(); }

  const T& value() const {
    if (!is_set()) {
      throw std::logic_error("baml::arg::value() called while unset");
    }
    return *value_;
  }

 private:
  std::optional<T> value_;
};

}  // namespace baml

#endif  // BAML_ARG_H_
