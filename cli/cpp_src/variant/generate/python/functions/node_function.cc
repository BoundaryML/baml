
#include "variant/ast/functions/node_function.h"

#include <regex>

#include "variant/generate/dir_writer.h"

namespace gloo::AST {

std::string toNameList(
    const std::vector<std::shared_ptr<VariantBaseNode>> &variants) {
  if (variants.empty()) {
    return "typing.Never";
  }
  std::string res = "typing.Literal[";
  for (const auto &variant : variants) {
    res += "'" + variant->name + "', ";
  }
  res = res.substr(0, res.size() - 2);
  return res + "]";
}

const std::string function_variant_template = R"(
    if variant == '{variant_name}':
        return await RunVariant_{variant_name}(args)
)";

const std::string function_template = R"(
import typing

VariantTypes = {@@variant_types}

async def {@@name}(variant: VariantTypes, args: {@@input_type}) -> {@@output_type}:
{@@variant_impls}
    raise NotImplementedError(f'Variant not found: {@@name}::{variant}')

__all__ = ["{@@name}", "VariantTypes"]
)";

IMPL_PYTHONIC(FunctionNode) {
  (void)deps;  // UNUSED
  auto file =
      DirectoryWriter::get().file(std::filesystem::path("functions") /
                                  (name + "_") / std::string("__init__.py"));

  for (const auto &dep : dependencies()) {
    file->add_import("...custom_types", dep);
  }
  std::string variant_imports_str = "";
  for (const auto &variant : variants) {
    file->add_import(".variants", "RunVariant_" + variant->name);
  }
  const std::string variant_types_str = toNameList(variants);

  std::string variant_impls_str = "";
  for (const auto &variant : variants) {
    variant_impls_str +=
        std::regex_replace(function_variant_template,
                           std::regex("\\{variant_name\\}"), variant->name);
  }

  file->add_template_var("name", name);
  file->add_template_var("input_type", input->pythonType());
  file->add_template_var("output_type", output->pythonType());
  file->add_template_var("variant_imports", variant_imports_str);
  file->add_template_var("variant_types", variant_types_str);
  file->add_template_var("variant_impls", variant_impls_str);
  (*file->stream()) << function_template;
  DirectoryWriter::get()
      .file(std::filesystem::path("functions") / std::string("__init__.py"))
      ->add_import("." + name + "_", name, /*export=*/true);

  // Write all variant imports in functions/{func}/variants/__init__.py
  auto variants_file = DirectoryWriter::get().file(
      std::filesystem::path("functions") / (name + "_") /
      std::string("variants/__init__.py"));
  for (const auto &variant : variants) {
    variants_file->add_import("." + variant->type() + "_" + variant->name,
                              "RunVariant_" + variant->name, /*export=*/true);
  }
}

}  // namespace gloo::AST
