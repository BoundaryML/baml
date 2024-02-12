import typing
import typing_extensions
from pydantic import BaseModel
from enum import Enum


class ListTypeDefinition(typing_extensions.TypedDict):
    type: typing.Literal["List"]
    item: "ITypeDefinition"


class UnionTypeDefinition(typing_extensions.TypedDict):
    type: typing.Literal["Union"]
    choices: typing.List["ITypeDefinition"]


class OptionalTypeDefinition(typing_extensions.TypedDict):
    type: typing.Literal["Optional"]
    item: "ITypeDefinition"


class PrimitiveTypeDefinition(typing_extensions.TypedDict):
    type: typing.Union[
        typing.Type[str], typing.Type[bool], typing.Type[int], typing.Type[float]
    ]


class NamedTypeDefinition(typing_extensions.TypedDict):
    type: typing.Literal["Ref"]
    ref: typing.Union[typing.Type[BaseModel], typing.Type[Enum]]


class NoneTypeDefinition(typing_extensions.TypedDict):
    type: typing.Literal["None"]


ITypeDefinition = typing.Union[
    ListTypeDefinition,
    UnionTypeDefinition,
    OptionalTypeDefinition,
    PrimitiveTypeDefinition,
    NamedTypeDefinition,
    NoneTypeDefinition,
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
            # Special case for Optional types (Union[X, NoneType])
            if len(t.__args__) == 2 and type(None) in t.__args__:
                # union_args.remove(None)
                return __get_optional_type(type_to_definition(t.__args__[0]))
            else:
                union_args = [type_to_definition(sub_t) for sub_t in t.__args__]
            return __get_union_type(union_args)
        if origin == list:
            list_arg = type_to_definition(t.__args__[0])
            return __get_list_type(list_arg)
    elif type(t) == type(BaseModel) or type(t) == type(Enum):
        # Assuming everything else is a named type
        return __get_named_type(t)

    # Print all attributes of t
    raise NotImplementedError(f"Cannot convert {t} to type definition: {type(t)}")
