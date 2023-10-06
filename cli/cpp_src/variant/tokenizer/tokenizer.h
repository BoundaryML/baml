#pragma once
#include <string>
#include <vector>

namespace gloo::Tokenizer {
enum TokenKind {
  RightCurlyBracket,
  LeftCurlyBracket,
  Colon,
  Comma,
  AtSymbol,

  EnumKeyword,

  MethodKeyword,

  ClassKeyword,

  FunctionKeyword,
  InputKeyword,
  OutputKeyword,

  ClientKeyword,
  ProviderKeyword,
  RetryKeyword,
  FallbackKeyword,

  VariantKeyword,
  PromptKeyword,
  StringifyKeyword,
  AliasKeyword,
  DescriptionKeyword,
  SkipKeyword,

  DependsOnKeyword,

  TestGroupKeyword,
  TestCaseKeyword,
  // Language specific
  Lang,
  // Catch all
  Identifier,
  // End of file
  Eof,
};

struct Token {
  std::string file;
  int line;
  int column;
  TokenKind kind;
  std::string value;
};

std::vector<Token> Tokenize(const std::string& file, const std::string& str);

std::string TokenKindToString(TokenKind kind);
}  // namespace gloo::Tokenizer
