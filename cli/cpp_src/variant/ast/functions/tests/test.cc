#include "variant/ast/functions/tests/test.h"

#include "variant/ast/utils.h"
#include "variant/tokenizer/tokenizer.h"

namespace gloo::AST {

std::shared_ptr<TestGroupNode> TestGroupNode::Parser(
    std::vector<Tokenizer::Token>::const_iterator &it) {
  const auto &start_token = *it;
  ensureTokenKind(*it++, Tokenizer::TokenKind::TestGroupKeyword);
  const auto &name = ParseName(it);
  const auto &forToken = *it;
  const auto &forKeyword = ParseName(it);
  if (forKeyword != "for") {
    throw SyntaxError(forToken, "Expected 'for' keyword. Got: " + forKeyword);
  }
  const auto &functionName = ParseName(it);
  ensureTokenKind(*it++, Tokenizer::TokenKind::LeftCurlyBracket);

  std::vector<std::shared_ptr<TestCaseNode>> cases;
  std::vector<std::shared_ptr<MethodNode>> methods;
  while (it->kind == Tokenizer::TokenKind::AtSymbol) {
    ++it;
    switch (it->kind) {
      case Tokenizer::TokenKind::TestCaseKeyword:
        cases.push_back(TestCaseNode::Parser(cases.size(), it));
        break;
      case Tokenizer::TokenKind::InputKeyword: {
        const auto &token = *it;
        ensureTokenKind(*it++, Tokenizer::TokenKind::InputKeyword);
        auto value = ParseString(it);
        cases.push_back(std::shared_ptr<TestCaseNode>(new TestCaseNode(
            token, "case_" + std::to_string(cases.size()), value, {})));
        break;
      }
      case Tokenizer::TokenKind::MethodKeyword:
        methods.push_back(MethodNode::Parser(it));
        break;
      default:
        throw SyntaxError(
            *it, "Unexpected token parsing 'test_group': " + it->value);
    }
  }

  ensureTokenKind(*it++, Tokenizer::TokenKind::RightCurlyBracket);
  auto group = std::shared_ptr<TestGroupNode>(
      new TestGroupNode(start_token, name, functionName, cases, methods));
  return group;
}

std::shared_ptr<TestCaseNode> TestCaseNode::Parser(
    size_t index, std::vector<Tokenizer::Token>::const_iterator &it) {
  const auto &start_token = *it;
  ensureTokenKind(*it++, Tokenizer::TokenKind::TestCaseKeyword);
  std::string name = "case_" + std::to_string(index);
  if (it->kind == Tokenizer::TokenKind::Identifier) {
    name = it->value;
    it++;
  }
  ensureTokenKind(*it++, Tokenizer::TokenKind::LeftCurlyBracket);
  bool sawInput = false;
  std::string value = "";
  std::vector<std::shared_ptr<MethodNode>> methods;
  while (it->kind == Tokenizer::TokenKind::AtSymbol) {
    ++it;
    switch (it->kind) {
      case Tokenizer::TokenKind::InputKeyword:
        if (sawInput) {
          throw SyntaxError(*it, "Duplicate input.");
        }
        sawInput = true;
        value = ParseString(++it);
        break;
      case Tokenizer::TokenKind::MethodKeyword:
        methods.push_back(MethodNode::Parser(it));
        break;
      default:
        throw SyntaxError(*it, "Unexpected token parsing 'case': " + it->value);
    }
  }
  ensureTokenKind(*it++, Tokenizer::TokenKind::RightCurlyBracket);
  return std::shared_ptr<TestCaseNode>(
      new TestCaseNode(start_token, name, value, methods));
}

}  // namespace gloo::AST