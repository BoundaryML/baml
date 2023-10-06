#include "variant/ast/functions/node_function.h"
#include "variant/ast/functions/variants/code_variant.h"
#include "variant/ast/functions/variants/llm_variant.h"
#include "variant/ast/functions/variants/variant_base.h"
#include "variant/ast/shared/node_method.h"
#include "variant/ast/shared/node_stringify.h"
#include "variant/ast/utils.h"

namespace gloo::AST {
enum VariantType { LLM, CODE };

std::vector<std::string> LLMVariantNode::dependencies() const {
  return function->dependencies();
}

std::vector<std::string> CodeVariantNode::dependencies() const {
  std::vector<std::string> deps = function->dependencies();
  deps.insert(deps.end(), usedFunction.begin(), usedFunction.end());
  return deps;
}

VariantType getVariantType(const Tokenizer::Token& tk) {
  ensureTokenKind(tk, Tokenizer::TokenKind::VariantKeyword);
  // Find the variant type by looking for the word in the brackets.
  auto bracket_start = tk.value.find('[');
  auto bracket_end = tk.value.find(']');
  if (bracket_start == std::string::npos || bracket_end == std::string::npos) {
    throw std::runtime_error("Invalid variant name: " + tk.value);
  }
  auto variant_type =
      tk.value.substr(bracket_start + 1, bracket_end - bracket_start - 1);
  if (variant_type == "llm") {
    return VariantType::LLM;
  } else if (variant_type == "code") {
    return VariantType::CODE;
  } else {
    throw SyntaxError(tk, "Unknown variant type: " + variant_type);
  }
}

std::vector<std::shared_ptr<VariantBaseNode>> VariantBaseNode::Parser(
    std::vector<Tokenizer::Token>::const_iterator& it) {
  const auto type = getVariantType(*it++);
  const auto& name = ParseName(it);
  auto& tk = *it;
  const auto& forKeyword = ParseName(it);
  if (forKeyword != "for") {
    throw SyntaxError(tk, "Expected 'for' keyword. Got: " + forKeyword);
  }
  const auto& function_name = ParseName(it);
  std::vector<std::shared_ptr<VariantBaseNode>> baseResult;
  switch (type) {
    case VariantType::LLM:
      for (const auto& x : LLMVariantNode::Parser(function_name, name, it)) {
        baseResult.push_back(x);
      }
      break;
    case VariantType::CODE:
      baseResult.push_back(CodeVariantNode::Parser(function_name, name, it));
      break;
    default:
      throw SyntaxError(*it, "Unknown variant type");
  }

  return baseResult;
}

std::vector<std::shared_ptr<LLMVariantNode>> LLMVariantNode::Parser(
    const std::string& functionName, const std::string& variantName,
    std::vector<Tokenizer::Token>::const_iterator& it) {
  const auto& start_token = *it;
  ensureTokenKind(*it++, Tokenizer::TokenKind::LeftCurlyBracket);
  ensureTokenKind(*it++, Tokenizer::TokenKind::AtSymbol);
  ensureTokenKind(*it++, Tokenizer::TokenKind::ClientKeyword);
  std::vector<std::string> client_names = ParseIdentifierList(it);
  std::optional<std::string> prompt;
  std::vector<std::shared_ptr<StringifyNode>> stringify;
  std::vector<std::shared_ptr<MethodNode>> methods;
  while (it->kind == Tokenizer::TokenKind::AtSymbol) {
    ++it;
    switch (it->kind) {
      case Tokenizer::TokenKind::PromptKeyword: {
        ensureTokenKind(*it++, Tokenizer::TokenKind::PromptKeyword);
        prompt = ParseString(it);
        break;
      }
      case Tokenizer::TokenKind::MethodKeyword: {
        methods.push_back(MethodNode::Parser(it));
        break;
      }
      case Tokenizer::TokenKind::StringifyKeyword: {
        stringify.push_back(StringifyNode::Parser(it));
        break;
      }
      default:
        throw SyntaxError(*it, std::string("Unexpected field: ") + it->value);
    }
  }
  ensureTokenKind(*it++, Tokenizer::TokenKind::RightCurlyBracket);
  if (!prompt.has_value()) {
    throw SyntaxError(start_token, "Prompt must be specified");
  }

  if (client_names.size() == 0) {
    throw SyntaxError(start_token, "At least one client must be specified");
  }
  if (client_names.size() == 1) {
    return {std::shared_ptr<LLMVariantNode>(new LLMVariantNode(
        start_token, variantName, functionName, client_names[0], prompt.value(),
        stringify, methods))};
  } else {
    std::vector<std::shared_ptr<LLMVariantNode>> result;
    for (auto& client_name : client_names) {
      result.push_back(std::shared_ptr<LLMVariantNode>(new LLMVariantNode(
          start_token, variantName + "_" + client_name, functionName,
          client_name, prompt.value(), stringify, methods)));
    }
    return result;
  }
}

std::shared_ptr<CodeVariantNode> CodeVariantNode::Parser(
    const std::string& functionName, const std::string& variantName,
    std::vector<Tokenizer::Token>::const_iterator& it) {
  std::vector<std::string> usedFunction;
  std::vector<std::shared_ptr<MethodNode>> methods;
  const auto& start_token = *it;
  ensureTokenKind(*it++, Tokenizer::TokenKind::LeftCurlyBracket);
  while (it->kind == Tokenizer::TokenKind::AtSymbol) {
    ++it;
    switch (it->kind) {
      case Tokenizer::TokenKind::DependsOnKeyword: {
        ensureTokenKind(*it++, Tokenizer::TokenKind::DependsOnKeyword);
        if (usedFunction.size() > 0) {
          throw SyntaxError(*it, "Multiple depends_on statements");
        }
        const auto deps = ParseIdentifierList(it);
        usedFunction.insert(usedFunction.end(), deps.begin(), deps.end());
        break;
      }
      case Tokenizer::TokenKind::MethodKeyword: {
        methods.push_back(MethodNode::Parser(it));
        break;
      }
      default:
        break;
    }
  }
  ensureTokenKind(*it++, Tokenizer::TokenKind::RightCurlyBracket);
  return std::shared_ptr<CodeVariantNode>(new CodeVariantNode(
      start_token, variantName, functionName, usedFunction, methods));
}

void LLMVariantNode::validate(
    const std::unordered_set<std::string>& class_names,
    const std::unordered_set<std::string>& enum_names,
    const std::unordered_set<std::string>&,
    const std::unordered_set<std::string>& client_names) const {
  if (client_names.find(client_name) == client_names.end()) {
    throw SyntaxError(token, "client[llm] not found: " + client_name);
  }

  // Ensure stringify properties are valid
  std::unordered_set<std::string> property_names;
  for (const auto& prop : stringify) {
    if (property_names.find(prop->type_name) != property_names.end()) {
      throw SyntaxError(token,
                        "Duplicate stringified property: " + prop->type_name);
    }
    if (class_names.find(prop->type_name) == class_names.end() &&
        enum_names.find(prop->type_name) == enum_names.end()) {
      throw SyntaxError(token, "Stringified property must be enum or class: " +
                                   prop->type_name);
    }
    property_names.insert(prop->type_name);
  }

  // Ensure methods are valid
  std::unordered_set<std::string> method_names;
  for (const auto& method : methods) {
    if (method_names.find(method->name) != method_names.end()) {
      throw SyntaxError(token, "Duplicate method: " + method->name);
    }
    method_names.insert(method->name);
  }
}

}  // namespace gloo::AST