#include "variant/ast/utils.h"

#include <string>
#include <vector>

#include "variant/tokenizer/tokenizer.h"

namespace gloo::AST {
std::string TokensToString(const std::vector<Tokenizer::Token> &tokens) {
  if (tokens.size() == 0) {
    return "";
  }

  std::string result = "";
  int lastLine = tokens[0].line;
  const int dedentCount = tokens[0].column;
  int lastColumn = dedentCount;
  for (const Tokenizer::Token &token : tokens) {
    if (token.line > lastLine) {
      result += std::string(token.line - lastLine, '\n');
      lastColumn = dedentCount;
      lastLine = token.line;
    }

    if (token.column < lastColumn) {
      throw SyntaxError(token,
                        "Strings must be indented to match the first line.");
    }

    if (token.column > lastColumn) {
      result += std::string(token.column - lastColumn, ' ');
    }
    result += token.value;
    lastColumn = token.column + static_cast<int>(token.value.length());
  }
  return result;
}

std::string ParseMultiLineString(
    std::vector<Tokenizer::Token>::const_iterator &it) {
  ensureTokenKind(*it++, Tokenizer::TokenKind::LeftCurlyBracket);
  int bracketCount = 1;
  std::vector<Tokenizer::Token> tokens;
  while (bracketCount > 0) {
    if (it->kind == Tokenizer::TokenKind::Eof) {
      throw SyntaxError(*it, "Missing closing '}'");
    }
    if (it->kind == Tokenizer::TokenKind::LeftCurlyBracket) {
      bracketCount++;
    } else if (it->kind == Tokenizer::TokenKind::RightCurlyBracket) {
      bracketCount--;
    }

    if (bracketCount > 0) {
      tokens.push_back(*it);
    }

    // Check it is not the last token
    it++;
  }

  return TokensToString(tokens);
}

std::string ParseSingleLineString(
    int line, std::vector<Tokenizer::Token>::const_iterator &it) {
  std::vector<Tokenizer::Token> tokens;
  while (it->line == line) {
    tokens.push_back(*it++);
  }
  return TokensToString(tokens);
}

std::string ParseString(std::vector<Tokenizer::Token>::const_iterator &it) {
  if (it->kind == Tokenizer::TokenKind::LeftCurlyBracket) {
    return ParseMultiLineString(it);
  } else {
    return ParseSingleLineString(it->line, it);
  }
}

std::vector<std::string> ParseIdentifierList(
    std::vector<Tokenizer::Token>::const_iterator &it) {
  std::vector<std::string> result;
  if (it->kind == Tokenizer::TokenKind::LeftCurlyBracket) {
    // TODO: Support parsing single line lists
    ensureTokenKind(*it++, Tokenizer::TokenKind::LeftCurlyBracket);
    while (it->kind != Tokenizer::TokenKind::RightCurlyBracket) {
      ensureTokenKind(*it, Tokenizer::TokenKind::Identifier);
      result.push_back(it->value);
      it++;
    }
    ensureTokenKind(*it++, Tokenizer::TokenKind::RightCurlyBracket);
  } else {
    const auto startLine = it->line;
    while (it->line == startLine) {
      ensureTokenKind(*it, Tokenizer::TokenKind::Identifier);
      result.push_back(it->value);
      it++;
    }
  }
  return result;
}

std::string ParseName(std::vector<Tokenizer::Token>::const_iterator &it) {
  ensureTokenKind(*it, Tokenizer::TokenKind::Identifier);
  std::string name = it->value;
  it++;
  return name;
}

}  // namespace gloo::AST
