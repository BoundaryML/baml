#include "variant/ast/shared/node_type.h"

#include <string>

#include "variant/ast/types/node_class.h"
#include "variant/ast/types/node_enum.h"
#include "variant/ast/utils.h"
#include "variant/error.h"
#include "variant/tokenizer/tokenizer.h"

namespace gloo::AST {

void TypeTypeRef::link(const std::vector<std::shared_ptr<ClassNode>> &classes,
                       const std::vector<std::shared_ptr<EnumNode>> &enums) {
  for (auto &cn : classes) {
    if (cn->name == name) {
      this->class_node = cn.get();
      return;
    }
  }
  for (auto &en : enums) {
    if (en->name == name) {
      this->enum_node = en.get();
      return;
    }
  }
}

std::shared_ptr<TypeNode> TypeNode::Parser(
    std::vector<Tokenizer::Token>::const_iterator &it) {
  const Tokenizer::Token &start_token = *it;
  // TODO: this should be ParseWord not ParseString.
  const std::string typeString = ParseString(it);
  return std::shared_ptr<TypeNode>(new TypeNode(start_token, typeString));
}

void TypeNode::validate(
    const std::unordered_set<std::string> &class_names,
    const std::unordered_set<std::string> &enum_names) const {
  if (!type) {
    throw SyntaxError(token, "Type not found");
  }
  type->validate(class_names, enum_names);
}

std::shared_ptr<TypeType> TypeFromStringImpl(const Tokenizer::Token &token,
                                             const std::string &str, int &pos);

std::shared_ptr<TypeType> ParseListType(const Tokenizer::Token &token,
                                        const std::string &str, int &pos) {
  if (pos - 1 > 0 && str[pos] == ']' && str[pos - 1] == '[') {
    pos -= 2;
    auto type = TypeFromStringImpl(token, str, pos);
    return std::shared_ptr<TypeType>(new TypeTypeList(token, type));
  }
  return nullptr;
}

std::shared_ptr<TypeType> ParseOptionalType(const Tokenizer::Token &token,
                                            const std::string &str, int &pos) {
  if (str[pos] == '?') {
    pos--;
    auto type = TypeFromStringImpl(token, str, pos);
    return std::shared_ptr<TypeType>(new TypeTypeOptional(token, type));
  }
  return nullptr;
}

std::shared_ptr<TypeType> ParseUnionType(const Tokenizer::Token &token,
                                         const std::string &str, int &pos) {
  if (str[pos] != '|') return nullptr;

  std::vector<std::shared_ptr<TypeType>> types;

  pos--;
  while (pos >= 0) {
    auto type = TypeFromStringImpl(token, str, pos);
    if (!type) {
      return nullptr;
    }

    types.push_back(type);

    if (pos > 0 && str[pos] == '|') {
      pos--;
    } else {
      break;
    }
  }

  if (types.empty()) return nullptr;
  return std::shared_ptr<TypeType>(new TypeTypeUnion(token, types));
}

std::shared_ptr<TypeType> ParseBaseType(const Tokenizer::Token &token,
                                        const std::string &str, int &pos) {
  int start = pos;
  while (pos >= 0 && (isalpha(str[pos]) || isdigit(str[pos]))) {
    pos--;
  }

  if (start == pos) return nullptr;

  std::string baseType = str.substr(pos + 1, start - pos);

  if (baseType == "int") {
    return std::shared_ptr<TypeType>(new TypeTypePrimitive(token, INT));
  } else if (baseType == "float") {
    return std::shared_ptr<TypeType>(new TypeTypePrimitive(token, FLOAT));
  } else if (baseType == "bool") {
    return std::shared_ptr<TypeType>(new TypeTypePrimitive(token, BOOL));
  } else if (baseType == "char") {
    return std::shared_ptr<TypeType>(new TypeTypePrimitive(token, CHAR));
  } else if (baseType == "string") {
    return std::shared_ptr<TypeType>(new TypeTypePrimitive(token, STRING));
  } else if (baseType == "null") {
    return std::shared_ptr<TypeType>(new TypeTypePrimitive(token, NONE));
  } else {
    return std::shared_ptr<TypeType>(new TypeTypeRef(token, baseType));
  }
}

std::shared_ptr<TypeType> TypeFromStringImpl(const Tokenizer::Token &token,
                                             const std::string &str, int &pos) {
  auto optionalType = ParseOptionalType(token, str, pos);
  if (optionalType) {
    return optionalType;
  }

  auto listType = ParseListType(token, str, pos);
  if (listType) {
    return listType;
  }

  auto unionType = ParseUnionType(token, str, pos);
  if (unionType) {
    return unionType;
  }

  return ParseBaseType(token, str, pos);
}

std::shared_ptr<TypeType> TypeFromString(const Tokenizer::Token &token,
                                         const std::string &str) {
  int pos = static_cast<int>(str.length()) - 1;
  auto type = TypeFromStringImpl(token, str, pos);
  if (pos != -1) {
    throw SyntaxError(token, "Invalid type: " + str);
  }
  return type;
}
}  // namespace gloo::AST
