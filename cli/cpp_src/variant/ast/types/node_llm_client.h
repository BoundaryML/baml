#pragma once
#include <ostream>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <vector>

#include "variant/ast/node.h"
#include "variant/tokenizer/tokenizer.h"

namespace gloo::AST {
class LLMClientNode : public OutputNode {
 public:
  LLMClientNode(const Tokenizer::Token token, const std::string &name,
                const std::string &provider,
                const std::unordered_map<std::string, std::string> &args,
                const int num_retries,
                const std::optional<std::string> &default_fallback_client,
                const std::unordered_map<int, std::string> &fallback_clients)
      : OutputNode(token, name),
        provider(provider),
        args(args),
        num_retries(num_retries),
        default_fallback_client(default_fallback_client),
        fallback_clients(fallback_clients) {}
  NodeOrder order() const { return NodeOrder::LLM_CLIENT; }

  std::vector<std::string> dependencies() const {
    std::unordered_set<std::string> deps;
    for (const auto &[key, val] : fallback_clients) {
      deps.insert(val);
    }
    if (default_fallback_client.has_value()) {
      deps.insert(default_fallback_client.value());
    }

    return std::vector<std::string>(deps.begin(), deps.end());
  }

  const std::string provider;
  const std::unordered_map<std::string, std::string> args;

  const int num_retries;
  const std::optional<std::string> default_fallback_client;
  const std::unordered_map<int, std::string> fallback_clients;

  std::string toString() const;
  PYTHONIC();

  void validate(const std::unordered_set<std::string> &llm_clients) const;

  static std::shared_ptr<LLMClientNode> Parser(
      std::vector<Tokenizer::Token>::const_iterator &it);
};

}  // namespace gloo::AST