import typing
from .stringify import StringifyBase
from .errors import StringifyError

T = typing.TypeVar("T")


class StringifyUnion(StringifyBase[T]):
    def __init__(self, *args: StringifyBase[typing.Any]) -> None:
        self.__args = args

    def _json_str(self) -> str:
        return " | ".join(map(lambda x: x.json, self.__args))

    def _parse(self, value: typing.Any) -> T:
        for arg in self.__args:
            try:
                return typing.cast(T, arg.parse(value))
            except StringifyError:
                pass
        raise StringifyError(f"Could not parse {value} as {self.json}")

    def vars(self) -> typing.Dict[str, str]:
        v = {}
        for arg in self.__args:
            v.update(arg.vars())
        return v
