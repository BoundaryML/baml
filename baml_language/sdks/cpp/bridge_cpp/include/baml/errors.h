#ifndef BAML_ERRORS_H_
#define BAML_ERRORS_H_

#include <cstdint>
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
      : std::runtime_error(Render(message, baml_trace)),
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
  static std::string Render(const std::string& message,
                            const std::string& trace) {
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

// A thrown BAML value decoded into the function's declared `throws` set:
// generated bindings throw BamlThrown<baml::Union<A, B>> when the error
// arm decodes into that set, so a catch site reads the typed payload with
// baml::match instead of probing is<T>()/get<T>(). The template argument
// is order-canonical (baml::Union), so BamlThrown<Union<A, B>> and
// BamlThrown<Union<B, A>> are the same catchable type. Derives BamlError:
// untyped catch sites keep working, and an undeclared thrown value (one
// outside the declared set) still surfaces as a plain BamlError.
template <class U>
class BamlThrown : public BamlError {
 public:
  BamlThrown(U thrown, std::string message, std::string class_name,
             std::string baml_trace, std::vector<uint8_t> payload)
      : BamlError(std::move(message), std::move(class_name),
                  std::move(baml_trace), std::move(payload)),
        value(std::move(thrown)) {}

  U value;
};

// Cancellation surfaces as the engine panic `baml.panics.Cancelled`.
class BamlCancelled : public BamlPanic {
 public:
  using BamlPanic::BamlPanic;
};

}  // namespace baml

#endif  // BAML_ERRORS_H_
