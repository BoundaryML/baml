#pragma once
#include <unordered_set>

#include "variant/ast/functions/variants/variant_base.h"
#include "variant/ast/shared/node_method.h"
#include "variant/ast/shared/node_stringify.h"

namespace gloo::AST {
class LLMVariantNode : public VariantBaseNode {
 public:
  LLMVariantNode(const Tokenizer::Token &token, const std::string &name,
                 const std::string &functionName,
                 const std::string &client_name, const std::string &prompt,
                 const std::vector<std::shared_ptr<StringifyNode>> &stringify,
                 const std::vector<std::shared_ptr<MethodNode>> &methods)
      : VariantBaseNode(token, name, functionName),
        client_name(client_name),
        prompt(prompt),
        stringify(stringify),
        methods(methods) {}

  const std::string client_name;
  const std::string prompt;
  const std::vector<std::shared_ptr<StringifyNode>> stringify;
  const std::vector<std::shared_ptr<MethodNode>> methods;

  PYTHONIC();
  virtual std::vector<std::string> dependencies() const;

  virtual std::string type() const { return "llm"; }

  void validate(const std::unordered_set<std::string> &,
                const std::unordered_set<std::string> &,
                const std::unordered_set<std::string> &,
                const std::unordered_set<std::string> &client_names) const;

  static std::vector<std::shared_ptr<LLMVariantNode>> Parser(
      const std::string &functionName, const std::string &variantName,
      std::vector<Tokenizer::Token>::const_iterator &it);
};

}  // namespace gloo::AST