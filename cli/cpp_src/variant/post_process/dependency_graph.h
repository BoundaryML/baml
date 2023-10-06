#pragma once
#include <string>
#include <unordered_map>
#include <vector>

#include "variant/ast/ast.h"

namespace gloo::PostProcess {
std::pair<std::vector<std::shared_ptr<AST::OutputNode>>,
          std::unordered_map<std::string, std::vector<std::string>>>
BuildDependencyGraph(const AST::Nodes &nodes);
};
