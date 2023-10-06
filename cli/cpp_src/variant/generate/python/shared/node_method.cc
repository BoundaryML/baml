#include "variant/ast/shared/node_method.h"

#include "variant/error.h"

namespace gloo::AST {

std::string MethodNode::toPyString(bool with_usage) const {
  // Find python lang
  auto it = std::find_if(langs.begin(), langs.end(), [](const auto &lang) {
    return lang.language == Language::PYTHON;
  });
  if (it == langs.end()) {
    throw SyntaxError(token, "No python implementation for method " + name);
  }

  std::string result = it->code + "\n";

  if (with_usage) {
    const bool is_async = it->code.starts_with("async");
    if (is_async) {
      result += "await ";
    }
    result += name + "(arg, output)\n";
  }

  return result;
}

}  // namespace gloo::AST
