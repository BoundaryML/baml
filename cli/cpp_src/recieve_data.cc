#include <cstring>
#include <fstream>
#include <iostream>
#include <map>
#include <string>

#include "variant/ast/ast.h"
#include "variant/ast/utils.h"
#include "variant/error.h"
#include "variant/generate/dir_writer.h"
#include "variant/post_process/dependency_graph.h"
#include "variant/post_process/validate.h"
#include "variant/tokenizer/tokenizer.h"

void generate(const std::string &out_dir,
              const std::map<std::string, std::string> &file_map) {
  std::unordered_map<std::string, gloo::AST::Nodes> file_nodes;
  for (const auto &pair : file_map) {
    auto tokens = gloo::Tokenizer::Tokenize(pair.first, pair.second);
    auto nodes = gloo::AST::Parser(tokens);
    file_nodes[pair.first] = nodes;
  }

  // Combine all the nodes into one AST
  gloo::AST::Nodes nodes;
  for (auto pair : file_nodes) {
    for (auto &item : pair.second.enums) {
      nodes.enums.push_back(item);
    }

    for (auto &item : pair.second.classes) {
      nodes.classes.push_back(item);
    }

    for (auto &item : pair.second.functions) {
      nodes.functions.push_back(item);
    }

    for (auto &[func, group] : pair.second.function_test_groups) {
      nodes.function_test_groups[func].insert(
          nodes.function_test_groups[func].end(), group.begin(), group.end());
    }

    for (auto &[func, variants] : pair.second.function_variants) {
      nodes.function_variants[func].insert(nodes.function_variants[func].end(),
                                           variants.begin(), variants.end());
    }
    for (auto &item : pair.second.clients) {
      nodes.clients.push_back(item);
    }
  }

  gloo::PostProcess::Validate(nodes);

  auto [order, deps] = gloo::PostProcess::BuildDependencyGraph(nodes);

  // Print the nodes in the order they should be processed
  for (const auto &node : order) {
    node->toPython(deps.at(node->uniqueName()));
  }

  // Write the __init__.py files
  gloo::DirectoryWriter::get().file("__init__.py");
  gloo::DirectoryWriter::get().flush(out_dir);
}

extern "C" {
int receive_data(const char *out_dir, const char **filenames,
                 const char **contents, int len, char *error_msg) {
  std::map<std::string, std::string> file_map;
  for (int i = 0; i < len; i++) {
    file_map[filenames[i]] = contents[i];
  }

  try {
    generate(out_dir, file_map);
    return 0;
  } catch (const gloo::GlooError &e) {
    if (error_msg) {
// Copy the exception's error message to the provided buffer
#ifdef _WIN32
      strncpy_s(error_msg, 255, e.what().data(), 255);
#else
      strncpy(error_msg, e.what().data(), 255);
#endif
      error_msg[255] = '\0';  // Null-terminate just to be sure
    }
    return 1;  // Error
  } catch (const std::exception &e) {
    if (error_msg) {
      // Copy the exception's error message to the provided buffer
#ifdef _WIN32
      strncpy_s(error_msg, 255, e.what(), 255);
#else
      strncpy(error_msg, e.what(), 255);
#endif
      error_msg[255] = '\0';  // Null-terminate just to be sure
    }
    return 2;  // Error
  } catch (...) {
    if (error_msg) {
      // Copy the exception's error message to the provided buffer
#ifdef _WIN32
      strncpy_s(error_msg, 255, "Unknown error", sizeof("Unknown error"));
#else
      strncpy(error_msg, "Unknown error", 255);
#endif
      error_msg[255] = '\0';  // Null-terminate just to be sure
    }
    return 3;  // Error
  }
}
}
