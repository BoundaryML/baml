#pragma once
#include <unordered_set>

#include "variant/ast/functions/variants/variant_base.h"
#include "variant/error.h"

namespace gloo::AST {

class CodeVariantNode : public VariantBaseNode {
 public:
  CodeVariantNode(const Tokenizer::Token &token, const std::string &name,
                  const std::string &functionName,
                  const std::vector<std::string> &usedFunction,
                  const std::vector<std::shared_ptr<MethodNode>> &methods)
      : VariantBaseNode(token, name, functionName),
        usedFunction(usedFunction),
        methods(methods) {}
  PYTHONIC();

  virtual std::vector<std::string> dependencies() const;

  const std::vector<std::string> usedFunction;
  const std::vector<std::shared_ptr<MethodNode>> methods;

  virtual std::string type() const { return "code"; }

  void validate(const std::unordered_set<std::string> &,
                const std::unordered_set<std::string> &,
                const std::unordered_set<std::string> &function_names,
                const std::unordered_set<std::string> &) const {
    for (const auto &func : usedFunction) {
      if (function_names.find(func) == function_names.end()) {
        throw UndefinedError(token, "Dependency not found: " + func);
      }
    }
  }

  static std::shared_ptr<CodeVariantNode> Parser(
      const std::string &functionName, const std::string &variantName,
      std::vector<Tokenizer::Token>::const_iterator &it);
};

}  // namespace gloo::AST