#pragma once
#include <ostream>
#include <string>
#include <vector>

#include "variant/ast/node.h"
#include "variant/tokenizer/tokenizer.h"

namespace gloo::AST {
class EnumNode : public OutputNode {
 public:
  EnumNode(const Tokenizer::Token token, const std::string &name,
           const std::vector<std::string> &values)
      : OutputNode(token, name), values(values) {}
  NodeOrder order() const { return NodeOrder::ENUM; }

  const std::vector<std::string> values;
  std::string toString() const;
  PYTHONIC();

  void validate() const;

  static std::shared_ptr<EnumNode> Parser(
      std::vector<Tokenizer::Token>::const_iterator &it);
};

}  // namespace gloo::AST