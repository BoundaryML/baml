from __future__ import annotations
import typing
from enum import Enum
from .stringify import StringifyBase, StringifyRemappedField, StringifyCtx, as_singular
from .errors import StringifyError

T = typing.TypeVar("T", bound=Enum)


class EnumFieldDescription:
    def __init__(self, *, name: str, description: None | str, skip: bool) -> None:
        self.name = name
        self.description = description
        self.skip = skip

    def __str__(self) -> str:
        return self.name

    def to_enum_str(self) -> str:
        if self.description is None:
            return self.name
        return f"{self.name}: {self.description}"


def update_field_description(
    field: EnumFieldDescription,
    *,
    update: typing.Optional[StringifyRemappedField] = None,
) -> EnumFieldDescription:
    if update is None:
        return field
    return EnumFieldDescription(
        name=update.name or field.name,
        description=update.description or field.description,
        skip=update.skip or field.skip,
    )


class StringifyEnum(StringifyBase[T]):
    def __new__(cls, *args: typing.Any, **kwargs: typing.Any) -> "StringifyEnum[T]":
        instance = StringifyCtx.get_instance_for_current_context(cls)
        if not instance:
            instance = super(StringifyEnum, cls).__new__(cls)
            StringifyCtx.set_instance_for_current_context(cls, instance)
            instance._initialized = False  # type: ignore
        else:
            instance._initialized = True
        return instance

    def __init__(
        self,
        *,
        values: typing.Dict[T, EnumFieldDescription],
        updates: typing.Dict[str, StringifyRemappedField],
    ) -> None:
        # If this instance is already initialized, don't execute __init__ again
        if getattr(self, "_initialized", False):
            return None

        self.__name = list(values.keys())[0].__class__.__name__
        props = {
            k: update_field_description(v, update=updates.get(k.name))
            for k, v in values.items()
        }
        self.__props = {k.name: v for k, v in props.items()}
        self.__reverse_props = {v.name.lower(): k for k, v in props.items()}

    def __getattribute__(self, item: str) -> typing.Any:
        # Attempt to return the attribute using the standard method
        try:
            return super().__getattribute__(item)
        except AttributeError:
            # If it fails, use the custom logic in __getattr__
            return self.__getattr__(item)

    def __getattr__(self, item: str) -> EnumFieldDescription:
        # This will only be called if the attribute is not found through the standard methods
        res = self.__props.get(item)
        if res is None:
            raise AttributeError(f"Unknown field: {item}")
        return res

    @property
    def names(self) -> str:
        return " | ".join(
            [f'"{val.name}"' for val in self.__props.values() if not val.skip]
        )

    @property
    def values(self) -> str:
        return "\n".join(
            [val.to_enum_str() for val in self.__props.values() if not val.skip]
        )

    @property
    def description(self) -> str:
        return self.names

    def _json_str(self) -> str:
        return self.names

    def _parse(self, value: typing.Any) -> T:
        value = as_singular(value)

        if not isinstance(value, str):
            raise StringifyError(f"Invalid enum: {value}: {type(value)}")
        val = self.__reverse_props.get(value.lower())
        if val is None:
            raise StringifyError(f"Invalid enum: {value}: {type(value)}")
        return val

    def vars(self) -> typing.Dict[str, str]:
        v = {
            f"{self.__name}.names": self.names,
            f"{self.__name}.values": self.values,
        }
        for k, val in self.__props.items():
            v[f"{self.__name}.{k}"] = val.name
            v[f"{self.__name}.{k}.name"] = val.name
            v[f"{self.__name}.{k}.desc"] = val.description or ""
        return v
