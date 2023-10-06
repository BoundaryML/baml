#include "variant/post_process/validate.h"

#include <string>
#include <unordered_set>

#include "variant/common.h"
#include "variant/error.h"

namespace gloo::PostProcess {
void Validate(const AST::Nodes &nodes) {
  // First pass get all names of enums, classes, tasks, runners, and pipelines
  std::unordered_set<std::string> all_names;
  std::unordered_set<std::string> enum_names;
  std::unordered_set<std::string> class_names;
  std::unordered_set<std::string> function_names;
  std::unordered_set<std::string> client_names;

  auto checkDupes = [&](std::unordered_set<std::string> &target,
                        const std::shared_ptr<AST::OutputNode> &node,
                        const std::string &name) {
    validateIdentifier(node->token, name);

    if (all_names.find(name) != all_names.end()) {
      throw DuplicateError({node->token}, name + " in " + node->toString());
    }
    all_names.insert(name);
    target.insert(name);
  };

  for (const auto &node : nodes.enums) {
    checkDupes(enum_names, node, node->name);
  }
  for (const auto &node : nodes.classes) {
    checkDupes(class_names, node, node->name);
  }
  for (const auto &node : nodes.functions) {
    checkDupes(function_names, node, node->name);
  }
  for (const auto &node : nodes.clients) {
    checkDupes(client_names, node, node->name);
  }
  for (const auto &function_variant : nodes.function_variants) {
    std::string func = function_variant.first;
    auto variants = function_variant.second;
    // Find the function node in nodes.functions by func
    auto it = std::find_if(nodes.functions.begin(), nodes.functions.end(),
                           [&](const std::shared_ptr<AST::FunctionNode> &node) {
                             return node->name == func;
                           });

    std::unordered_set<std::string> names;
    for (const auto &variant : variants) {
      if (it == nodes.functions.end()) {
        throw SyntaxError(variant->token, "Function not found: " + func);
      }
      validateIdentifier(variant->token, variant->name);
      if (names.find(variant->name) != names.end()) {
        throw DuplicateError({variant->token},
                             variant->name + "\n" + variant->toString());
      }
      names.insert(variant->name);
      (*it)->addVariant(variant);
    }
  }

  for (const auto &function_test_group : nodes.function_test_groups) {
    std::string func = function_test_group.first;
    // Find the function node in nodes.functions by func
    auto it = std::find_if(nodes.functions.begin(), nodes.functions.end(),
                           [&](const std::shared_ptr<AST::FunctionNode> &node) {
                             return node->name == func;
                           });

    std::unordered_set<std::string> names;
    auto test_groups = function_test_group.second;
    for (const auto &test_group : test_groups) {
      if (it == nodes.functions.end()) {
        throw SyntaxError(test_group->token, "Function not found: " + func);
      }
      validateIdentifier(test_group->token, test_group->name);
      if (names.find(test_group->name) != names.end()) {
        throw DuplicateError({test_group->token},
                             test_group->name + "\n" + test_group->toString());
      }
      names.insert(test_group->name);
      test_group->validate(function_names);
      (*it)->addTestGroup(test_group);
    }
  }

  // Now validate all the enums, classes, tasks, runners, and pipelines
  for (const auto &node : nodes.clients) {
    node->validate(client_names);
  }

  for (const auto &node : nodes.enums) {
    node->validate();
  }

  for (const auto &node : nodes.classes) {
    node->validate(class_names, enum_names);
  }

  for (const auto &node : nodes.functions) {
    node->validate(class_names, enum_names);
  }

  for (const auto &[func, variants] : nodes.function_variants) {
    for (const auto &variant : variants) {
      variant->validate(class_names, enum_names, function_names, client_names);
    }
  }

  // Link all types.
  for (const auto &node : nodes.classes) {
    node->link(nodes.classes, nodes.enums);
  }
  for (const auto &node : nodes.functions) {
    node->link(nodes.classes, nodes.enums);
  }
}
};  // namespace gloo::PostProcess
