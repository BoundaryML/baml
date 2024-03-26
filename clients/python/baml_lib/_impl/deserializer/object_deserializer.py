import typing

from pydantic import BaseModel

from .base_deserializer import (
    BaseDeserializer,
    CheckLutFn,
    ITypeDefinition,
    RawWrapper,
    Result,
    Diagnostics,
)
from .type_definition import type_to_definition


T = typing.TypeVar("T", bound=BaseModel)


def _generate_type_definitions_for_model(
    model: typing.Type[T],
) -> typing.Dict[str, ITypeDefinition]:
    field_definitions: typing.Dict[str, ITypeDefinition] = {}

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
        # This field is alias to value.
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
        diagnostics.push_scope(self.__model.__name__)
        items: typing.Dict[str, typing.Any] = {}
        for raw_key, item in raw.as_dict():
            if raw_key is None:
                continue
            key = raw_key.as_str(inner=True)
            if key is None:
                continue

            if key in self.__alias_to_field:
                key = self.__alias_to_field[key]
            meta = self.__fields.get(key)
            if meta is None:
                diagnostics.push_unkown_warning(f"Unknown key {key}")
                continue

            value_deserializer = from_lut(meta)

            if value_deserializer is None:
                diagnostics.push_unkown_warning(f"Unknown key {key}")
                continue
            diagnostics.push_scope(key)
            parsed = value_deserializer.coerce(item, diagnostics, from_lut)
            diagnostics.pop_scope(errors_as_warnings=False)
            if parsed.has_value:
                items[key] = parsed.as_value

        # Check if all required keys are present.
        missing_keys = []
        for key, meta in self.__fields.items():

            def is_optional(meta: ITypeDefinition) -> bool:
                if meta["type"] == "Optional":
                    return True
                if meta["type"] == "Union":
                    for c in meta["choices"]:
                        if is_optional(c):
                            return True
                return False

            def is_list(meta: ITypeDefinition) -> bool:
                if meta["type"] == "List":
                    return True
                if meta["type"] == "Union":
                    for c in meta["choices"]:
                        if is_list(c):
                            return True
                if meta["type"] == "Optional":
                    return is_list(meta["item"])
                return False

            if items.get(key) is None:
                if not is_optional(meta):
                    if is_list(meta):
                        items[key] = []
                    else:
                        missing_keys.append(key)

        if missing_keys:
            diagnostics.push_missing_keys_error(missing_keys)
            diagnostics.pop_scope(errors_as_warnings=False)
            return Result.failed()

        try:
            parsed_item = self.__model(**items)
            return Result.from_value(parsed_item)
        except Exception as e:
            diagnostics.push_unknown_error(
                f"Failed to parse into {self.__model.__name__}: {e}"
            )
            return Result.failed()
        finally:
            diagnostics.pop_scope(errors_as_warnings=False)
