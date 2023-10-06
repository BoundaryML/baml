#include "variant/ast/ast.h"

#include <iostream>
#include <ostream>

#include "variant/ast/utils.h"

namespace gloo::AST {
using namespace Tokenizer;

Nodes Parser(const std::vector<Token> &tokens) {
  Nodes nodes;

  auto it = tokens.begin();
  while (it->kind == TokenKind::AtSymbol) {
    ++it;
    // Parse the tokens
    switch (it->kind) {
      case TokenKind::EnumKeyword:
        nodes.enums.push_back(EnumNode::Parser(it));
        break;
      case TokenKind::ClassKeyword:
        nodes.classes.push_back(ClassNode::Parser(it));
        break;
      case TokenKind::FunctionKeyword:
        nodes.functions.push_back(FunctionNode::Parser(it));
        break;
      case TokenKind::VariantKeyword: {
        for (auto res : VariantBaseNode::Parser(it)) {
          nodes.function_variants[res->functionName].push_back(res);
        }
        break;
      }
      case TokenKind::TestGroupKeyword: {
        auto res = TestGroupNode::Parser(it);
        nodes.function_test_groups[res->functionName].push_back(res);
        break;
      }
      case TokenKind::ClientKeyword:
        nodes.clients.push_back(LLMClientNode::Parser(it));
        break;
      default:
        throw SyntaxError(*it, "Unexpected token: " + it->value);
    }
  }
  if (it->kind != TokenKind::Eof) {
    throw SyntaxError(*it, "Did you forget @? Got: " + it->value);
  }
  return nodes;
}
}  // namespace gloo::AST