#pragma once
#include <ostream>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <vector>

#include "variant/ast/functions/tests/test.h"
#include "variant/ast/functions/variants/variant_base.h"
#include "variant/ast/node.h"
#include "variant/ast/shared/node_type.h"
#include "variant/tokenizer/tokenizer.h"

namespace gloo::AST {

class FunctionNode : public OutputNode {
 public:
  FunctionNode(const Tokenizer::Token token, const std::string &name,
               const std::shared_ptr<TypeNode> &input,
               const std::shared_ptr<TypeNode> &output)
      : OutputNode(token, name), input(input), output(output) {}
  NodeOrder order() const { return NodeOrder::FUNCTION; }
  PYTHONIC();

  void link(const std::vector<std::shared_ptr<ClassNode>> &classes,
            const std::vector<std::shared_ptr<EnumNode>> &enums) {
    input->link(classes, enums);
    output->link(classes, enums);
  }

  const std::shared_ptr<TypeNode> input;
  const std::shared_ptr<TypeNode> output;
  std::vector<std::shared_ptr<VariantBaseNode>> variants;
  std::vector<std::shared_ptr<TestGroupNode>> test_groups;

  std::string toString() const;

  void validate(const std::unordered_set<std::string> &class_names,
                const std::unordered_set<std::string> &enum_names) const;

  void addVariant(std::shared_ptr<VariantBaseNode> node) {
    variants.push_back(node);
    node->function = this;
  }
  void addTestGroup(std::shared_ptr<TestGroupNode> node) {
    test_groups.push_back(node);
    node->function = this;
  }

  static std::shared_ptr<FunctionNode> Parser(
      std::vector<Tokenizer::Token>::const_iterator &it);

  std::vector<std::string> dependencies() const {
    std::vector<std::string> deps;
    auto input_deps = input->dependencies();
    deps.insert(deps.end(), input_deps.begin(), input_deps.end());
    auto output_deps = output->dependencies();
    deps.insert(deps.end(), output_deps.begin(), output_deps.end());
    return deps;
  }
};

}  // namespace gloo::AST
