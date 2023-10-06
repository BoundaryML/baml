#include <fstream>
#include <regex>

#include "variant/ast/functions/node_function.h"
#include "variant/ast/functions/tests/test.h"
#include "variant/generate/dir_writer.h"
#include "variant/generate/python/utils.h"

namespace gloo::AST {

const std::string test_group_template = R"(
import typing
import pytest

InputType = typing.TypeVar('InputType', bound={@@input_type})
OutputType = typing.TypeVar('OutputType', bound={@@output_type})

@pytest.mark.gloo_test
@pytest.mark.asyncio
@pytest.mark.parametrize("variant", [{@@variants}])
class Test{@@group_name}:
)";

const std::string test_case_template = R"(
    async def test_{@@case_name__num__}(self, variant: VariantTypes) -> None:
        arg = {@@arg__num__}
        {@@setter__num__}await {@@func_name}(variant, arg)
        {@@group_eval_methods}
        {@@case_eval_methods__num__}
)";

IMPL_PYTHONIC(TestGroupNode) {
  // Ensure the init file exists
  DirectoryWriter::get().file(std::filesystem::path("functions") /
                              (functionName + "_") / std::string("tests") /
                              std::string("__init__.py"));

  auto file = DirectoryWriter::get().file(
      std::filesystem::path("functions") / (functionName + "_") /
      std::string("tests") / std::string("test_" + name + ".py"));

  std::string variants_str = "";
  for (const auto &variant : function->variants) {
    variants_str += "'" + variant->name + "', ";
  }
  if (variants_str.size() > 0)
    variants_str = variants_str.substr(0, variants_str.size() - 2);
  file->add_template_var("variants", variants_str);

  std::string group_eval_methods = "";
  for (const auto &method : methods) {
    group_eval_methods += method->toPyString(true);
  }
  // Indent group_eval_methods
  file->add_template_var("group_eval_methods",
                         Python::indent(group_eval_methods, 2));

  file->add_import("..", functionName);
  file->add_import("..", "VariantTypes");
  file->add_template_var("group_name", name);
  file->add_template_var("func_name", function->name);
  file->add_template_var("input_type", function->input->pythonType());
  file->add_template_var("output_type", function->output->pythonType());

  for (const auto &dep : deps) {
    if (dep != functionName) {
      file->add_import("....custom_types", dep);
    }
  }

  auto stream = file->stream();
  (*stream) << test_group_template;

  int counter = 0;
  for (auto &c : cases) {
    std::string case_eval_methods = "";
    for (const auto &method : c->methods) {
      case_eval_methods += method->toPyString(true);
    }
    std::string counter_str = std::to_string(counter++);
    file->add_template_var("case_name" + counter_str, c->name);
    file->add_template_var("case_eval_methods" + counter_str,
                           Python::indent(case_eval_methods, 2));
    file->add_template_var("arg" + counter_str,
                           Python::AsValue(function->input->type, c->value));
    file->add_template_var(
        "setter" + counter_str,
        (group_eval_methods.empty() && case_eval_methods.empty())
            ? ""
            : "output = ");

    std::string case_string = test_case_template;
    // Replace every instance of __num__ with counter_str
    case_string =
        std::regex_replace(case_string, std::regex("__num__"), counter_str);
    (*stream) << case_string;
  }
}

}  // namespace gloo::AST
