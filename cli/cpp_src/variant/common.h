#pragma once
#include <string>

#include "variant/error.h"

namespace gloo {

inline void validateTestCaseIdentifier(const Tokenizer::Token &token,
                                       const std::string &name) {
  // Check string is only alphanumeric and underscores.
  // First character must be a letter.
  if (name.length() == 0) {
    throw SyntaxError(token, "Invalid identifier: " + name);
  } else {
    for (char c : name) {
      if (!std::isalnum(c) && c != '_') {
        throw SyntaxError(token, "Invalid identifier: " + name);
      }
    }
  }
}

inline void validateIdentifier(const Tokenizer::Token &token,
                               const std::string &name) {
  // Check string is only alphanumeric and underscores.
  // First character must be a letter.
  if (name.length() == 0 || !std::isalpha(name[0])) {
    throw SyntaxError(token, "Invalid identifier: " + name);
  } else if (name.length() > 1) {
    for (char c : name) {
      if (!std::isalnum(c) && c != '_') {
        throw SyntaxError(token, "Invalid identifier: " + name);
      }
    }
  }
}
}  // namespace gloo