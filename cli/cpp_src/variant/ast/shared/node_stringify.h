#pragma once
#include <memory>
#include <sstream>
#include <string>
#include <unordered_set>

#include "variant/ast/node.h"
#include "variant/error.h"
#include "variant/tokenizer/tokenizer.h"

namespace gloo::Python {
std::string AsValue(const std::string &value);
}

namespace gloo::AST {

class StringifyPropertyNode : public AstNode {
 public:
  StringifyPropertyNode(const Tokenizer::Token &token, const std::string &name,
                        const std::optional<std::string> &rename,
                        const std::optional<std::string> &describe, bool skip)
      : AstNode(token),
        name(name),
        rename(rename),
        describe(describe),
        skip(skip) {}

  const std::string name;
  const std::optional<std::string> rename;
  const std::optional<std::string> describe;
  const bool skip;
  const std::string pyString() const {
    std::string res = name + "= StringifyRemappedField(";
    if (skip) {
      res += "skip=True,";
    } else {
      if (rename.has_value()) {
        res += "rename=" + Python::AsValue(rename.value()) + ",";
      }
      if (describe.has_value()) {
        res += "describe=" + Python::AsValue(describe.value()) + ",";
      }
    }
    res += ")";
    return res;
  }

  std::string toString() const {
    std::stringstream ss;
    ss << "  " << name;
    if (skip) {
      ss << " [skipped]";
    } else {
      if (rename.has_value()) {
        ss << " [aliased to] " << rename.value();
      }
      if (describe.has_value()) {
        ss << " [described as] " << describe.value();
      }
    }
    return ss.str();
  }

  static std::shared_ptr<StringifyPropertyNode> Parser(
      std::vector<Tokenizer::Token>::const_iterator &it);
};

class StringifyNode : public AstNode {
 public:
  StringifyNode(
      const Tokenizer::Token token, const std::string &type_name,
      const std::vector<std::shared_ptr<StringifyPropertyNode>> &properties)
      : AstNode(token), type_name(type_name), properties(properties) {}

  const std::string type_name;
  const std::vector<std::shared_ptr<StringifyPropertyNode>> properties;

  std::string toString() const {
    std::stringstream ss;
    ss << "Stringify " << type_name << " {" << std::endl;
    for (const auto &prop : properties) {
      ss << prop << std::endl;
    }
    ss << "}";
    return ss.str();
  }

  const std::string pyString() const {
    std::string params = "";
    for (const auto &prop : properties) {
      params += prop->pyString() + ",";
    }
    return params;
  }

  static std::shared_ptr<StringifyNode> Parser(
      std::vector<Tokenizer::Token>::const_iterator &it);
};
}  // namespace gloo::AST
