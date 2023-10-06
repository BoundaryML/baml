#include "variant/ast/types/node_class.h"

#include <ostream>
#include <sstream>
#include <string>
#include <vector>

#include "variant/ast/utils.h"
#include "variant/tokenizer/tokenizer.h"

namespace gloo::AST {

std::string PropertyNode::toString() const {
  return "Property " + name + ": " + type->toString();
}

PropertyNode PropertyNode::Parser(
    std::vector<Tokenizer::Token>::const_iterator &it) {
  const Tokenizer::Token &start_token = *it;
  auto name = ParseName(it);
  auto type = TypeNode::Parser(it);
  return PropertyNode(start_token, name, type);
}

std::string ClassNode::toString() const {
  std::stringstream ss;
  ss << "Class: " << name << std::endl;
  for (auto &property : properties) {
    ss << property << std::endl;
  }
  for (auto &method : methods) {
    ss << method;
  }
  return ss.str();
}

std::shared_ptr<ClassNode> ClassNode::Parser(
    std::vector<Tokenizer::Token>::const_iterator &it) {
  const Tokenizer::Token &class_token = *it;
  ensureTokenKind(*it++, Tokenizer::TokenKind::ClassKeyword);
  const std::string name = ParseName(it);
  ensureTokenKind(*it++, Tokenizer::TokenKind::LeftCurlyBracket);

  std::vector<PropertyNode> properties;
  std::vector<MethodNode> methods;

  while (it->kind == Tokenizer::TokenKind::AtSymbol ||
         it->kind == Tokenizer::TokenKind::Identifier) {
    switch (it->kind) {
      case Tokenizer::TokenKind::Identifier:
        properties.push_back(PropertyNode::Parser(it));
        break;
      case Tokenizer::TokenKind::AtSymbol: {
        ++it;
        // For now we only support methods.
        methods.push_back(*MethodNode::Parser(it));
        break;
      }
      default:
        throw SyntaxError(*it, "Expected " +
                                   Tokenizer::TokenKindToString(
                                       Tokenizer::TokenKind::Identifier) +
                                   " or " +
                                   Tokenizer::TokenKindToString(
                                       Tokenizer::TokenKind::MethodKeyword) +
                                   ": Got[" + it->value + "]");
    }
  }

  ensureTokenKind(*it++, Tokenizer::TokenKind::RightCurlyBracket);

  return std::shared_ptr<ClassNode>(
      new ClassNode(class_token, name, properties, methods));
}

void PropertyNode::validate(
    const std::unordered_set<std::string> &class_names,
    const std::unordered_set<std::string> &enum_names) const {
  type->validate(class_names, enum_names);
}

void ClassNode::validate(
    const std::unordered_set<std::string> &class_names,
    const std::unordered_set<std::string> &enum_names) const {
  std::unordered_set<std::string> names;
  for (auto &property : properties) {
    if (names.find(property.name) != names.end()) {
      throw DuplicateError({token},
                           "Duplicate property name: " + property.name);
    }
    names.insert(property.name);
    property.validate(class_names, enum_names);
  }
  for (auto &method : methods) {
    if (names.find(method.name) != names.end()) {
      throw DuplicateError({token}, "Duplicate method name: " + method.name);
    }
    names.insert(method.name);
    method.validate();
  }
}

std::vector<std::string> ClassNode::dependencies() const {
  std::vector<std::string> deps;
  for (auto &property : properties) {
    auto type_deps = property.type->dependencies();
    deps.insert(deps.end(), type_deps.begin(), type_deps.end());
  }
  return deps;
}
}  // namespace gloo::AST