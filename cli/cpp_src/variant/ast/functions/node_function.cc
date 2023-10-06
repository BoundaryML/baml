#include "variant/ast/functions/node_function.h"

#include <sstream>

#include "variant/ast/utils.h"

namespace gloo::AST {
using namespace Tokenizer;

std::string FunctionNode::toString() const {
  std::stringstream ss;
  ss << "Function: " << name << std::endl;
  ss << "  Input: " << *input << std::endl;
  ss << "  Output: " << *output << std::endl;
  return ss.str();
}

std::shared_ptr<FunctionNode> FunctionNode::Parser(
    std::vector<Token>::const_iterator &it) {
  const Tokenizer::Token &start_token = *it;
  ensureTokenKind(*it++, TokenKind::FunctionKeyword);
  std::string name = ParseName(it);
  ensureTokenKind(*it++, TokenKind::LeftCurlyBracket);

  ensureTokenKind(*it++, Tokenizer::TokenKind::AtSymbol);
  ensureTokenKind(*it++, TokenKind::InputKeyword);
  auto input = TypeNode::Parser(it);

  ensureTokenKind(*it++, Tokenizer::TokenKind::AtSymbol);
  ensureTokenKind(*it++, TokenKind::OutputKeyword);
  auto output = TypeNode::Parser(it);

  ensureTokenKind(*it++, TokenKind::RightCurlyBracket);

  return std::shared_ptr<FunctionNode>(
      new FunctionNode(start_token, name, input, output));
}

void FunctionNode::validate(
    const std::unordered_set<std::string> &class_names,
    const std::unordered_set<std::string> &enum_names) const {
  input->validate(class_names, enum_names);
  output->validate(class_names, enum_names);
}

}  // namespace gloo::AST
