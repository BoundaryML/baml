import abc
import typing
from .raw_wrapper import RawWrapper


T = typing.TypeVar("T", bound=typing.Any)


def filter_empty(
    x: typing.Iterable[typing.Optional[T]],
) -> typing.Iterable[T]:
    return typing.cast(typing.Iterable[T], filter(lambda v: v is not None, x))


@typing.final
class ListRawWrapper(RawWrapper):
    def __init__(self, val: typing.List[RawWrapper]) -> None:
        self.__val = val

    def as_str(self) -> typing.Optional[str]:
        # A list can always be converted to a string.
        x = filter_empty(map(lambda v: v.as_str(), self.__val))
        str_rep = f'[{", ".join(x)}]'
        return str_rep

    def as_int(self) -> typing.Optional[int]:
        if len(self.__val) == 0:
            return None
        for item in filter_empty(map(lambda v: v.as_int(), self.__val)):
            return item
        return None

    def as_float(self) -> typing.Optional[float]:
        if len(self.__val) == 0:
            return None
        for item in filter_empty(map(lambda v: v.as_float(), self.__val)):
            return item
        return None

    def as_bool(self) -> typing.Optional[bool]:
        if len(self.__val) == 0:
            return None
        for item in filter_empty(map(lambda v: v.as_bool(), self.__val)):
            return item
        return None

    def as_list(self) -> typing.Iterable[RawWrapper]:
        return self.__val

    def as_dict(
        self,
    ) -> typing.ItemsView[typing.Optional[RawWrapper], RawWrapper]:
        return {None: self}.items()
