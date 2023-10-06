#pragma once
#include <string>

#include "variant/ast/node.h"
#include "variant/tokenizer/tokenizer.h"

namespace gloo::AST {

enum Language {
  PYTHON,
  TYPESCRIPT,
};

class CodeNode : public AstNode {
 public:
  CodeNode(const Tokenizer::Token token, const Language &language,
           const std::string &code)
      : AstNode(token), language(language), code(code) {}

  Language language;
  std::string code;

  std::string toString() const { return code; }

  // TODO: do some syntax checking
  void validate() const {}

  static CodeNode Parser(std::vector<Tokenizer::Token>::const_iterator &it);
};

}  // namespace gloo::AST