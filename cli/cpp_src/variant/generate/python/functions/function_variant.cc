#include "variant/ast/functions/node_function.h"
#include "variant/ast/functions/variants/code_variant.h"
#include "variant/ast/functions/variants/llm_variant.h"
#include "variant/ast/types/node_class.h"
#include "variant/generate/dir_writer.h"
#include "variant/generate/python/utils.h"

namespace gloo::AST {

const std::string llm_variant_template = R"(
import typing
import json
from gloo_py import LLMVariant
from gloo_py.stringify import (
    StringifyBase,
    StringifyError,
    StringifyNone,
    StringifyBool,
    StringifyInt,
    StringifyChar,
    StringifyString,
    StringifyFloat,
    StringifyEnum,
    StringifyUnion,
    StringifyOptional,
    StringifyList,
    StringifyClass,
    FieldDescription,
    EnumFieldDescription,
    StringifyRemappedField,
    StringifyCtx
)

prompt = """\
{@@prompt}"""

stringifiers: typing.List[typing.Any] = []
def gen_stringify() -> StringifyBase[{@@output_type}]:
    with StringifyCtx():
        {@@stringify_init}
        return OUTPUT_STRINGIFY

OUTPUT_STRINGIFY = gen_stringify()

{@@methods}

{@@parser_middleware}
{@@custom_vars_middleware}

async def parser(raw_llm_output: str) -> {@@output_type}:
    return OUTPUT_STRINGIFY.parse(parser_middleware(raw_llm_output))

async def prompt_vars(arg: {@@input_type}) -> typing.Dict[str, str]:
    vars = {
        'input': str(arg),
        {@@input_props}
        'output.json': OUTPUT_STRINGIFY.json,
    }
    vars.update(custom_vars())
    for stringify in stringifiers:
        vars.update(**stringify.vars())
    vars.update(**OUTPUT_STRINGIFY.vars())
    return vars

Variant{@@name} = LLMVariant[{@@input_type}, {@@output_type}](
    '{@@func_name}', '{@@name}', prompt=prompt, client={@@client}, parser=parser, prompt_vars=prompt_vars)

async def RunVariant_{@@name}(arg: {@@input_type}) -> {@@output_type}:
    return await Variant{@@name}.run(arg)
)";

IMPL_PYTHONIC(LLMVariantNode) {
  auto file = DirectoryWriter::get().file(
      std::filesystem::path("functions") / (functionName + "_") /
      std::string("variants") / std::string("llm_" + name + ".py"));

  for (const auto &dep : deps) {
    file->add_import("....custom_types", dep);
    file->add_import("....custom_types.stringify", "Stringify" + dep);
  }
  file->add_import("....clients", client_name);
  file->add_template_var("client", client_name);

  std::unordered_map<std::string, std::shared_ptr<StringifyNode>>
      stringify_vars;
  for (const auto &v : stringify) {
    stringify_vars[v->type_name] = v;
  }

  std::string stringify_init_str = "";
  for (const auto &dep : deps) {
    auto res = stringify_vars.find(dep);
    stringify_init_str += "stringify_" + dep + " = Stringify" + dep + "(";
    if (res != stringify_vars.end()) {
      // We have a custom stringify for this type
      stringify_init_str += res->second->pyString();
    }
    stringify_init_str += ")\n";
    stringify_init_str += "stringifiers.append(stringify_" + dep + ")\n";
  }
  if (function->output->type->isCustomType()) {
    stringify_init_str += "OUTPUT_STRINGIFY = stringify_" +
                          function->output->pythonType() + "\n ";
  } else {
    stringify_init_str +=
        "OUTPUT_STRINGIFY = " + function->output->type->toPyDescription() +
        "\n";
    stringify_init_str += "stringifiers.append(OUTPUT_STRINGIFY)\n";
  }

  std::string parser_middleware =
      "def parser_middleware(raw_llm_output: str) -> str:\n    return "
      "raw_llm_output\n";
  std::string custom_vars_middleware =
      "def custom_vars() -> typing.Dict[str, str]:\n    return "
      "{}\n";
  std::string methods_str = "";
  for (const auto &method : methods) {
    methods_str += method->toPyString(false) + "\n";
    if (method->name == "parser_middleware") {
      parser_middleware = "";
    }
    if (method->name == "custom_vars") {
      custom_vars_middleware = "";
    }
  }

  std::vector<std::tuple<std::string, ClassNode *>> input_types;
  const auto input_type_ptr = function->input->type->class_node;
  if (input_type_ptr) {
    input_types.push_back({"", input_type_ptr});
  }

  std::string input_props = "";
  auto add_input_prop = [&](const std::string &prefix, ClassNode *node) {
    for (const auto &prop : node->properties) {
      std::string suffix = prefix + prop.name;
      input_props += "\n'input." + suffix + "': str(arg." + suffix + "),";
      if (prop.type->type->class_node) {
        input_types.push_back({suffix + ".", prop.type->type->class_node});
      }
    }
    for (const auto &prop : node->methods) {
      input_props += "\n'input." + prefix + prop.name + "': str(arg." + prefix +
                     prop.name + "),";
    }
  };

  while (!input_types.empty()) {
    auto [prefix, node] = input_types.back();
    input_types.pop_back();
    add_input_prop(prefix, node);
  }

  // Remove starting whitespace
  if (!input_props.empty()) {
    input_props = input_props.substr(1);
  }
  file->add_template_var("name", name);
  file->add_template_var("func_name", function->name);
  file->add_template_var("input_type", function->input->pythonType());
  file->add_template_var("output_type", function->output->pythonType());
  file->add_template_var("input_props", Python::indent(input_props, 2));
  file->add_template_var("prompt", prompt);
  file->add_template_var("custom_vars_middleware", custom_vars_middleware);
  file->add_template_var("stringify_init",
                         Python::indent(stringify_init_str, 2));
  file->add_template_var("parser_middleware", parser_middleware);
  file->add_template_var("methods", methods_str);

  (*file->stream()) << llm_variant_template;
}

const std::string code_variant_impl_template = R"(
import typing

InputType = {@@input_type}
OutputType = {@@output_type}

async def {@@name}_impl(arg: InputType) -> OutputType:
    # Write your code here
    raise NotImplementedError('Code Variants must be custom implemented: {@@unique_name}')
)";

const std::string code_variant_template = R"(
import typing
from gloo_py import CodeVariant

InputType = {@@input_type}
OutputType = {@@output_type}

{@@method_str}

Variant{@@name} = CodeVariant[InputType, OutputType]('{@@func_name}', '{@@name}', func={@@name}_impl)

async def RunVariant_{@@name}(arg: InputType) -> OutputType:
    return await Variant{@@name}.run(arg)
)";

IMPL_PYTHONIC(CodeVariantNode) {
  auto file = DirectoryWriter::get().file(
      std::filesystem::path("functions") / (functionName + "_") /
      std::string("variants") / std::string("code_" + name + ".py"));

  std::unordered_set<std::string> func_dependencies;
  for (const auto &dep : usedFunction) {
    func_dependencies.insert(dep);
    file->add_import("..." + dep + "_", dep);
  }

  for (const auto &dep : deps) {
    if (func_dependencies.find(dep) != func_dependencies.end()) {
      continue;
    }
    file->add_import("....custom_types", dep);
  }

  std::string method_str = "";
  for (const auto &method : methods) {
    method_str += method->toPyString(false) + "\n";
  }

  auto impl_method =
      std::find_if(methods.begin(), methods.end(),
                   [](const auto &method) { return method->name == "impl"; });
  if (impl_method != methods.end()) {
    method_str += name + "_impl = impl";
  } else {
    file->add_import(".code_" + name + "_impl", name + "_impl");
  }

  file->add_template_var("name", name);
  file->add_template_var("func_name", function->name);
  file->add_template_var("input_type", function->input->pythonType());
  file->add_template_var("output_type", function->output->pythonType());
  file->add_template_var("method_str", method_str);
  file->add_template_var("unique_name", uniqueName());

  (*file->stream()) << code_variant_template;

  if (impl_method == methods.end()) {
    auto impl_file = DirectoryWriter::get().file(
        std::filesystem::path("functions") / (functionName + "_") /
        std::string("variants") / std::string("code_" + name + "_impl.py"));
    impl_file->add_template_var("name", name);
    impl_file->add_template_var("unique_name", uniqueName());
    impl_file->add_template_var("input_type", function->input->pythonType());
    impl_file->add_template_var("output_type", function->output->pythonType());

    for (const auto &dep : usedFunction) {
      impl_file->add_import("..." + dep, dep);
    }

    for (const auto &dep : deps) {
      if (func_dependencies.find(dep) != func_dependencies.end()) {
        continue;
      }
      impl_file->add_import("....custom_types", dep);
    }

    (*impl_file->stream()) << code_variant_impl_template;
  }
}

}  // namespace gloo::AST
