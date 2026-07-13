#ifndef BAML_ARG_HPP
#define BAML_ARG_HPP

#include <optional>
#include <stdexcept>
#include <utility>
#include <variant>

namespace baml {

// The BAML `null` unit type is std::monostate; this alias is documentation.
// Union-with-null is std::optional, so nullability uses only std vocabulary:
//   null         -> std::monostate (baml::Null)
//   T | null     -> std::optional<T>
//   A | B | null -> std::optional<std::variant<A, B>>
using Null = std::monostate;

// Tri-state optional-argument holder: unset / explicit null / value.
// Default-constructed = unset (engine evaluates the declared default);
// std::nullopt = explicit BAML null, same spelling as for optional values.
template <typename T>
class Arg {
public:
    enum class State { Unset, Null, Value };

    Arg() = default;
    Arg(std::nullopt_t) : state_(State::Null) {}
    Arg(T value) : state_(State::Value), value_(std::move(value)) {}

    State state() const { return state_; }
    bool is_unset() const { return state_ == State::Unset; }
    bool is_null() const { return state_ == State::Null; }
    bool has_value() const { return state_ == State::Value; }

    const T& value() const {
        if (!has_value()) {
            throw std::logic_error("baml::Arg::value() called without a value");
        }
        return *value_;
    }

private:
    State state_ = State::Unset;
    std::optional<T> value_;
};

}  // namespace baml

#endif  // BAML_ARG_HPP
