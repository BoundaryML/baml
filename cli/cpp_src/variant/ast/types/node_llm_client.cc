#include "variant/ast/types/node_llm_client.h"

#include <sstream>
#include <unordered_set>

#include "variant/ast/utils.h"
#include "variant/common.h"

namespace gloo::AST {
using namespace Tokenizer;

std::string LLMClientNode::toString() const {
  std::stringstream ss;
  ss << "LLMClient: " << name << ": " << provider << std::endl;
  for (const auto &[key, val] : args) {
    ss << "  " << key << ": " << val << std::endl;
  }
  return ss.str();
}

std::shared_ptr<LLMClientNode> LLMClientNode::Parser(
    std::vector<Tokenizer::Token>::const_iterator &it) {
  const Tokenizer::Token &start_token = *it;
  ensureTokenKind(*it++, TokenKind::ClientKeyword);
  const std::string name = ParseName(it);
  ensureTokenKind(*it++, TokenKind::LeftCurlyBracket);
  ensureTokenKind(*it++, TokenKind::AtSymbol);
  ensureTokenKind(*it++, TokenKind::ProviderKeyword);
  const std::string provider = ParseString(it);

  int num_retries = 0;
  std::optional<std::string> default_fallback_client;
  std::unordered_map<int, std::string> fallback_clients;

  std::unordered_map<std::string, std::string> args;
  while (it->kind != TokenKind::RightCurlyBracket) {
    switch (it->kind) {
      case TokenKind::Identifier: {
        const std::string key = ParseName(it);
        const std::string value = ParseString(it);
        args[key] = value;
        break;
      }
      case TokenKind::AtSymbol: {
        ++it;
        switch (it->kind) {
          case TokenKind::RetryKeyword: {
            ++it;
            num_retries = std::stoi(it->value);
            ++it;
            break;
          }
          case TokenKind::FallbackKeyword: {
            if (it->value == "fallback") {
              ++it;
              default_fallback_client = ParseString(it);
            } else {
              // Find code from string: fallback[code]
              int code =
                  std::stoi(it->value.substr(9, it->value.length() - 10));
              if (fallback_clients.find(code) != fallback_clients.end()) {
                throw SyntaxError(
                    *it, "Duplicate fallback code: " + std::to_string(code));
              }
              ++it;
              fallback_clients[code] = ParseString(it);
            }
            break;
          }
          default:
            throw SyntaxError(*it, "Unexpected token after @: " + it->value);
        }
        break;
      }
      default:
        throw SyntaxError(*it, "Unexpected token in client[llm]: " +
                                   Tokenizer::TokenKindToString(it->kind) +
                                   ": " + it->value);
    }
  }
  ensureTokenKind(*it++, TokenKind::RightCurlyBracket);
  return std::shared_ptr<LLMClientNode>(
      new LLMClientNode(start_token, name, provider, args, num_retries,
                        default_fallback_client, fallback_clients));
}

void LLMClientNode::validate(
    const std::unordered_set<std::string> &llm_clients) const {
  if (args.size() == 0) {
    throw SyntaxError(
        token, "Generally at least the model name is required for client[llm]");
  }

  if (default_fallback_client.has_value()) {
    if (default_fallback_client.value() == name) {
      throw SyntaxError(token, "Cannot fallback to self");
    }
    if (llm_clients.find(default_fallback_client.value()) ==
        llm_clients.end()) {
      throw SyntaxError(token, "Fallback client not found: " +
                                   default_fallback_client.value());
    }
  }

  for (const auto &[code, client] : fallback_clients) {
    if (client == name) {
      throw SyntaxError(token, "Cannot fallback to self");
    }
    if (llm_clients.find(client) == llm_clients.end()) {
      throw SyntaxError(token, "Fallback client not found: " + client);
    }
  }
}
}  // namespace gloo::AST
