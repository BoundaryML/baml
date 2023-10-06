#include "variant/ast/shared/node_stringify.h"

#include "variant/ast/utils.h"

namespace gloo::AST {

std::shared_ptr<StringifyNode> StringifyNode::Parser(
    std::vector<Tokenizer::Token>::const_iterator &it) {
  const auto &start_token = *it;
  ensureTokenKind(*it++, Tokenizer::TokenKind::StringifyKeyword);
  const auto &type_name = ParseName(it);
  ensureTokenKind(*it++, Tokenizer::TokenKind::LeftCurlyBracket);
  std::vector<std::shared_ptr<StringifyPropertyNode>> properties;
  while (it->kind == Tokenizer::TokenKind::Identifier) {
    properties.push_back(StringifyPropertyNode::Parser(it));
  }
  ensureTokenKind(*it++, Tokenizer::TokenKind::RightCurlyBracket);
  return std::shared_ptr<StringifyNode>(
      new StringifyNode(start_token, type_name, properties));
}

std::shared_ptr<StringifyPropertyNode> StringifyPropertyNode::Parser(
    std::vector<Tokenizer::Token>::const_iterator &it) {
  const auto &start_token = *it;
  const auto &name = ParseName(it);
  std::optional<std::string> rename;
  std::optional<std::string> describe;
  bool skip = false;

  while (it->kind == Tokenizer::TokenKind::AtSymbol) {
    ++it;
    switch (it->kind) {
      case Tokenizer::TokenKind::AliasKeyword: {
        ensureTokenKind(*it++, Tokenizer::TokenKind::AliasKeyword);
        rename = ParseString(it);
        break;
      }
      case Tokenizer::TokenKind::DescriptionKeyword: {
        ensureTokenKind(*it++, Tokenizer::TokenKind::DescriptionKeyword);
        describe = ParseString(it);
        break;
      }
      case Tokenizer::TokenKind::SkipKeyword: {
        skip = true;
        ++it;
        break;
      }
      default:
        throw SyntaxError(*it, "Unknown property");
    }
  }

  return std::shared_ptr<StringifyPropertyNode>(
      new StringifyPropertyNode(start_token, name, rename, describe, skip));
}
}  // namespace gloo::AST
