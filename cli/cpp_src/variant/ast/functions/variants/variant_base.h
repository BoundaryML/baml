#pragma once
#include <ostream>
#include <string>
#include <unordered_set>
#include <vector>

#include "variant/ast/node.h"
#include "variant/tokenizer/tokenizer.h"

namespace gloo::AST {

class FunctionNode;

class VariantBaseNode : public OutputNode {
 public:
  VariantBaseNode(const Tokenizer::Token &token, const std::string &name,
                  const std::string &functionName)
      : OutputNode(token, name), functionName(functionName) {}

  virtual ~VariantBaseNode() = default;

  virtual std::string type() const = 0;

  std::string toString() const {
    std::string result = functionName + "::" + name + "[" + type() + "]";
    return result;
  }
  NodeOrder order() const { return NodeOrder::VARIANT; }
  const std::string functionName;

  virtual std::vector<std::string> dependencies() const { return {}; }

  static std::vector<std::shared_ptr<VariantBaseNode>> Parser(
      std::vector<Tokenizer::Token>::const_iterator &it);
  virtual std::string uniqueName() const { return functionName + "::" + name; }

  virtual void validate(
      const std::unordered_set<std::string> &class_names,
      const std::unordered_set<std::string> &enum_names,
      const std::unordered_set<std::string> &function_names,
      const std::unordered_set<std::string> &client_names) const = 0;

  FunctionNode *function;  // This is a non-owning pointer
};

}  // namespace gloo::AST
