#pragma once

#include "variant/ast/functions/node_function.h"
#include "variant/ast/types/node_class.h"
#include "variant/ast/types/node_enum.h"
#include "variant/ast/types/node_llm_client.h"
#include "variant/tokenizer/tokenizer.h"

namespace gloo::AST {

struct Nodes {
  std::vector<std::shared_ptr<EnumNode>> enums;
  std::vector<std::shared_ptr<ClassNode>> classes;
  std::vector<std::shared_ptr<FunctionNode>> functions;
  std::vector<std::shared_ptr<LLMClientNode>> clients;
  std::unordered_map<std::string, std::vector<std::shared_ptr<VariantBaseNode>>>
      function_variants;
  std::unordered_map<std::string, std::vector<std::shared_ptr<TestGroupNode>>>
      function_test_groups;
};

Nodes Parser(const std::vector<Tokenizer::Token> &tokens);
}  // namespace gloo::AST