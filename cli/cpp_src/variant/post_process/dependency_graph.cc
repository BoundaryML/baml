#include "variant/post_process/dependency_graph.h"

#include <string>
#include <unordered_map>

#include "variant/common.h"
#include "variant/error.h"

namespace gloo::PostProcess {

std::pair<std::vector<std::shared_ptr<AST::OutputNode>>,
          std::unordered_map<std::string, std::vector<std::string>>>
BuildDependencyGraph(const AST::Nodes &nodes) {
  std::unordered_map<std::string, std::pair<std::shared_ptr<AST::OutputNode>,
                                            std::vector<std::string>>>
      deps;

  // Now validate all the enums, classes, tasks, runners, and pipelines
  for (const auto &node : nodes.enums) {
    deps[node->uniqueName()] = {node, {}};
  }

  for (const auto &node : nodes.clients) {
    deps[node->uniqueName()] = {node, node->dependencies()};
  }

  for (const auto &node : nodes.classes) {
    deps[node->uniqueName()] = {node, node->dependencies()};
  }

  for (const auto &node : nodes.functions) {
    auto func_deps = node->dependencies();

    deps[node->uniqueName()] = {node, func_deps};
  }

  for (const auto &[func, variants] : nodes.function_variants) {
    for (const auto &variant : variants) {
      auto func_deps = variant->dependencies();

      deps[variant->uniqueName()] = {variant, func_deps};
    }
  }

  for (const auto &[func, test_groups] : nodes.function_test_groups) {
    for (const auto &group : test_groups) {
      deps[group->uniqueName()] = {group, {func}};
    }
  }

  std::unordered_map<std::string, std::vector<std::string>> name_to_dep;
  for (const auto &it : deps) {
    name_to_dep[it.first] = it.second.second;
  }

  // Generate the order in which to process the nodes using a topological sort
  std::vector<std::shared_ptr<AST::OutputNode>> order;
  std::unordered_map<std::string, int> order_map;

  std::vector<std::string> next_in_line;
  for (const auto &dep : deps) {
    if (dep.second.second.size() == 0) {
      next_in_line.push_back(dep.first);
    }
  }

  int max_loops = 0;
  while (next_in_line.size() > 0 && max_loops++ < 1000) {
    // Remove the nodes that are next in line
    for (const auto &name : next_in_line) {
      order.push_back(deps[name].first);
      order_map[name] = max_loops;
      deps.erase(name);
    }

    // Remove them from the dependencies of other nodes
    for (const auto &name : next_in_line) {
      for (auto &dep : deps) {
        auto &dependencies = dep.second.second;
        dependencies.erase(
            std::remove(dependencies.begin(), dependencies.end(), name),
            dependencies.end());
      }
    }

    next_in_line.clear();
    for (const auto &dep : deps) {
      if (dep.second.second.size() == 0) {
        next_in_line.push_back(dep.first);
      }
    }
  }

  if (deps.size() > 0) {
    std::string error = "";
    for (const auto &dep : deps) {
      error += dep.first + " ";
    }
    throw CircularDependencyError(deps.begin()->second.first->token, error);
  }

  std::sort(order.begin(), order.end(),
            [&](const std::shared_ptr<AST::OutputNode> &a,
                const std::shared_ptr<AST::OutputNode> &b) {
              if (order_map[a->name] == order_map[b->name]) {
                if (a->order() == b->order()) {
                  return a->token.line < b->token.line;
                }
                return a->order() < b->order();
              }
              return order_map[a->name] < order_map[b->name];
            });

  // Now that we have the order, for each dependency, recurively add the
  // dependencies of the dependency. This will ensure that the dependencies are
  // added in the correct order.

  for (const auto &it : order) {
    std::vector<std::string> name_deps = name_to_dep[it->uniqueName()];
    // update the dependencies to be the dependencies of the dependencies
    std::unordered_set<std::string> new_deps(name_deps.begin(),
                                             name_deps.end());
    for (const auto &dep : name_deps) {
      auto dep_deps = name_to_dep[dep];
      new_deps.insert(dep_deps.begin(), dep_deps.end());
    }
    name_to_dep[it->uniqueName()] =
        std::vector<std::string>(new_deps.begin(), new_deps.end());
  }

  return std::make_pair(order, name_to_dep);
}
};  // namespace gloo::PostProcess
