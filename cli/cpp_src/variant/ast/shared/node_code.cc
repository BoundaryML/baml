#include "variant/ast/shared/node_code.h"

#include <string>

#include "variant/ast/utils.h"
#include "variant/tokenizer/tokenizer.h"

namespace gloo::AST {
Language fromLangToken(const Tokenizer::Token &tk) {
  // Find the lang type by looking for the word in the brackets.
  auto bracket_start = tk.value.find('[');
  auto bracket_end = tk.value.rfind(']');
  if (bracket_start == std::string::npos || bracket_end == std::string::npos) {
    throw std::runtime_error("Invalid language name: " + tk.value);
  }
  auto lang =
      tk.value.substr(bracket_start + 1, bracket_end - bracket_start - 1);

  if (lang == "py") {
    return Language::PYTHON;
  } else if (lang == "ts") {
    return Language::TYPESCRIPT;
  } else {
    throw std::runtime_error("Unknown language: " + lang);
  }
}

CodeNode CodeNode::Parser(std::vector<Tokenizer::Token>::const_iterator &it) {
  ensureTokenKind(*it, Tokenizer::TokenKind::Lang);
  const Tokenizer::Token &start_token = *it;
  Language language = fromLangToken(*it++);
  std::string code = ParseString(it);
  return {start_token, language, code};
}

}  // namespace gloo::AST