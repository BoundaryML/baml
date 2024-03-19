from enum import Enum
import typing

from pydantic import BaseModel
from .base_deserializer import BaseDeserializer
from .primitive_deserializer import PrimitiveDeserializer, NoneDeserializer
from .enum_deserializer import EnumDeserializer
from .object_deserializer import ObjectDeserializer


DefaultDeserializerLUT: typing.Dict[
    typing.Union[typing.Literal["None"], typing.Type[typing.Any]],
    BaseDeserializer[typing.Any],
] = {
    "None": NoneDeserializer(),
    str: PrimitiveDeserializer(lambda x: x.as_str(inner=False), "Expected str", rank=1),
    bool: PrimitiveDeserializer(lambda x: x.as_bool(), "Expected bool", rank=2),
    int: PrimitiveDeserializer(lambda x: x.as_int(), "Expected int", rank=3),
    float: PrimitiveDeserializer(lambda x: x.as_float(), "Expected float", rank=4),
}

GeneratedDeserializerLUT: typing.Dict[str, BaseDeserializer[typing.Any]] = {}

ENM = typing.TypeVar("ENM", bound=Enum)


T = typing.TypeVar("T", bound=typing.Union[BaseModel, Enum])


def register_deserializer(
    aliases: typing.Dict[str, str] = {},
) -> typing.Callable[[typing.Type[T]], typing.Type[T]]:
    """
    Decorator to register a new deserializer.

    Args:
        aliases (Dict[str, str], optional): The alias to enum mapping. Defaults to {}.
        alias_to_field (Dict[str, str], optional): The alias to field mapping. Defaults to {}.

    Raises:
        AssertionError: If the type is already registered.
    """

    def decorator(cls: typing.Type[T]) -> typing.Type[T]:
        global GeneratedDeserializerLUT
        assert (
            cls.__name__ not in GeneratedDeserializerLUT
        ), f"Cannot register {cls.__name__} twice."
        if issubclass(cls, Enum):
            GeneratedDeserializerLUT[cls.__name__] = EnumDeserializer(
                enm=cls, aliases=aliases
            )
        else:
            assert issubclass(
                cls, BaseModel
            ), f"Cannot register {cls.__name__}. Must be a subclass of BaseModel."
            GeneratedDeserializerLUT[cls.__name__] = ObjectDeserializer(
                model=cls,
                alias_to_field=aliases,
            )
        return cls

    return decorator


__all__ = [
    "register_deserializer",
]
