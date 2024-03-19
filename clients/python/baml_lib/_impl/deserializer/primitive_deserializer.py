import typing

from .base_deserializer import (
    BaseDeserializer,
    CheckLutFn,
    RawWrapper,
    Result,
    Diagnostics,
)

T = typing.TypeVar("T", str, int, float, bool)


@typing.final
class PrimitiveDeserializer(BaseDeserializer[T]):
    __as_type: typing.Callable[[RawWrapper], typing.Optional[T]]
    __error_message: str

    def __init__(
        self,
        as_type: typing.Callable[[RawWrapper], typing.Optional[T]],
        error_message: str,
        rank: int,
    ):
        self.__as_type = as_type
        self.__error_message = error_message
        super().__init__(rank=rank)

    def coerce(
        self,
        raw: RawWrapper,
        diagnostics: Diagnostics,
        from_lut: CheckLutFn[T],
    ) -> Result[T]:
        parsed = self.__as_type(raw)
        if parsed is None:
            diagnostics.push_unknown_error(self.__error_message)
            return Result.failed()
        return Result.from_value(parsed)


@typing.final
class NoneDeserializer(BaseDeserializer[None]):
    def __init__(self) -> None:
        super().__init__(rank=0)

    def coerce(
        self,
        raw: RawWrapper,
        diagnostics: Diagnostics,
        from_lut: CheckLutFn[None],
    ) -> Result[None]:
        # We don't actually care about the value of raw, since we're just
        # returning None.
        return Result.from_value(None)
