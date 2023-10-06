#pragma once

#include <regex>
#include <string>

#include "variant/ast/shared/node_type.h"

namespace gloo::Python {

const char pairs[][2] = {
    {'(', ')'}, {'[', ']'}, {'{', '}'}, {'"', '"'}, {'\'', '\''}};

inline bool isConstructor(const std::string& value) {
  // Check if the string has matching pairs of (), [], {}
  std::vector<char> stack;
  char in_string = '\0';
  for (const auto& c : value) {
    if (in_string != '\0') {
      if (c == in_string) {
        in_string = '\0';
        stack.pop_back();
      }
      continue;
    }
    for (const auto& [open, close] : pairs) {
      if (c == '"' || c == '\'') {
        in_string = c;
        stack.push_back(c);
        break;
      }
      if (c == open) {
        stack.push_back(open);
      } else if (c == close) {
        if (stack.empty() || stack.back() != open) {
          return false;
        }
        stack.pop_back();
      }
    }
  }

  bool is_constructor = stack.empty();

  // Check if the string is of the form CLASS_NAME(...)
  if (is_constructor) {
    std::regex re("^[a-zA-Z_]\\w*\\([\\s\\S]*\\)\\s*$");
    is_constructor = std::regex_match(value, re);
  }

  return is_constructor;
}

inline std::string AsValue(const std::string& value) {
  if (value.find("Conversation(thread=") != std::string::npos) {
  }
  // if the string is a number, return it as is
  if (std::regex_match(value, std::regex("[-+]?[0-9]*\\.?[0-9]+"))) {
    return value;
  }
  if (value == "true" || value == "True") {
    return "True";
  }
  if (value == "false" || value == "False") {
    return "False";
  }

  for (const auto& [open, close] : pairs) {
    if (value[0] == open && value[value.size() - 1] == close) {
      return value;
    }
  }
  // if string is an f-string, return it as is
  if (value[0] == 'f' && value[1] == value[value.size() - 1] &&
      (value[1] == '"' || value[1] == '\'')) {
    return value;
  }
  // Special value of the form @ENV.VAR_NAME should be returned
  // without the @
  if (std::regex_match(value, std::regex("@ENV\\.[a-zA-Z_]\\w*"))) {
    return value.substr(1);
  }
  // Special case for None
  if (value == "None") {
    return value;
  }
  // Special case for empty string
  if (value == "") {
    return "''";
  }
  // Special case for Python constructors
  if (isConstructor(value)) {
    return value;
  }

  return "'''" + value + "'''";
}

inline std::string AsValue(const std::shared_ptr<gloo::AST::TypeType>& node,
                           const std::string& value) {
  const auto type = node->toString();
  if (type == "bool") {
    if (value == "true" || value == "True") {
      return "True";
    }
    if (value == "false" || value == "False") {
      return "False";
    }
    throw SyntaxError(node->token, "Invalid boolean value: " + value);
  }

  if (type == "int" || type == "float") {
    // if the string is a number, return it as is
    if (std::regex_match(value, std::regex("[-+]?[0-9]*\\.?[0-9]+"))) {
      return value;
    }
    throw SyntaxError(node->token, "Invalid number value: " + value);
  }

  if (!(node->toString() == "string" || node->toString() == "char")) {
    return value;
  }

  if (value[0] == '"' && value[value.size() - 1] == '"') {
    return value;
  }

  if (value[0] == '\'' && value[value.size() - 1] == '\'') {
    return value;
  }

  // if string is an f-string, return it as is
  if (value[0] == 'f' && value[1] == value[value.size() - 1] &&
      (value[1] == '"' || value[1] == '\'')) {
    return value;
  }
  // Special value of the form @ENV.VAR_NAME should be returned
  // without the @
  if (std::regex_match(value, std::regex("@ENV\\.[a-zA-Z_]\\w*"))) {
    return value.substr(1);
  }
  // Special case for empty string
  if (value == "") {
    return "''";
  }

  return "'''" + value + "'''";
}

inline std::string indent(const std::string& value, int level) {
  std::string output = "";
  for (int i = 0; i < level; i++) {
    output += "    ";
  }
  // deal with the case where the value is a multiline string by replacing
  // newlines with newlines + indent
  std::string indent_str = "\n" + output;
  std::string indent_value =
      std::regex_replace(value, std::regex("\n"), indent_str);
  // Remove leading and trailing newlines
  indent_value = std::regex_replace(indent_value, std::regex("^\\n+"), "");
  indent_value = std::regex_replace(indent_value, std::regex("\\n+$"), "");
  return indent_value;
}

}  // namespace gloo::Python