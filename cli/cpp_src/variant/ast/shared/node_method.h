#pragma once
#include <ostream>
#include <string>
#include <unordered_set>
#include <vector>

#include "variant/ast/node.h"
#include "variant/ast/shared/node_code.h"
#include "variant/tokenizer/tokenizer.h"

namespace gloo::AST {

class MethodNode : public AstNode {
 public:
  MethodNode(const Tokenizer::Token token, const std::string &name,
             const std::vector<CodeNode> &langs)
      : AstNode(token), name(name), langs(langs) {}

  const std::string name;
  const std::vector<CodeNode> langs;

  std::string toString() const;
  std::string toPyString(bool with_usage) const;

  static std::shared_ptr<MethodNode> Parser(
      std::vector<Tokenizer::Token>::const_iterator &it);

  void validate() const;
};

}  // namespace gloo::AST