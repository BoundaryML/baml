#ifndef BAML_ARG_HPP
#define BAML_ARG_HPP

#include <optional>
#include <stdexcept>
#include <utility>

namespace baml {

// Unit type for BAML `null`-typed values (e.g. a class field declared `null`).
struct Null {
    friend bool operator==(Null, Null) { return true; }
    friend bool operator!=(Null, Null) { return false; }
};

// Tag for passing an explicit BAML null to an optional argument, as opposed
// to leaving it unset (engine evaluates the declared default).
struct null_t {
    explicit constexpr null_t() = default;
};
inline constexpr null_t null{};

// Tri-state optional-argument holder: unset / explicit null / value.
// Default-constructed = unset, which makes `Opts opts = {}` mean "engine
// defaults for everything".
template <typename T>
class Arg {
public:
    enum class State { Unset, Null, Value };

    Arg() = default;
    Arg(null_t) : state_(State::Null) {}
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
