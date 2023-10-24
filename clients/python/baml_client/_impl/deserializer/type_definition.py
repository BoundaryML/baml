import typing
from pydantic import BaseModel
from enum import Enum


class ListTypeDefinition(typing.TypedDict):
    type: typing.Literal["List"]
    item: "ITypeDefinition"


class UnionTypeDefinition(typing.TypedDict):
    type: typing.Literal["Union"]
    choices: typing.List["ITypeDefinition"]


class OptionalTypeDefinition(typing.TypedDict):
    type: typing.Literal["Optional"]
    item: "ITypeDefinition"


class PrimitiveTypeDefinition(typing.TypedDict):
    type: typing.Union[
        typing.Type[str], typing.Type[bool], typing.Type[int], typing.Type[float]
    ]


class NamedTypeDefinition(typing.TypedDict):
    type: typing.Literal["Ref"]
    ref: typing.Union[typing.Type[BaseModel], typing.Type[Enum]]


ITypeDefinition = typing.Union[
    ListTypeDefinition,
    UnionTypeDefinition,
    OptionalTypeDefinition,
    PrimitiveTypeDefinition,
    NamedTypeDefinition,
]


def __get_primitive_type(t: type) -> PrimitiveTypeDefinition:
    return {"type": t}


def __get_list_type(sub_type: ITypeDefinition) -> ListTypeDefinition:
    return {"type": "List", "item": sub_type}


def __get_union_type(sub_types: typing.List[ITypeDefinition]) -> UnionTypeDefinition:
    return {"type": "Union", "choices": sub_types}


def __get_optional_type(sub_type: ITypeDefinition) -> OptionalTypeDefinition:
    return {"type": "Optional", "item": sub_type}


def __get_named_type(
    t: typing.Union[typing.Type[BaseModel], typing.Type[Enum]],
) -> NamedTypeDefinition:
    return {"type": "Ref", "ref": t}


def type_to_definition(t: typing.Type[typing.Any]) -> ITypeDefinition:
    if t in [str, bool, int, float]:
        return __get_primitive_type(t)
    if hasattr(t, "__origin__"):
        origin = t.__origin__
        if origin == typing.Union:
            union_args = [type_to_definition(sub_t) for sub_t in t.__args__]
            # Special case for Optional types (Union[X, NoneType])
            if len(union_args) == 2 and None in union_args:
                # union_args.remove(None)
                return __get_optional_type(union_args[0])
            return __get_union_type(union_args)
        if origin == typing.List:
            list_arg = type_to_definition(t.__args__[0])
            return __get_list_type(list_arg)
    # Assuming everything else is a named type
    if issubclass(t, (Enum, BaseModel)):
        return __get_named_type(t)
    raise NotImplementedError(f"Cannot convert {t} to type definition.")
