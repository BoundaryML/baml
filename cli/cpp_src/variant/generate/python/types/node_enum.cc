#include "variant/ast/types/node_enum.h"

#include "variant/generate/dir_writer.h"
#include "variant/generate/python/utils.h"

namespace gloo::AST {

const std::string enum_template = R"(
import typing
from enum import Enum
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

class {@@name}(str, Enum):
    {@@values}

class Stringify{@@name}(StringifyEnum[{@@name}]):
    def __init__(self, **update_kwargs: StringifyRemappedField) -> None:
        values = {
            v: EnumFieldDescription(name=v.value, description=None, skip=False)
            for v in {@@name}
        }
        super().__init__(values=values, updates=update_kwargs)
)";

IMPL_PYTHONIC(EnumNode) {
  (void)deps;  // UNUSED
  auto file = DirectoryWriter::get().file("custom_types/" + name + "_.py");
  file->add_template_var("name", name);

  std::string values_str = "";
  for (const auto &value : values) {
    values_str += value + " = \"" + value + "\"\n";
  }
  file->add_template_var("values", Python::indent(values_str, 1));
  (*file->stream()) << enum_template;

  DirectoryWriter::get()
      .file(std::filesystem::path("custom_types") / std::string("__init__.py"))
      ->add_import("." + name + "_", name, /*export=*/true);
  gloo::DirectoryWriter::get()
      .file("custom_types/stringify.py")
      ->add_import("." + name + "_", "Stringify" + name, /*export=*/true);
}

}  // namespace gloo::AST
