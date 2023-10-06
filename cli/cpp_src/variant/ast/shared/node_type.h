#pragma once
#include <memory>
#include <sstream>
#include <string>
#include <unordered_set>

#include "variant/ast/node.h"
#include "variant/error.h"
#include "variant/tokenizer/tokenizer.h"

namespace gloo::AST {

class ClassNode;
class EnumNode;
class TypeType {
 public:
  TypeType(const Tokenizer::Token &token) : token(token) {}

  const Tokenizer::Token token;

  virtual ~TypeType() {}
  virtual std::string toPyString() const = 0;
  virtual std::string toPyDescription() const = 0;
  virtual std::string toString() const = 0;
  virtual bool isCustomType() const { return false; }
  virtual std::string defaultValue() const { return ""; }
  virtual std::vector<std::string> dependencies() const { return {}; }
  virtual void validate(const std::unordered_set<std::string> &,
                        const std::unordered_set<std::string> &) const = 0;
  virtual void link(const std::vector<std::shared_ptr<ClassNode>> &,
                    const std::vector<std::shared_ptr<EnumNode>> &) = 0;

  ClassNode *class_node = nullptr;
  EnumNode *enum_node = nullptr;
};

enum PrimitiveType {
  CHAR,
  STRING,
  INT,
  FLOAT,
  BOOL,
  NONE,
};

class TypeTypeOptional final : public TypeType {
 public:
  TypeTypeOptional(const Tokenizer::Token &token,
                   std::shared_ptr<TypeType> type)
      : TypeType(token), type(type) {}
  void link(const std::vector<std::shared_ptr<ClassNode>> &classes,
            const std::vector<std::shared_ptr<EnumNode>> &enums) {
    type->link(classes, enums);
  }
  virtual void validate(
      const std::unordered_set<std::string> &class_names,
      const std::unordered_set<std::string> &enum_names) const {
    type->validate(class_names, enum_names);
  }
  virtual std::string defaultValue() const { return " = None"; }

  virtual std::string toString() const {
    return "Optional[" + type->toString() + "]";
  }

  std::string toPyDescription() const {
    return "StringifyOptional(" + type->toPyDescription() + ")";
  }

  std::string toPyString() const {
    return "typing.Optional[" + type->toPyString() + "]";
  }

  virtual std::vector<std::string> dependencies() const {
    return type->dependencies();
  }

  // shared_ptr to TypeType
  const std::shared_ptr<TypeType> type;
};

class TypeTypePrimitive final : public TypeType {
 public:
  TypeTypePrimitive(const Tokenizer::Token &token, const PrimitiveType &type)
      : TypeType(token), type(type) {}
  void link(const std::vector<std::shared_ptr<ClassNode>> &,
            const std::vector<std::shared_ptr<EnumNode>> &) {}

  virtual std::string toString() const {
    switch (type) {
      case PrimitiveType::CHAR:
        return "char";
      case PrimitiveType::STRING:
        return "string";
      case PrimitiveType::INT:
        return "int";
      case PrimitiveType::FLOAT:
        return "float";
      case PrimitiveType::BOOL:
        return "bool";
      case PrimitiveType::NONE:
        return "null";
      default:
        throw SyntaxError(token, "Unknown primitive type");
    }
  }

  std::string toPyDescription() const {
    switch (type) {
      case PrimitiveType::CHAR:
        return "StringifyChar()";
      case PrimitiveType::STRING:
        return "StringifyString()";
      case PrimitiveType::INT:
        return "StringifyInt()";
      case PrimitiveType::FLOAT:
        return "StringifyFloat()";
      case PrimitiveType::BOOL:
        return "StringifyBool()";
      case PrimitiveType::NONE:
        return "StringifyNone()";
      default:
        throw SyntaxError(token, "Unknown primitive type");
    }
  }

  std::string toPyString() const {
    switch (type) {
      case PrimitiveType::CHAR:
        return "str";
      case PrimitiveType::STRING:
        return "str";
      case PrimitiveType::INT:
        return "int";
      case PrimitiveType::FLOAT:
        return "float";
      case PrimitiveType::BOOL:
        return "bool";
      case PrimitiveType::NONE:
        return "None";
      default:
        throw SyntaxError(token, "Unknown primitive type");
    }
  }

  const PrimitiveType type;

  void validate(const std::unordered_set<std::string> &,
                const std::unordered_set<std::string> &) const {}
};

class TypeTypeRef final : public TypeType {
 public:
  TypeTypeRef(const Tokenizer::Token &token, const std::string &name)
      : TypeType(token), name(name) {}
  void link(const std::vector<std::shared_ptr<ClassNode>> &classes,
            const std::vector<std::shared_ptr<EnumNode>> &enums);

  virtual std::string toString() const { return name; }

  bool isCustomType() const { return true; }
  std::string toPyString() const { return toString(); }
  std::string toPyDescription() const {
    return "Stringify" + toString() + "()";
  }

  virtual std::vector<std::string> dependencies() const { return {name}; }

  const std::string name;

  virtual void validate(
      const std::unordered_set<std::string> &class_names,
      const std::unordered_set<std::string> &enum_names) const {
    if (class_names.find(name) == class_names.end() &&
        enum_names.find(name) == enum_names.end()) {
      throw SyntaxError(token, "Unknown type: " + name);
    }
  }
};

class TypeTypeList final : public TypeType {
 public:
  TypeTypeList(const Tokenizer::Token &token, std::shared_ptr<TypeType> type)
      : TypeType(token), type(type) {}
  void link(const std::vector<std::shared_ptr<ClassNode>> &classes,
            const std::vector<std::shared_ptr<EnumNode>> &enums) {
    type->link(classes, enums);
  }

  virtual std::string toString() const {
    return "List[" + type->toString() + "]";
  }
  void validate(const std::unordered_set<std::string> &class_names,
                const std::unordered_set<std::string> &enum_names) const {
    type->validate(class_names, enum_names);
  }
  std::string toPyString() const {
    return "typing.List[" + type->toPyString() + "]";
  }
  std::string toPyDescription() const {
    return "StringifyList(" + type->toPyDescription() + ")";
  }
  virtual std::vector<std::string> dependencies() const {
    return type->dependencies();
  }

  // shared_ptr to TypeType
  const std::shared_ptr<TypeType> type;
};

class TypeTypeUnion final : public TypeType {
 public:
  TypeTypeUnion(const Tokenizer::Token &token,
                const std::vector<std::shared_ptr<TypeType>> &types)
      : TypeType(token), types(types) {}
  void link(const std::vector<std::shared_ptr<ClassNode>> &classes,
            const std::vector<std::shared_ptr<EnumNode>> &enums) {
    for (auto &type : types) {
      type->link(classes, enums);
    }
  }

  virtual std::string toString() const {
    std::stringstream ss;
    ss << "Union[";
    for (auto &type : types) {
      ss << type->toString() << ", ";
    }
    ss << "]";
    return ss.str();
  }
  std::string toPyString() const {
    std::stringstream ss;
    ss << "typing.Union[";
    for (auto &type : types) {
      ss << type->toPyString() << ", ";
    }
    ss << "]";
    return ss.str();
  }

  virtual void validate(
      const std::unordered_set<std::string> &class_names,
      const std::unordered_set<std::string> &enum_names) const {
    for (auto &type : types) {
      type->validate(class_names, enum_names);
    }
  }

  std::string toPyDescription() const {
    std::string desc = "StringifyUnion[" + toPyString() + "](";
    for (auto &type : types) {
      desc += type->toPyDescription() + ", ";
    }
    desc += ")";
    return desc;
  }

  virtual std::vector<std::string> dependencies() const {
    std::vector<std::string> deps;
    for (auto &type : types) {
      auto type_deps = type->dependencies();
      deps.insert(deps.end(), type_deps.begin(), type_deps.end());
    }
    return deps;
  }

  // shared_ptr to TypeType
  const std::vector<std::shared_ptr<TypeType>> types;
};

std::shared_ptr<TypeType> TypeFromString(const Tokenizer::Token &token,
                                         const std::string &str);

class TypeNode : public AstNode {
 public:
  TypeNode(const Tokenizer::Token token, const std::string &type)
      : AstNode(token), type(TypeFromString(token, type)) {
    if (!this->type) {
      throw SyntaxError(token, "Unexpected type: " + type);
    }
  }
  void link(const std::vector<std::shared_ptr<ClassNode>> &classes,
            const std::vector<std::shared_ptr<EnumNode>> &enums) {
    type->link(classes, enums);
  }
  virtual std::string pythonType() const { return type->toPyString(); }
  std::string pythonDescription() const { return type->toPyDescription(); }

  std::vector<std::string> dependencies() const { return type->dependencies(); }

  const std::shared_ptr<TypeType> type;

  std::string toString() const { return "Type[" + type->toString() + "]"; }

  void validate(const std::unordered_set<std::string> &class_names,
                const std::unordered_set<std::string> &enum_names) const;

  static std::shared_ptr<TypeNode> Parser(
      std::vector<Tokenizer::Token>::const_iterator &it);
};
}  // namespace gloo::AST
