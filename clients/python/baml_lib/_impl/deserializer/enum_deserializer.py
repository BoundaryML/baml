from __future__ import annotations
import typing
from typing import Generator, List, Tuple

from enum import Enum
import re

from .base_deserializer import (
    BaseDeserializer,
    CheckLutFn,
    RawWrapper,
    Result,
    Diagnostics,
)


T = typing.TypeVar("T", bound=Enum)


@typing.final
class EnumDeserializer(BaseDeserializer[T]):
    __enm: typing.Type[T]
    __value_aliases: typing.Dict[str, str]

    def __init__(
        self,
        *,
        enm: typing.Type[T],
        aliases: typing.Dict[str, str] = {},
    ):
        super().__init__(rank=5)
        self.__enm = enm
        # This field is alias to value.
        self.__value_aliases = aliases

    def copy_with_aliases(
        self, aliases: typing.Dict[str, typing.Optional[str]]
    ) -> "EnumDeserializer[T]":
        # Use any aliases that are defined in the model.
        _aliases = {
            key: value
            for key, value in self.__value_aliases.items()
            if aliases.get(key) is not None
        }
        # Update with any aliases that are defined in the call.
        _aliases.update(
            {key: value for key, value in aliases.items() if value is not None}
        )
        return EnumDeserializer(
            enm=self.__enm,
            aliases=_aliases,
        )

    def aliases(self) -> Generator[Tuple[str, T], None, None]:
        for item in self.__enm:
            yield item.name.lower(), item
        for alias, value_name in self.__value_aliases.items():
            yield alias.lower(), self.__enm(value_name)

    def normalized_aliases(self) -> Generator[Tuple[str, T], None, None]:
        for item in self.__enm:
            yield item.name.lower(), item
        for alias, value_name in self.__value_aliases.items():
            yield re.sub("[^a-zA-Z0-9]+", " ", alias.lower()), self.__enm(value_name)

    def coerce(
        self,
        raw: RawWrapper,
        diagnostics: Diagnostics,
        from_lut: CheckLutFn[T],
    ) -> Result[T]:
        expected = [item.name for item in self.__enm] + [
            f"{k} ({v})" for k, v in self.__value_aliases.items()
        ]

        parsed = raw.as_smart_str(inner=True)
        if parsed is None:
            diagnostics.push_enum_error(self.__enm.__name__, parsed, expected)
            return Result.failed()

        def search(contents: str, aliases: List[Tuple[str, T]]) -> T | None:
            for alias, value in aliases:
                if alias == contents:
                    return value

            for alias, value in aliases:
                if contents.endswith(f": {alias}"):
                    return value
                if contents.endswith(f"\n\n{alias}"):
                    return value

            return None

        value = search(parsed.strip().lower(), list(self.aliases()))
        if value:
            return Result.from_value(value)

        value2 = search(
            re.sub("[^a-zA-Z0-9]+", " ", parsed.strip().lower()),
            list(self.normalized_aliases()),
        )
        if value2:
            return Result.from_value(value2)

        def find_most_common(contents: str, aliases: List[Tuple[str, T]]) -> T | None:
            counts = []
            for alias, value in aliases:
                matches = list(re.finditer(rf"\b{re.escape(alias)}\b", contents))
                if matches:
                    # Count the occurrences and find the first match index
                    count = len(matches)
                    first_match_index = matches[0].start()
                    counts.append((count, first_match_index, value))

            # Sort by frequency (descending) and then by the first match index (ascending)
            counts.sort(key=lambda x: (-x[0], x[1]))
            if counts:
                # in case of a tie, return None
                if len(counts) > 1 and counts[0][0] == counts[1][0]:
                    return None
                return counts[0][2]
            return None

        most_common = find_most_common(parsed.strip().lower(), list(self.aliases()))
        if most_common:
            return Result.from_value(most_common)

        most_common2 = find_most_common(
            re.sub("[^a-zA-Z0-9]+", " ", parsed.strip().lower()),
            list(self.normalized_aliases()),
        )
        if most_common2:
            return Result.from_value(most_common2)

        diagnostics.push_enum_error(self.__enm.__name__, parsed, expected)
        return Result.failed()
