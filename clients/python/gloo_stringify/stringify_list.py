import json
import typing
from .stringify import StringifyBase
from .stringify_primitive import StringifyPrimitive
from .stringify_class import StringifyClass
from .errors import StringifyError

T = typing.TypeVar("T")


class StringifyList(StringifyBase[typing.List[T]]):
    def __init__(self, args: StringifyBase[T]) -> None:
        self.__args = args

    def _json_str(self) -> str:
        if isinstance(self.__args, (StringifyPrimitive, StringifyClass)):
            return f"{self.__args.json}[]"
        return f"({self.__args.json})[]"

    def _parse(self, value: typing.Any) -> typing.List[T]:
        if isinstance(value, str):
            if value.startswith("[") and value.endswith("]"):
                value = json.loads(value)
        # Make sure we have a list
        if not isinstance(value, list):
            value = [value]
        result: typing.List[T] = []
        for item in value:
            try:
                parsed = self.__args.parse(item)
                result.append(parsed)
            except StringifyError:
                pass
        return result

    def vars(self) -> typing.Dict[str, str]:
        return self.__args.vars()
