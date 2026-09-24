#ifndef BAML_BIGINT_H_
#define BAML_BIGINT_H_

#include <ostream>
#include <string>
#include <utility>

namespace baml {

// An arbitrary-precision integer represented by decimal text.
// Arithmetic is intentionally outside the SDK boundary type's scope.
class bigint {
 public:
  /// Constructs a bigint from its decimal representation.
  explicit bigint(std::string value) : value_(std::move(value)) {}
  /// Constructs a bigint from its decimal representation.
  explicit bigint(const char* value) : value_(value) {}

  /// Returns the decimal representation used at the BAML boundary.
  const std::string& str() const noexcept { return value_; }

  /// Compares two bigint values by their decimal representation.
  friend bool operator==(const bigint& lhs, const bigint& rhs) {
    return lhs.value_ == rhs.value_;
  }
  /// Compares two bigint values by their decimal representation.
  friend bool operator!=(const bigint& lhs, const bigint& rhs) {
    return !(lhs == rhs);
  }
  /// Writes the decimal representation to a stream.
  friend std::ostream& operator<<(std::ostream& out, const bigint& value) {
    return out << value.value_;
  }

 private:
  std::string value_;
};

}  // namespace baml

#endif  // BAML_BIGINT_H_
