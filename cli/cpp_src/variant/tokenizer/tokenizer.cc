#include "variant/tokenizer/tokenizer.h"

#include <iostream>
#include <sstream>
#include <string>

#include "variant/error.h"

namespace gloo::Tokenizer {
TokenKind GetIdentifier(const std::string &str) {
  if (str == "enum") {
    return TokenKind::EnumKeyword;
  }
  if (str == "class") {
    return TokenKind::ClassKeyword;
  }
  if (str == "function") {
    return TokenKind::FunctionKeyword;
  }
  if (str == "method") {
    return TokenKind::MethodKeyword;
  }
  if (str == "prompt") {
    return TokenKind::PromptKeyword;
  }
  if (str == "input") {
    return TokenKind::InputKeyword;
  }
  if (str == "output") {
    return TokenKind::OutputKeyword;
  }
  if (str == "depends_on") {
    return TokenKind::DependsOnKeyword;
  }
  if (str == "method") {
    return TokenKind::MethodKeyword;
  }
  if (str.starts_with("lang[") && str.ends_with("]")) {
    return TokenKind::Lang;
  }
  if (str.starts_with("variant[") && str.ends_with("]")) {
    return TokenKind::VariantKeyword;
  }
  if (str == "test_group") {
    return TokenKind::TestGroupKeyword;
  }
  if (str == "case") {
    return TokenKind::TestCaseKeyword;
  }
  if (str.starts_with("client[") && str.ends_with("]")) {
    return TokenKind::ClientKeyword;
  }
  if (str == "provider") {
    return TokenKind::ProviderKeyword;
  }
  if (str == "rename") {
    return TokenKind::AliasKeyword;
  }
  if (str == "describe") {
    return TokenKind::DescriptionKeyword;
  }
  if (str == "skip") {
    return TokenKind::SkipKeyword;
  }
  if (str == "stringify") {
    return TokenKind::StringifyKeyword;
  }
  if (str == "retry") {
    // || (str.starts_with("retry[") && str.ends_with("]"))) {
    return TokenKind::RetryKeyword;
  }
  if (str == "fallback" ||
      (str.starts_with("fallback[") && str.ends_with("]"))) {
    return TokenKind::FallbackKeyword;
  }
  return TokenKind::Identifier;
}

std::vector<Token> Tokenize(const std::string &file, const std::string &str) {
  std::vector<Token> tokens;
  int line = 1;
  // Read the string one line at a time.
  std::istringstream iss(str);
  std::string line_str;
  while (std::getline(iss, line_str)) {
    int column = 1;
    // Read the line one character at a time until whitespace which would be a
    // token.
    std::string token_str;
    int atSymbolCol = -1;
    auto maybeAddIdentifier = [&]() {
      if (token_str.length() > 0) {
        TokenKind kind =
            atSymbolCol >= 0 ? GetIdentifier(token_str) : TokenKind::Identifier;
        tokens.push_back({file, line, column, kind, token_str});
        column += static_cast<int>(token_str.length());
        token_str.clear();
      }
      atSymbolCol = -1;
    };

    for (char c : line_str) {
      switch (c) {
        case ',':
          maybeAddIdentifier();
          tokens.push_back({file, line, column, TokenKind::Comma, ","});
          column++;
          break;
        case ':':
          maybeAddIdentifier();
          tokens.push_back({file, line, column, TokenKind::Colon, ":"});
          column++;
          break;
        case '{':
          maybeAddIdentifier();
          tokens.push_back(
              {file, line, column, TokenKind::LeftCurlyBracket, "{"});
          column++;
          break;
        case '}':
          maybeAddIdentifier();
          tokens.push_back(
              {file, line, column, TokenKind::RightCurlyBracket, "}"});
          column++;
          break;
        case '@':
          maybeAddIdentifier();
          tokens.push_back({file, line, column, TokenKind::AtSymbol, "@"});
          atSymbolCol = column;
          column++;
          break;
        case ' ':  // Whitespace
        case '\t':
        case '\n':
        case '\r':
          maybeAddIdentifier();
          column++;
          break;
        default:
          token_str += c;
          break;
      }
    }
    maybeAddIdentifier();
    line++;
  }
  tokens.push_back({file, line, 1, TokenKind::Eof, "[EOF]"});
  return tokens;
}

std::string TokenKindToString(TokenKind kind) {
  switch (kind) {
    case TokenKind::Comma:
      return "comma (,)";
    case TokenKind::Colon:
      return "colon (:)";
    case TokenKind::RightCurlyBracket:
      return "right curly bracket (})";
    case TokenKind::LeftCurlyBracket:
      return "left curly bracket ({)";
    case TokenKind::AtSymbol:
      return "at symbol (@)";
    case TokenKind::EnumKeyword:
      return "@enum";
    case TokenKind::ClassKeyword:
      return "@class";
    case TokenKind::FunctionKeyword:
      return "@function";
    case TokenKind::MethodKeyword:
      return "@method";
    case TokenKind::PromptKeyword:
      return "@prompt";
    case TokenKind::InputKeyword:
      return "@input";
    case TokenKind::OutputKeyword:
      return "@output";
    case TokenKind::DependsOnKeyword:
      return "@depends_on";
    case TokenKind::TestGroupKeyword:
      return "@test_group";
    case TokenKind::TestCaseKeyword:
      return "@case";
    case TokenKind::VariantKeyword:
      return "@variant[*]";
    case TokenKind::Lang:
      return "@lang[*]";
    case TokenKind::ClientKeyword:
      return "@client[*]";
    case TokenKind::ProviderKeyword:
      return "@provider";
    case TokenKind::AliasKeyword:
      return "@rename";
    case TokenKind::DescriptionKeyword:
      return "@describe";
    case TokenKind::SkipKeyword:
      return "@skip";
    case TokenKind::StringifyKeyword:
      return "@stringify";
    case TokenKind::RetryKeyword:
      return "@retry";
    case TokenKind::FallbackKeyword:
      return "@fallback[code]";
    case TokenKind::Identifier:
      return "[identifier]";
    // Add cases for any other TokenKinds you might have.
    default:
      return "unknown token";
  }
}

}  // namespace gloo::Tokenizer
