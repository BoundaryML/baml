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
class error : public std::runtime_error {
 public:
  explicit error(std::string message)
      : error(std::move(message), std::string(), std::string(), {}) {}

  error(std::string message, std::string class_name, std::string baml_trace,
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
  // instantiating these requires the generated codecs.
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
class panic : public error {
 public:
  using error::error;
};

// A thrown BAML value decoded into the function's declared `throws` set:
// generated bindings throw thrown<baml::variant<A, B>> when the error
// arm decodes into that set, so a catch site reads the typed payload with
// baml::match instead of probing is<T>()/get<T>(). The template argument
// is order-canonical (baml::variant), so thrown<variant<A, B>> and
// thrown<variant<B, A>> are the same catchable type. Derives error:
// untyped catch sites keep working, and an undeclared thrown value (one
// outside the declared set) still surfaces as a plain error.
template <class U>
class thrown : public error {
 public:
  thrown(U thrown, std::string message, std::string class_name,
         std::string baml_trace, std::vector<uint8_t> payload)
      : error(std::move(message), std::move(class_name), std::move(baml_trace),
              std::move(payload)),
        value(std::move(thrown)) {}

  U value;
};

// Cancellation surfaces as the engine panic `baml.panics.Cancelled`.
class cancelled : public panic {
 public:
  using panic::panic;
};

}  // namespace baml

#endif  // BAML_ERRORS_H_
