import abc
import typing


from .diagnostics import Diagnostics, DeserializerError, DeserializerWarning
from .raw_wrapper import RawWrapper
from .type_definition import ITypeDefinition

T = typing.TypeVar("T")


class Result(typing.Generic[T]):
    has_value: bool
    value: typing.Optional[T]

    def __init__(self, *, has_value: bool, value: typing.Optional[T]) -> None:
        self.has_value = has_value
        self.value = value

    @staticmethod
    def from_value(value: T) -> "Result[T]":
        return Result(has_value=True, value=value)

    @staticmethod
    def failed() -> "Result[T]":
        return Result(has_value=False, value=None)

    @property
    def as_value(self) -> T:
        assert self.has_value, "Result has no value"
        # Technically self.value could be None, since T could itself be Optional.
        return typing.cast(T, self.value)


CoerceFn = typing.Callable[
    [
        RawWrapper,
        Diagnostics,
        typing.Callable[[str], typing.Optional["BaseDeserializer[T]"]],
    ],
    Result[T],
]


class BaseDeserializer(typing.Generic[T], metaclass=abc.ABCMeta):
    def __init__(self, rank: int) -> None:
        self.__rank = rank

    @property
    def rank(self) -> int:
        # Rank means lower is less specific, higher is more specific.
        # When searching for a deserializer, we want to pick the most specific.
        return self.__rank

    @abc.abstractmethod
    def coerce(
        self,
        raw: RawWrapper,
        diagnostics: Diagnostics,
        from_lut: typing.Callable[
            [ITypeDefinition], "typing.Optional[BaseDeserializer[typing.Any]]"
        ],
    ) -> Result[T]:
        raise NotImplementedError()


CheckLutFn = typing.Callable[[ITypeDefinition], typing.Optional[BaseDeserializer[T]]]


__all__ = [
    "BaseDeserializer",
    "Result",
    "Diagnostics",
    "DeserializerError",
    "DeserializerWarning",
    "RawWrapper",
    "CoerceFn",
    "ITypeDefinition",
]
