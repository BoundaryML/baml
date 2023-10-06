#pragma once

#include <iostream>
#include <string>

#include "variant/generate/generate.h"
#include "variant/tokenizer/tokenizer.h"

namespace gloo::AST {
class AstNode {
 public:
  AstNode(const Tokenizer::Token &token) : token(token) {}

  const Tokenizer::Token token;

  virtual ~AstNode() = default;

  virtual std::string toString() const = 0;

  friend std::ostream &operator<<(std::ostream &os, const AstNode &node) {
    os << node.toString();
    return os;
  }
};

enum NodeOrder {
  ENUM = 1,
  CLASS,
  LLM_CLIENT,
  FUNCTION,
  VARIANT,
  TEST_GROUP,
};

class OutputNode : public AstNode, public Generate::PythonImpl {
 public:
  OutputNode(const Tokenizer::Token &token, const std::string &name)
      : AstNode(token), name(name) {}

  virtual NodeOrder order() const = 0;
  virtual std::string uniqueName() const { return name; }
  const std::string name;
};
}  // namespace gloo::AST