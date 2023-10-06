#pragma once
#include <ostream>
#include <string>

#include "variant/tokenizer/tokenizer.h"

namespace gloo {
class GlooError {
 public:
  GlooError(const std::string &msg) : msg(msg) {}
  ~GlooError() {}

  std::string msg;

  virtual std::string what() const { return msg; }
};

class UndefinedError : public GlooError {
 public:
  UndefinedError(const Tokenizer::Token &token, const std::string &msg)
      : GlooError(msg), token(token) {}

  const Tokenizer::Token token;

  virtual std::string what() const {
    return token.file + ":" + std::to_string(token.line) + ":" +
           std::to_string(token.column) + ": Undefined Error: " + msg;
  }
};

class SyntaxError : public GlooError {
 public:
  SyntaxError(const Tokenizer::Token &token, const std::string &msg)
      : GlooError(msg), token(token) {}

  const Tokenizer::Token token;

  virtual std::string what() const {
    return token.file + ":" + std::to_string(token.line) + ":" +
           std::to_string(token.column) + ": Syntax Error: " + msg;
  }
};

class DuplicateError : public GlooError {
 public:
  DuplicateError(const std::vector<Tokenizer::Token> &tokens,
                 const std::string &msg)
      : GlooError(msg), tokens(tokens) {}

  const std::vector<Tokenizer::Token> tokens;

  virtual std::string what() const {
    std::string message = "Duplicate Error: " + msg + "\n";
    for (const auto &token : tokens) {
      message += "\tFound at: " + token.file + ":" +
                 std::to_string(token.line) + ":" +
                 std::to_string(token.column) + "\n";
    }
    return message;
  }
};

class CircularDependencyError : public GlooError {
 public:
  CircularDependencyError(const Tokenizer::Token &token, const std::string &msg)
      : GlooError(msg), token(token) {}

  const Tokenizer::Token token;

  virtual std::string what() const {
    return token.file + ":" + std::to_string(token.line) + ":" +
           std::to_string(token.column) + ": Circular dependency found\n\t" +
           msg;
  }
};
}  // namespace gloo