from types import TracebackType
from typing import Any, Generic, AsyncIterator, Optional, Type
import typing
from typing_extensions import get_origin, TypedDict
from pydantic import BaseModel
from baml_core.provider_manager.llm_response import LLMResponse
from baml_lib._impl.deserializer import Deserializer
from baml_core.stream import JSONParser

TYPE = typing.TypeVar("TYPE")
partial_parser = JSONParser()


class Unset:
    pass


_ValDict = TypedDict("_ValDict", {"value": Any})
_PartialDict = TypedDict("_PartialDict", {"delta": str, "parsed": Optional[_ValDict]})


# Note the generic here could be partial or actual type
class ValueWrapper(Generic[TYPE]):
    __value: typing.Union[TYPE, Unset]
    __is_set: bool

    def __init__(self, val: typing.Union[TYPE, Unset], is_set: bool) -> None:
        self.__value = val
        self.__is_set = is_set

    @staticmethod
    def unset() -> "ValueWrapper[TYPE]":
        return ValueWrapper[TYPE](Unset(), False)

    @staticmethod
    def from_value(val: TYPE) -> "ValueWrapper[TYPE]":
        return ValueWrapper[TYPE](val, True)

    @property
    def has_value(self) -> bool:
        return self.__is_set

    @property
    def value(self) -> TYPE:
        assert self.__is_set, "No value set."
        assert not isinstance(self.__value, Unset)
        return self.__value

    def json(self) -> Optional[_ValDict]:
        if not self.__is_set:
            return None
        val = self.value
        return {"value": val.model_dump() if isinstance(val, BaseModel) else val}


class PartialValueWrapper(Generic[TYPE]):
    __partial: ValueWrapper[TYPE]
    __delta: str

    def __init__(self, partial: ValueWrapper[TYPE], delta: str) -> None:
        self.__partial = partial
        self.__delta = delta

    @staticmethod
    def from_parseable(partial: TYPE, delta: str) -> "PartialValueWrapper[TYPE]":
        return PartialValueWrapper[TYPE](ValueWrapper.from_value(partial), delta)

    @staticmethod
    def from_parse_failure(delta: str) -> "PartialValueWrapper[TYPE]":
        return PartialValueWrapper[TYPE](ValueWrapper.unset(), delta)

    @property
    def delta(self) -> str:
        return self.__delta

    @property
    def is_parseable(self) -> bool:
        return self.__partial.has_value

    @property
    def parsed(self) -> TYPE:
        assert self.is_parseable, "No parsed value."
        return self.__partial.value

    def json(self) -> _PartialDict:
        return {
            "delta": self.delta,
            "parsed": self.__partial.json(),
        }


class TextDelta(BaseModel):
    delta: str


# We need another generic here
PARTIAL_TYPE = typing.TypeVar("PARTIAL_TYPE")


class AsyncStream(Generic[TYPE, PARTIAL_TYPE]):
    __stream: typing.Optional[AsyncIterator[LLMResponse]] = None
    __is_stream_completed: bool
    __stream_cb: typing.Callable[[], AsyncIterator[LLMResponse]]
    __total_text: str

    def __init__(
        self,
        stream_cb: typing.Callable[[], AsyncIterator[LLMResponse]],
        partial_deserializer: Deserializer[PARTIAL_TYPE],
        final_deserializer: Deserializer[TYPE],
    ):
        self.__partial_deserializer = partial_deserializer
        self.__deserializer = final_deserializer
        self.__is_stream_completed = False
        self.__stream_cb = stream_cb
        self.__total_text = ""

    async def __aenter__(self) -> "AsyncStream[TYPE, PARTIAL_TYPE]":
        self.__stream = self.__stream_cb()
        return self

    async def __aexit__(
        self,
        exc_type: Optional[Type[BaseException]],
        exc_val: Optional[BaseException],
        exc_tb: Optional[TracebackType],
    ) -> None:
        await self.__until_done()

    async def _parse_stream_chunk(
        self, total_text: str, delta: str
    ) -> PartialValueWrapper[PARTIAL_TYPE]:
        # We do some magic here to get the orig_class
        t = typing.get_args(self.__partial_deserializer.__orig_class__)[  # type: ignore
            0
        ]  # deserializer only has 1 type arg
        is_list = get_origin(t) is list
        if get_origin(t) is list or (
            isinstance(t, type) and not issubclass(t, (str, bytes, int, float))
        ):
            # get the text that's between the first [ and the last ], and if the last ] is missing, get the whole remaining text.
            start_char = "[" if is_list else "{"
            end_char = "]" if is_list else "}"
            start_index = total_text.find(start_char)
            end_index = start_index
            bracket_count = 0
            for i, char in enumerate(total_text[start_index:]):
                if char == start_char:
                    bracket_count += 1
                elif char == end_char:
                    bracket_count -= 1
                if bracket_count == 0:
                    end_index = start_index + i
                    break
            else:  # No matching closing bracket found
                end_index = len(total_text)
            first_partial_json_substr = total_text[start_index : end_index + 1]

            # Fill in the rest of the json
            json_string = partial_parser.parse(first_partial_json_substr)
            # run through our deserializer
            parsed = self.__partial_deserializer.from_string(json_string)
            return PartialValueWrapper.from_parseable(partial=parsed, delta=delta)

        else:
            parsed = self.__partial_deserializer.from_string(total_text)
            return PartialValueWrapper.from_parseable(partial=parsed, delta=delta)

    def __get_stream(self) -> AsyncIterator[LLMResponse]:
        assert self.__stream is not None, "Stream not initialized"
        return self.__stream

    @property
    async def parsed_stream(self) -> AsyncIterator[PartialValueWrapper[PARTIAL_TYPE]]:
        if self.__is_stream_completed:
            return
        async for response in self.__get_stream():
            try:
                self.__total_text += response.generated
                yield await self._parse_stream_chunk(
                    self.__total_text, delta=response.generated
                )
            except Exception:
                yield PartialValueWrapper.from_parse_failure(delta=response.generated)

        self.__is_stream_completed = True

    async def get_final_response(self) -> ValueWrapper[TYPE]:
        await self.__until_done()
        # This ensures any deserialization exception is bubbled up
        final_response = ValueWrapper.from_value(
            self.__deserializer.from_string(self.__total_text)
        )

        return final_response

    async def __until_done(self) -> None:
        if self.__is_stream_completed:
            return
        async for _ in self.parsed_stream:
            pass
