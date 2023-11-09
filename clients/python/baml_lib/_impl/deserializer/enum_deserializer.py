import typing

from enum import Enum


from .base_deserialzier import (
    BaseDeserializer,
    CheckLutFn,
    RawWrapper,
    Result,
    Diagnostics,
)


T = typing.TypeVar("T", bound=Enum)


@typing.final
class EnumDeserializer(BaseDeserializer[T]):
    __enm: typing.Type[T]
    __value_aliases: typing.Dict[str, str]

    def __init__(
        self,
        *,
        enm: typing.Type[T],
        aliases: typing.Dict[str, str] = {},
    ):
        super().__init__(rank=5)
        self.__enm = enm
        # This field is alias to value.
        self.__value_aliases = aliases

    def copy_with_aliases(
        self, aliases: typing.Dict[str, typing.Optional[str]]
    ) -> "EnumDeserializer[T]":
        # Use any aliases that are defined in the model.
        _aliases = {
            key: value
            for key, value in self.__value_aliases.items()
            if aliases.get(key) is not None
        }
        # Update with any aliases that are defined in the call.
        _aliases.update(
            {key: value for key, value in aliases.items() if value is not None}
        )
        return EnumDeserializer(
            enm=self.__enm,
            aliases=_aliases,
        )

    def coerce(
        self,
        raw: RawWrapper,
        diagnostics: Diagnostics,
        from_lut: CheckLutFn[T],
    ) -> Result[T]:
        parsed = raw.as_str()
        if parsed is None:
            diagnostics.push_enum_error(
                self.__enm.__name__,
                parsed,
                [item.name for item in self.__enm]
                + [f"{k} ({v})" for k, v in self.__value_aliases.items()],
            )
            return Result.failed()
        if parsed in self.__value_aliases:
            parsed = self.__value_aliases[parsed]

        try:
            parsed_item = self.__enm(parsed)
            return Result.from_value(parsed_item)
        except Exception:
            diagnostics.push_enum_error(
                self.__enm.__name__,
                parsed,
                [item.name for item in self.__enm]
                + [f"{k} ({v})" for k, v in self.__value_aliases.items()],
            )

            return Result.failed()
