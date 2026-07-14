#ifndef BAML_ERRORS_HPP
#define BAML_ERRORS_HPP

#include <cstdint>
#include <exception>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

namespace baml {

// A value thrown by BAML code (the `error` arm of the result envelope).
// what() carries the rendered message plus the BAML trace; the thrown value
// itself rides along as encoded bytes and decodes via is<T>() / get<T>().
class BamlError : public std::runtime_error {
public:
    explicit BamlError(std::string message)
        : BamlError(std::move(message), std::string(), std::string(), {}) {}

    BamlError(std::string message, std::string class_name, std::string baml_trace,
              std::vector<uint8_t> payload)
        : std::runtime_error(render(message, baml_trace)),
          message_(std::move(message)),
          class_name_(std::move(class_name)),
          baml_trace_(std::move(baml_trace)),
          payload_(std::move(payload)) {}

    const std::string& message() const { return message_; }
    const std::string& class_name() const { return class_name_; }
    const std::string& baml_trace() const { return baml_trace_; }
    const std::vector<uint8_t>& payload() const { return payload_; }

    // Typed access to the thrown BAML value. Defined in the codec header;
    // instantiating these requires the generated typemap.
    template <typename T>
    bool is() const;
    template <typename T>
    T get() const;

private:
    static std::string render(const std::string& message, const std::string& trace) {
        return trace.empty() ? message : message + "\n" + trace;
    }

    std::string message_;
    std::string class_name_;
    std::string baml_trace_;
    std::vector<uint8_t> payload_;
};

// The `panic` arm: an engine invariant failure, not a user-thrown value.
class BamlPanic : public BamlError {
public:
    using BamlError::BamlError;
};

// Cancellation surfaces as the engine panic `baml.panics.Cancelled`.
class BamlCancelled : public BamlPanic {
public:
    using BamlPanic::BamlPanic;
};

namespace detail {
namespace wire {
class Writer;
}  // namespace wire
}  // namespace detail

template <typename T>
struct codec;

// Thrown from inside a host callable to surface a typed BAML error to the
// BAML caller: `throw baml::HostThrow<ValidationError>{value}` crosses the
// boundary as a real `ValidationError` class value, so a BAML
// `catch (e: ValidationError)` matches it structurally (the analog of
// Python's `raise BamlError(ValidationError(...))`). Any other host
// exception crosses as an opaque `baml.errors.HostCallable` instead.
class HostThrowBase : public std::exception {
public:
    const char* what() const noexcept override {
        return "BAML host-callable typed throw";
    }
    // Writes the thrown value as an InboundValue message body.
    virtual void encode_value(detail::wire::Writer& value_msg) const = 0;
};

template <typename T>
class HostThrow : public HostThrowBase {
public:
    explicit HostThrow(T value) : value(std::move(value)) {}
    void encode_value(detail::wire::Writer& value_msg) const override {
        codec<T>::encode(value_msg, value);
    }

    T value;
};

}  // namespace baml

#endif  // BAML_ERRORS_HPP
