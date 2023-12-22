import json
import typing
from .raw_wrapper import RawWrapper


T = typing.TypeVar("T", bound=typing.Any)


def filter_empty(
    x: typing.Iterable[typing.Tuple[RawWrapper, typing.Optional[T]]],
) -> typing.Iterable[typing.Tuple[RawWrapper, T]]:
    return typing.cast(
        typing.Iterable[typing.Tuple[RawWrapper, T]],
        filter(lambda v: v[1] is not None, x),
    )


@typing.final
class DictRawWrapper(RawWrapper):
    def __init__(self, val: typing.Mapping[RawWrapper, RawWrapper]) -> None:
        self.__val = val

    def as_self(self) -> typing.Any:
        return {key.as_self(): value.as_self() for key, value in self.__val.items()}

    def as_str(self, inner: bool) -> typing.Optional[str]:
        return json.dumps(self.as_self(), indent=2)

    def as_smart_str(self, inner: bool) -> typing.Optional[str]:
        return self.as_str(True)

    def as_int(self) -> typing.Optional[int]:
        if len(self.__val) == 1:
            for _, item in filter_empty(
                map(lambda kv: (kv[0], kv[1].as_int()), self.__val.items())
            ):
                return item
        return None

    def as_float(self) -> typing.Optional[float]:
        if len(self.__val) == 1:
            for _, item in filter_empty(
                map(lambda kv: (kv[0], kv[1].as_float()), self.__val.items())
            ):
                return item
        return None

    def as_bool(self) -> typing.Optional[bool]:
        if len(self.__val) == 1:
            for _, item in filter_empty(
                map(lambda kv: (kv[0], kv[1].as_bool()), self.__val.items())
            ):
                return item
        return None

    def as_list(self) -> typing.Iterable[RawWrapper]:
        return self.__val.values()

    def as_dict(
        self,
    ) -> typing.ItemsView[typing.Optional[RawWrapper], RawWrapper]:
        return self.__val.items()
