from typing import List, Optional, Union, TypedDict, Any, Literal

# Define TypedDict classes


class Meta(TypedDict, total=False):
    description: str


class PrimitiveType(TypedDict):
    rtype: Literal["primitive"]
    optional: bool
    value: str


class EnumType(TypedDict):
    rtype: Literal["enum"]
    name: str
    optional: bool


class FieldType(TypedDict):
    name: str
    meta: "Meta"
    type_meta: "DataType"


class ClassType(TypedDict):
    rtype: Literal["class"]
    fields: List["FieldType"]


class ListType(TypedDict):
    rtype: Literal["list"]
    dims: int
    inner: "DataType"


class InlineType(TypedDict):
    rtype: Literal["inline"]
    type_meta: "DataType"


class UnionType(TypedDict):
    rtype: Literal["union"]
    members: List["DataType"]


DataType = Union[PrimitiveType, ClassType, ListType, InlineType, UnionType]

# Utility functions


def as_comment(text: str) -> str:
    return "\n".join(f"// {line.strip()}" for line in text.strip().split("\n"))


def as_indented_string(content: str, level: int = 1) -> str:
    indentation = "  " * level
    return "\n".join(f"{indentation}{line}" for line in content.strip().split("\n"))


def print_optional(value: str, is_optional: bool) -> str:
    return f"{value} | null" if is_optional else value


# Specialized printing functions


def print_primitive(item: PrimitiveType) -> str:
    return print_optional(item["value"], item["optional"])


def print_class(item: ClassType) -> str:
    fields = []
    for field in item["fields"]:
        description = (
            as_comment(field["meta"].get("description", "")) + "\n"
            if "description" in field["meta"]
            else ""
        )
        field_value = print_type(field["type_meta"])
        fields.append(description + f'"{field["name"]}": {field_value}')
    class_content = as_indented_string(",\n".join(fields))
    return print_optional(f"{{\n{class_content}\n}}", item["optional"] or False)


def print_list(item: ListType) -> str:
    return print_type(item["inner"]) + ("[]" * item["dims"])


def print_enum(item: EnumType) -> str:
    return print_optional(f'"{item["name"]} as string"', item["optional"] or False)


def print_union(item: UnionType) -> str:
    member_types = [print_type(member) for member in item["members"]]
    return " | ".join(member_types)


# Main function to print data type
def print_type(item: DataType) -> str:
    if "rtype" not in item:
        raise Exception(f"Invalid type: {item}")
    type_printers = {
        "primitive": print_primitive,
        "class": print_class,
        "list": print_list,
        "enum": print_enum,
        "union": print_union,
    }

    printer = type_printers.get(item["rtype"])
    if printer:
        return printer(item)

    if item["rtype"] == "output":
        return f'{print_type(item["value"])}'

    if item["rtype"] == "inline":
        return print_type(item["value"])

    return f"<BAML_NOT_HANDLED:{item['rtype']}>"
