#pragma once

#include <ostream>
#include <string>
#include <unordered_set>
#include <vector>

#include "variant/ast/node.h"
#include "variant/ast/shared/node_code.h"
#include "variant/ast/shared/node_method.h"
#include "variant/ast/shared/node_type.h"
#include "variant/tokenizer/tokenizer.h"

namespace gloo::AST {

class PropertyNode : public AstNode {
 public:
  PropertyNode(const Tokenizer::Token &token, const std::string &name,
               const std::shared_ptr<TypeNode> &type)
      : AstNode(token), name(name), type(type) {}

  const std::string name;
  const std::shared_ptr<TypeNode> type;

  std::string toString() const;

  static PropertyNode Parser(std::vector<Tokenizer::Token>::const_iterator &it);

  void validate(const std::unordered_set<std::string> &class_names,
                const std::unordered_set<std::string> &enum_names) const;

  void link(const std::vector<std::shared_ptr<ClassNode>> &classes,
            const std::vector<std::shared_ptr<EnumNode>> &enums) {
    type->link(classes, enums);
  }

  std::string pythonDescription() const;
};

class ClassNode : public OutputNode {
 public:
  ClassNode(const Tokenizer::Token token, const std::string &name,
            const std::vector<PropertyNode> &properties,
            const std::vector<MethodNode> &methods)
      : OutputNode(token, name), properties(properties), methods(methods) {}

  PYTHONIC();

  NodeOrder order() const { return NodeOrder::CLASS; }

  std::vector<PropertyNode> properties;
  std::vector<MethodNode> methods;

  std::string toString() const;

  void validate(const std::unordered_set<std::string> &class_names,
                const std::unordered_set<std::string> &enum_names) const;

  std::vector<std::string> dependencies() const;

  static std::shared_ptr<ClassNode> Parser(
      std::vector<Tokenizer::Token>::const_iterator &it);

  void link(const std::vector<std::shared_ptr<ClassNode>> &classes,
            const std::vector<std::shared_ptr<EnumNode>> &enums) {
    for (auto &property : properties) {
      property.link(classes, enums);
    }
  }
};

}  // namespace gloo::AST