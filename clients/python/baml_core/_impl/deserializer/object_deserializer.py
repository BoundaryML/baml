import typing

from enum import Enum
from pydantic import BaseModel

from .base_deserialzier import (
    BaseDeserializer,
    CheckLutFn,
    ITypeDefinition,
    RawWrapper,
    Result,
    Diagnostics,
    DeserializerError,
    DeserializerWarning,
)
from .type_definition import type_to_definition


T = typing.TypeVar("T", bound=BaseModel)


def _generate_type_definitions_for_model(
    model: typing.Type[T],
) -> typing.Dict[str, ITypeDefinition]:
    schema = model.model_json_schema()
    field_definitions = {}

    for field_name, field_info in model.model_fields.items():
        field_type = field_info.annotation
        if field_type is None:
            raise NotImplementedError(
                f"Cannot generate type definition for {model.__name__}.{field_name} with no type."
            )
        field_definitions[field_name] = type_to_definition(field_type)

    return field_definitions


@typing.final
class ObjectDeserializer(BaseDeserializer[T]):
    __model: typing.Type[T]

    def __init__(
        self,
        *,
        model: typing.Type[T],
        alias_to_field: typing.Dict[str, str] = {},
    ):
        super().__init__(rank=5)
        self.__model = model
        self.__fields = _generate_type_definitions_for_model(model)
        self.__alias_to_field = alias_to_field

    def copy_with_aliases(
        self, aliases: typing.Dict[str, typing.Optional[str]]
    ) -> "ObjectDeserializer[T]":
        # Use any aliases that are defined in the model.
        _aliases = {
            key: value
            for key, value in self.__alias_to_field.items()
            if aliases.get(key) is not None
        }
        # Update with any aliases that are defined in the call.
        _aliases.update(
            {key: value for key, value in aliases.items() if value is not None}
        )
        return ObjectDeserializer(
            model=self.__model,
            alias_to_field=_aliases,
        )

    def coerce(
        self,
        raw: RawWrapper,
        diagnostics: Diagnostics,
        from_lut: CheckLutFn[typing.Any],
    ) -> Result[T]:
        items: typing.Dict[str, typing.Any] = {}
        for raw_key, item in raw.as_dict():
            if raw_key is None:
                continue
            key = raw_key.as_str()
            if key is None:
                continue

            if key in self.__alias_to_field:
                key = self.__alias_to_field[key]
            meta = self.__fields.get(key)
            if meta is None:
                diagnostics.push_warning(
                    DeserializerWarning.create_warning(f"Unknown key {key}")
                )
                continue

            value_deserializer = from_lut(meta)

            if value_deserializer is None:
                diagnostics.push_warning(
                    DeserializerWarning.create_warning(f"Unknown key {key}")
                )
                continue
            parsed = value_deserializer.coerce(item, diagnostics, from_lut)
            if parsed.has_value:
                items[key] = parsed.as_value

        try:
            parsed_item = self.__model(**items)
            return Result.from_value(parsed_item)
        except Exception as e:
            diagnostics.push_error(
                DeserializerError.create_error(
                    f"Failed to parse into {self.__model.__name__}: {e}"
                )
            )
            return Result.failed()
