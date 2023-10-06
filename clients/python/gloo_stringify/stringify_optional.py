import typing
from .stringify import StringifyBase

U = typing.TypeVar("U")


class StringifyOptional(StringifyBase[typing.Optional[U]]):
    def __init__(self, args: StringifyBase[U]) -> None:
        self.__args = args

    def _json_str(self) -> str:
        return f"{self.__args.json} | null"

    def _parse(self, value: typing.Any) -> typing.Optional[U]:
        if value is None:
            return None
        if isinstance(value, str):
            if value.strip().lower() == "null":
                return None
        return self.__args.parse(value)

    def vars(self) -> typing.Dict[str, str]:
        return self.__args.vars()
