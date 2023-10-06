#pragma once
#include <iostream>
#include <string>

#include "variant/error.h"
#include "variant/tokenizer/tokenizer.h"

namespace gloo::AST {
// Make some general error for syntax errors
inline void ensureTokenKind(const Tokenizer::Token &token,
                            Tokenizer::TokenKind expectedKind) {
  if (token.kind != expectedKind) {
    throw SyntaxError(token, "Expected " +
                                 Tokenizer::TokenKindToString(expectedKind) +
                                 " Got: " + token.value + " " +
                                 Tokenizer::TokenKindToString(token.kind) + "");
  }
}

std::string ParseString(std::vector<Tokenizer::Token>::const_iterator &it);
std::vector<std::string> ParseIdentifierList(
    std::vector<Tokenizer::Token>::const_iterator &it);
std::string ParseName(std::vector<Tokenizer::Token>::const_iterator &it);

}  // namespace gloo::AST