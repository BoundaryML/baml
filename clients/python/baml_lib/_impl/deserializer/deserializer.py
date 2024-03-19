import typing

from .base_deserializer import BaseDeserializer, ITypeDefinition, Diagnostics
from .enum_deserializer import EnumDeserializer
from .object_deserializer import ObjectDeserializer
from .complex_deserializer import (
    ListDeserializer,
    UnionDeserializer,
    OptionalDeserializer,
)
from .type_definition import type_to_definition

from .exports import (
    DefaultDeserializerLUT,
    GeneratedDeserializerLUT,
)

from .raw_wrapper import from_string


T = typing.TypeVar("T")


class Deserializer(typing.Generic[T]):
    __lut: typing.Dict[str, BaseDeserializer[typing.Any]]
    __target_output_type: ITypeDefinition

    def __init__(self, output_target: typing.Type[T]) -> None:
        self.__lut = {}
        self.__target_output_type = type_to_definition(output_target)

    def overload(
        self, name: str, aliases: typing.Dict[str, typing.Optional[str]]
    ) -> None:
        assert name in GeneratedDeserializerLUT, f"Overloading {name} is not allowed."
        default_serializer = GeneratedDeserializerLUT.get(name)
        # default_serializer is a bound method, so we need to get the owning class
        assert default_serializer is not None, f"Could not find owning class for {name}"
        assert name not in self.__lut, f"Overloading {name} twice is not allowed."
        if isinstance(default_serializer, (EnumDeserializer, ObjectDeserializer)):
            deserializer = default_serializer.copy_with_aliases(aliases)
            self.__lut[name] = deserializer
        else:
            assert (
                False
            ), f"Cannot overload {name} with aliases for {type(default_serializer)}"

    def __from_lut(self, dfn: ITypeDefinition) -> BaseDeserializer[T]:
        if dfn["type"] == "List":
            assert "item" in dfn, "List type must have an item"
            return typing.cast(BaseDeserializer[T], ListDeserializer(item=dfn["item"]))
        if dfn["type"] == "Union":
            assert "choices" in dfn, "Union type must have choices"
            return UnionDeserializer(
                *dfn["choices"],
            )
        if dfn["type"] == "Optional":
            assert "item" in dfn, "Optional type must have an item"
            return typing.cast(
                BaseDeserializer[T], OptionalDeserializer(item=dfn["item"])
            )
        if dfn["type"] == "Ref":
            assert "ref" in dfn, "Ref type must have a ref"
            key = dfn["ref"]
            if key.__name__ in self.__lut:
                return self.__lut[key.__name__]
            found = GeneratedDeserializerLUT.get(key.__name__)
            assert found is not None, f"Could not find deserializer for {key.__name__}"
            return found
        found = DefaultDeserializerLUT.get(dfn["type"])
        assert found is not None, f"Could not find deserializer for {dfn['type']}"
        return found

    def from_string(self, s: str) -> T:
        diagnostics = Diagnostics(s)
        raw = from_string(s, diagnostics)
        deserializer = self.__from_lut(self.__target_output_type)
        result = deserializer.coerce(raw, diagnostics, self.__from_lut)
        diagnostics.to_exception()
        return result.as_value
