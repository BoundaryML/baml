#include "variant/ast/types/node_enum.h"

#include <map>
#include <sstream>
#include <unordered_set>

#include "variant/ast/utils.h"
#include "variant/common.h"

namespace gloo::AST {
using namespace Tokenizer;

std::string EnumNode::toString() const {
  std::stringstream ss;
  ss << "Enum: " << name << std::endl;
  for (const auto &value : values) {
    ss << "  " << value << std::endl;
  }
  return ss.str();
}

std::shared_ptr<EnumNode> EnumNode::Parser(
    std::vector<Tokenizer::Token>::const_iterator &it) {
  /* Enums are of the form:
   * enum <name> {
   *   values {
   *      <name>
   *      <name>
   *   }
   * }
   */
  const Tokenizer::Token &start_token = *it;
  ensureTokenKind(*it++, TokenKind::EnumKeyword);
  const std::string name = ParseName(it);
  ensureTokenKind(*it++, TokenKind::LeftCurlyBracket);
  std::map<std::string, std::vector<Tokenizer::Token>> values;
  while (it->kind == Tokenizer::TokenKind::Identifier) {
    auto &token = *it;
    values[ParseName(it)].push_back(token);
  }
  ensureTokenKind(*it++, TokenKind::RightCurlyBracket);

  // If there are duplicate values, throw an error.
  for (const auto &[key, val] : values) {
    if (val.size() > 1) {
      throw DuplicateError(val, "Duplicate value in enum: " + key);
    }
  }

  std::vector<std::string> values_str;
  for (const auto &[key, val] : values) {
    values_str.push_back(key);
  }

  return std::shared_ptr<EnumNode>(new EnumNode(start_token, name, values_str));
}

void EnumNode::validate() const {
  if (values.size() == 0) {
    throw SyntaxError(token, "Enum must have at least one value.");
  }
}
}  // namespace gloo::AST
