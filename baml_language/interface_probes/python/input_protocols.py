"""Typing experiment: expect errors only at the three calls in negative().

The invariant nominal Pins token distinguishes associated bindings. The
private witness method supports multiple views using ordinary overloads.
It is static evidence; a bridge must still validate the actual registration.
"""

from typing import Generic, Protocol, TypeVar, overload

O = TypeVar("O")
E = TypeVar("E")


class Pins(Generic[O, E]):
    pass


class SourceInput(Protocol[O, E]):
    def _baml_source(self, view: Pins[O, E], /) -> None: ...


class SourceRef(Generic[O, E]):
    def _baml_source(self, view: Pins[O, E], /) -> None:
        pass


class Both:
    @overload
    def _baml_source(self, view: Pins[str, ValueError], /) -> None: ...
    @overload
    def _baml_source(self, view: Pins[int, TypeError], /) -> None: ...
    def _baml_source(
        self, view: Pins[str, ValueError] | Pins[int, TypeError], /
    ) -> None:
        pass


class GenericSource(Generic[O]):
    def _baml_source(self, view: Pins[O, ValueError], /) -> None:
        pass


class MethodShaped:
    async def read(self) -> str:
        return "hi"


def text(value: SourceInput[str, ValueError]) -> None:
    pass


def integer(value: SourceInput[int, TypeError]) -> None:
    pass


def positive(
    value: Both, ref: SourceRef[str, ValueError], generic: GenericSource[str]
) -> None:
    text(value)
    integer(value)
    text(ref)
    text(generic)


def negative(
    wrong: SourceRef[str, TypeError], generic: GenericSource[int], shaped: MethodShaped
) -> None:
    text(wrong)
    text(generic)
    text(shaped)
