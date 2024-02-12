from typing import List
from typing_extensions import TypedDict


# Define TypedDict classes


class Meta(TypedDict, total=False):
    description: str


class EnumType(TypedDict):
    name: str
    meta: "Meta"
    values: List["EnumValue"]


class EnumValue(TypedDict):
    name: str
    meta: "Meta"


# Utility functions


def as_comment(text: str) -> str:
    return "\n".join(f"// {line.strip()}" for line in text.strip().split("\n"))


def as_indented_string(content: str, level: int = 1) -> str:
    indentation = "  " * level
    return "\n".join(f"{indentation}{line}" for line in content.strip().split("\n"))


def print_optional(value: str, is_optional: bool) -> str:
    return f"{value} | null" if is_optional else value


# Specialized print functions


def print_enum_value(value: EnumValue) -> str:
    if "description" in value["meta"]:
        return f"{value['name']}: {value['meta']['description']}"
    return value["name"]


def print_enum(enm: EnumType) -> str:
    block = []
    if "description" in enm["meta"]:
        block.append(as_comment(enm["meta"]["description"]))
    block.extend([f"{enm['name']}", "---"])
    for value in enm["values"]:
        block.append(print_enum_value(value))
    return "\n".join(block)
