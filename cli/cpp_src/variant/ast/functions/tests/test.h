#pragma once
#include <ostream>
#include <string>
#include <vector>
#include <unordered_map>

#include "variant/ast/node.h"
#include "variant/ast/shared/node_method.h"
#include "variant/error.h"
#include "variant/tokenizer/tokenizer.h"

namespace gloo::AST {

class TestCaseNode : public AstNode {
 public:
  TestCaseNode(const Tokenizer::Token &token, const std::string &name,
               const std::string &value,
               const std::vector<std::shared_ptr<MethodNode>> &methods)
      : AstNode(token), name(name), value(value), methods(methods) {}

  const std::string name;
  const std::string value;
  const std::vector<std::shared_ptr<MethodNode>> methods;

  std::string toString() const {
    std::string result = "TestCase[" + name + "]";
    return result;
  }

  static std::shared_ptr<TestCaseNode> Parser(
      size_t index, std::vector<Tokenizer::Token>::const_iterator &it);
};

class FunctionNode;

class TestGroupNode : public OutputNode {
 public:
  TestGroupNode(const Tokenizer::Token &token, const std::string &name,
                const std::string &functionName,
                const std::vector<std::shared_ptr<TestCaseNode>> &cases,
                const std::vector<std::shared_ptr<MethodNode>> &methods)
      : OutputNode(token, name),
        name(name),
        functionName(functionName),
        cases(cases),
        methods(methods) {}

  virtual NodeOrder order() const { return NodeOrder::TEST_GROUP; }
  PYTHONIC();

  const std::string name;
  const std::string functionName;
  const std::vector<std::shared_ptr<TestCaseNode>> cases;
  const std::vector<std::shared_ptr<MethodNode>> methods;

  std::string toString() const {
    std::string result = functionName + "::test_group[" + name + "]";
    return result;
  }

  static std::shared_ptr<TestGroupNode> Parser(
      std::vector<Tokenizer::Token>::const_iterator &it);

  FunctionNode *function = nullptr;

  void validate(const std::unordered_set<std::string> &function_names) {
    if (function_names.find(functionName) == function_names.end()) {
      throw SyntaxError(token, "Function not found: " + functionName);
    }
    std::unordered_map<std::string, std::vector<Tokenizer::Token>> names;
    for (auto &test_case : cases) {
      names[test_case->name].push_back(test_case->token);
    }

    for (auto &pair : names) {
      if (pair.second.size() > 1) {
        throw DuplicateError(pair.second,
                             name + ": Duplicate test case: " + pair.first);
      }
    }
  }
};

}  // namespace gloo::AST
