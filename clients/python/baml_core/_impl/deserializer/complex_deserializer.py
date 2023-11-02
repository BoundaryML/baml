import typing

from .base_deserialzier import (
    BaseDeserializer,
    CheckLutFn,
    RawWrapper,
    Result,
    Diagnostics,
    ITypeDefinition,
)
from .raw_wrapper.primitive_wrapper import RawNoneWrapper

T = typing.TypeVar("T")


@typing.final
class ListDeserializer(BaseDeserializer[typing.List[T]]):
    def __init__(
        self,
        item: ITypeDefinition,
    ):
        super().__init__(rank=5)
        self.__item_deserializer = item

    def coerce(
        self,
        raw: RawWrapper,
        diagnostics: Diagnostics,
        from_lut: CheckLutFn[T],
    ) -> Result[typing.List[T]]:
        items: typing.List[T] = []
        item_deserializer = from_lut(self.__item_deserializer)
        for item in raw.as_list():
            parsed = item_deserializer.coerce(item, diagnostics, from_lut)
            if parsed.has_value:
                items.append(parsed.as_value)
        return Result.from_value(items)


@typing.final
class OptionalDeserializer(BaseDeserializer[typing.Optional[T]]):
    def __init__(
        self,
        item: ITypeDefinition,
    ):
        super().__init__(rank=100)
        self.__item_deserializer = item

    def coerce(
        self,
        raw: RawWrapper,
        diagnostics: Diagnostics,
        from_lut: CheckLutFn[typing.Optional[T]],
    ) -> Result[typing.Optional[T]]:
        item_deserializer = from_lut(self.__item_deserializer)
        if isinstance(raw, RawNoneWrapper):
            return Result.from_value(None)
        # TODO: Merge child errors as warnings into the parent diagnostics object.
        # The point is that if the child fails, this is optional, so we're just gonna return None
        diagnostics_child = Diagnostics("")
        item = item_deserializer.coerce(raw, diagnostics_child, from_lut)
        if item.has_value:
            return Result.from_value(item.as_value)
        return Result.from_value(None)


@typing.final
class UnionDeserializer(BaseDeserializer[T]):
    def __init__(
        self,
        *item_deserializer: ITypeDefinition,
    ):
        self.__item_deserializer = list(item_deserializer)

    def coerce(
        self,
        raw: RawWrapper,
        diagnostics: Diagnostics,
        from_lut: CheckLutFn[T],
    ) -> Result[T]:
        deserializers = sorted(
            [
                from_lut(item_deserializer)
                for item_deserializer in self.__item_deserializer
            ],
            key=lambda x: x.rank,
            reverse=True,
        )

        for deserializer in deserializers:
            item = deserializer.coerce(raw, diagnostics, from_lut)
            if item.has_value:
                return Result.from_value(item.as_value)
        return Result.failed()
