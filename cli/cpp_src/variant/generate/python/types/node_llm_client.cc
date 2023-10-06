#include "variant/ast/types/node_llm_client.h"

#include "variant/generate/dir_writer.h"
#include "variant/generate/python/utils.h"

namespace gloo::AST {

const std::string llm_client_template = R"(
from gloo_py import llm_client_factory, ENV

{@@name} = llm_client_factory(provider='{@@provider}', {@@params})
)";

IMPL_PYTHONIC(LLMClientNode) {
  (void)deps;  // UNUSED, allow for -Wunused-value
  auto file = DirectoryWriter::get().file("clients/llm_" + name + ".py");

  file->add_template_var("name", name);
  file->add_template_var("provider", provider);

  std::string params = "";
  for (const auto& [key, v] : args) {
    params += key + "=" + Python::AsValue(v) + ", ";
  }
  if (num_retries > 0) {
    params += "__retries__=" + std::to_string(num_retries) + ", ";
  }
  if (default_fallback_client.has_value()) {
    file->add_import(".llm_" + default_fallback_client.value(),
                     default_fallback_client.value());
    params += "__default_fallback__=" + default_fallback_client.value() + ", ";
  }
  if (fallback_clients.size() > 0) {
    params += "__fallback__={";
    for (const auto& [code, client] : fallback_clients) {
      file->add_import(".llm_" + client, client);
      params += std::to_string(code) + ": " + client + ", ";
    }
    params = params.substr(0, params.size() - 2) + "}, ";
  }

  params = params.substr(0, params.size() - 2);

  file->add_template_var("params", params);

  (*file->stream()) << llm_client_template;

  DirectoryWriter::get()
      .file(std::filesystem::path("clients") / std::string("__init__.py"))
      ->add_import(".llm_" + name, name, /*export=*/true);
}

}  // namespace gloo::AST
