from __future__ import annotations
import typing
from .stringify import StringifyBase, StringifyRemappedField, StringifyCtx
from .errors import StringifyError
from pydantic import BaseModel
import json

U = typing.TypeVar("U", bound=BaseModel)
T = typing.TypeVar("T")


class FieldDescription(typing.Generic[T]):
    def __init__(
        self,
        name: str,
        description: None | str,
        type_desc: StringifyBase[T],
    ) -> None:
        self.name = name
        self.__description = description
        self.__type = type_desc

    @property
    def _type(self) -> StringifyBase[T]:
        return self.__type

    @property
    def description(self) -> str:
        if self.__description:
            return self.__description
        return self.__type.json

    def __str__(self) -> str:
        return self.name

    @property
    def json(self) -> str:
        return self.__type.json

    def vars(self) -> typing.Dict[str, str]:
        return self.__type.vars()


def update_field_description(
    field: FieldDescription[T],
    *,
    update: typing.Optional[StringifyRemappedField] = None,
) -> FieldDescription[T]:
    if update is None:
        return field
    return FieldDescription(
        name=update.name or field.name,
        description=update.description or field.description,
        type_desc=field._type,
    )


class StringifyClass(StringifyBase[U]):
    def __new__(cls, *args: typing.Any, **kwargs: typing.Any) -> "StringifyClass[U]":
        instance = StringifyCtx.get_instance_for_current_context(cls)
        if not instance:
            instance = super(StringifyClass, cls).__new__(cls)
            StringifyCtx.set_instance_for_current_context(cls, instance)
            instance._initialized = False  # type: ignore
        else:
            instance._initialized = True
        return instance

    def __init__(
        self,
        *,
        model: typing.Type[U],
        values: typing.Dict[str, FieldDescription[typing.Any]],
        updates: typing.Dict[str, StringifyRemappedField],
    ) -> None:
        # If this instance is already initialized, don't execute __init__ again
        if getattr(self, "_initialized", False):
            return None

        props = {
            k: update_field_description(v, update=updates.get(k))
            for k, v in values.items()
        }
        self.__props = props
        self.__reverse_props = {v.name.lower(): k for k, v in props.items()}
        self.__model = model
        self.__name = model.__name__

    def __getattribute__(self, item: str) -> typing.Any:
        # Attempt to return the attribute using the standard method
        try:
            return super().__getattribute__(item)
        except AttributeError:
            # If it fails, use the custom logic in __getattr__
            return self.__getattr__(item)

    def __getattr__(self, item: str) -> FieldDescription[typing.Any]:
        # This will only be called if the attribute is not found through the standard methods
        res = self.__props.get(item)
        if res is None:
            raise AttributeError(f"Unknown field: {item}")
        return res

    def _json_str(self) -> str:
        vals = [f'"{v.name}": {v.description}' for v in self.__props.values()]
        # join the values with a newline and indent them
        joined = ",\n".join(f"  {v}" for v in vals)
        return "{\n" + joined + "\n}"

    def _parse(self, value: typing.Any) -> U:
        if isinstance(value, str):
            value = json.loads(value)
        if not isinstance(value, dict):
            raise StringifyError(f"Expected dict, got {value} ({type(value)})")

        # Force all keys to be strings.
        dict_value = {str(k): v for k, v in value.items()}

        # Replace all keys with the renamed keys
        props = self.__props
        rev_props = self.__reverse_props
        dict_value = {
            rev_props[k.lower()]: props[rev_props[k.lower()]]._type.parse(v)
            for k, v in dict_value.items()
            if k.lower() in rev_props
        }
        try:
            return self.__model.model_validate(dict_value)
        except Exception as e:
            raise StringifyError(
                f"Expected {self.__name} as {self.json}, got {value} ({type(value)}): {e}"
            )

    def vars(self) -> typing.Dict[str, str]:
        v = {
            f"{self.__name}.{k}": {
                "name": v.name,
                "description": v.description,
                "json": v.json,
            }
            for k, v in self.__props.items()
        }
        # Flatten the dict
        x = {f"{k}.{k2}": v2 for k, v in v.items() for k2, v2 in v.items()}

        for k, v1 in self.__props.items():
            x[f"{self.__name}.{k}"] = v1.name

        x[f"{self.__name}.json"] = self.json
        return x
