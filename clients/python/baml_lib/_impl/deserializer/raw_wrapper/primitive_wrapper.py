import typing

from .raw_wrapper import RawWrapper

T = typing.TypeVar("T", bool, int, float)


@typing.final
class RawBaseWrapper(RawWrapper, typing.Generic[T]):
    __val: T

    def __init__(self, val: T) -> None:
        self.__val = val

    def as_str(self) -> typing.Optional[str]:
        return str(self.__val)

    def as_int(self) -> typing.Optional[int]:
        if isinstance(self.__val, int):
            return self.__val
        if isinstance(self.__val, float):
            return int(self.__val)
        if isinstance(self.__val, bool):
            if self.__val:
                return 1
            return 0
        raise Exception("Unreachable code")

    def as_float(self) -> typing.Optional[float]:
        if isinstance(self.__val, float):
            return self.__val
        if isinstance(self.__val, int):
            return float(self.__val)
        if isinstance(self.__val, bool):
            if self.__val:
                return 1.0
            return 0.0
        raise Exception("Unreachable code")

    def as_bool(self) -> typing.Optional[bool]:
        if isinstance(self.__val, bool):
            return self.__val
        if self.__val == 0:
            return False
        if self.__val == 1:
            return True
        # TODO: Add a warning here
        return True

    def as_list(self) -> typing.Iterable[RawWrapper]:
        # TODO: Add a warning here
        return [self]

    def as_dict(
        self,
    ) -> typing.ItemsView[typing.Optional[RawWrapper], RawWrapper]:
        # TODO: Add a warning here
        return {None: self}.items()


@typing.final
class RawStringWrapper(RawWrapper):
    def __init__(
        self,
        val: str,
        as_obj: typing.Optional[RawWrapper],
        as_list: typing.Optional[RawWrapper],
    ) -> None:
        self.__val = val
        self.__as_obj = as_obj
        self.__as_list = as_list

    def as_str(self) -> typing.Optional[str]:
        return self.__val

    def as_int(self) -> typing.Optional[int]:
        return None

    def as_float(self) -> typing.Optional[float]:
        return None

    def as_bool(self) -> typing.Optional[bool]:
        return None

    def as_list(self) -> typing.Iterable[RawWrapper]:
        if self.__as_list is not None:
            return self.__as_list.as_list()
        if self.__as_obj is not None:
            return [self.__as_obj]
        return [self]

    def as_dict(
        self,
    ) -> typing.ItemsView[typing.Optional[RawWrapper], RawWrapper]:
        if self.__as_obj is not None:
            return self.__as_obj.as_dict()
        return {None: self}.items()

    def __repr__(self) -> str:
        return f"RawStringWrapper\n---\n{self.__val}\n---"


@typing.final
class RawNoneWrapper(RawWrapper):
    def __init__(self) -> None:
        pass

    def as_str(self) -> typing.Optional[str]:
        return None

    def as_int(self) -> typing.Optional[int]:
        return None

    def as_float(self) -> typing.Optional[float]:
        return None

    def as_bool(self) -> typing.Optional[bool]:
        return None

    def as_list(self) -> typing.Iterable[RawWrapper]:
        return []

    def as_dict(
        self,
    ) -> typing.ItemsView[typing.Optional[RawWrapper], RawWrapper]:
        return {}.items()

    def __repr__(self) -> str:
        return "RawNoneWrapper\n---\nNone\n---"
