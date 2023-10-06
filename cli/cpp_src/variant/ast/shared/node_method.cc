#include "variant/ast/shared/node_method.h"

#include <ostream>
#include <sstream>
#include <string>
#include <vector>

#include "variant/ast/utils.h"
#include "variant/tokenizer/tokenizer.h"

namespace gloo::AST {
std::string MethodNode::toString() const {
  std::stringstream ss;
  ss << "Method: " << name << std::endl;
  for (const auto &lang : langs) {
    ss << lang << std::endl;
  }
  return ss.str();
}

std::shared_ptr<MethodNode> MethodNode::Parser(
    std::vector<Tokenizer::Token>::const_iterator &it) {
  ensureTokenKind(*it, Tokenizer::TokenKind::MethodKeyword);
  const Tokenizer::Token &start_token = *it;
  it++;
  ensureTokenKind(*it, Tokenizer::TokenKind::Identifier);
  std::string name = it->value;
  it++;

  ensureTokenKind(*it++, Tokenizer::TokenKind::LeftCurlyBracket);

  std::vector<CodeNode> langs;
  while (it->kind == Tokenizer::TokenKind::AtSymbol) {
    ++it;
    langs.push_back(CodeNode::Parser(it));
  }

  ensureTokenKind(*it++, Tokenizer::TokenKind::RightCurlyBracket);
  return std::shared_ptr<MethodNode>(new MethodNode(start_token, name, langs));
}

void MethodNode::validate() const {
  // Method should have at least one variant.
  if (langs.size() == 0) {
    throw SyntaxError(token, "Method must have at least one lang.");
  }

  for (auto &lang : langs) {
    lang.validate();
  }
}

}  // namespace gloo::AST