#include "variant/ast/types/node_class.h"

#include <regex>
#include <unordered_map>

#include "variant/generate/dir_writer.h"
#include "variant/generate/python/utils.h"

namespace gloo::AST {

const std::string class_template = R"(
import typing
from pydantic import BaseModel
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

class {@@name}(BaseModel):
    {@@properties}

class Stringify{@@name}(StringifyClass[{@@name}]):
    def __init__(self, **update_kwargs: StringifyRemappedField) -> None:
        values: typing.Dict[str, FieldDescription[typing.Any]] = {{@@property_descriptions}}
        super().__init__(model={@@name}, values=values, updates=update_kwargs)
)";

const std::string property_template =
    R"("{name}": FieldDescription(name="{name}", description=None, type_desc={type_desc}),)";

std::string PropertyNode::pythonDescription() const {
  std::unordered_map<std::string, std::string> replacements = {
      {"name", name},
      {"type_desc", type->pythonDescription()},
  };
  std::string output = property_template;
  for (const auto &[key, value] : replacements) {
    output = std::regex_replace(output, std::regex("\\{" + key + "\\}"), value);
  }
  return output;
}

void ClassNode::toPython(const std::vector<std::string> &deps) const {
  auto file = DirectoryWriter::get().file("custom_types/" + name + "_.py");

  for (const auto &dep : deps) {
    file->add_import("." + dep + "_", dep);
    file->add_import("." + dep + "_", "Stringify" + dep);
  }
  std::string properties_str = "";
  std::string property_descriptions = "";
  for (const auto &field : this->properties) {
    properties_str += field.name + ": " + field.type->pythonType() +
                      field.type->type->defaultValue() + "\n";
    property_descriptions += field.pythonDescription();
  }
  for (const auto &method : this->methods) {
    properties_str += method.toPyString(false) + "\n";
  }

  file->add_template_var("name", name);
  file->add_template_var("properties", Python::indent(properties_str, 1));
  file->add_template_var("property_descriptions", property_descriptions);
  (*file->stream()) << class_template;

  DirectoryWriter::get()
      .file(std::filesystem::path("custom_types") / std::string("__init__.py"))
      ->add_import("." + name + "_", name, /*export=*/true);

  gloo::DirectoryWriter::get()
      .file("custom_types/stringify.py")
      ->add_import("." + name + "_", "Stringify" + name, /*export=*/true);
}

}  // namespace gloo::AST
