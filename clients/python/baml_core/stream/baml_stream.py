from typing import Any, Dict, Generic, AsyncIterator
import json
import typing
from pydantic import BaseModel
from baml_core.provider_manager import LLMResponse
from baml_lib._impl.deserializer import Deserializer
from baml_core.stream import JSONParser

TYPE = typing.TypeVar("TYPE")
PARTIAL_TYPE = typing.TypeVar("PARTIAL_TYPE")
partial_parser = JSONParser()


class Unset:
    pass


# Note the generic here could be partial or actual type
class ValueWrapper(Generic[TYPE]):
    __value: typing.Union[TYPE, Unset]
    __is_set: bool

    def __init__(self, val: TYPE, is_set: bool) -> None:
        self.__value = val
        self.__is_set = is_set

    @staticmethod
    def unset() -> "ValueWrapper[TYPE]":
        return ValueWrapper[TYPE](Unset, False)  # type: ignore

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

    def json(self) -> Dict[str, Any] | None:
        if not self.__is_set:
            return None
        val = self.value
        return {"value": val.model_dump() if isinstance(val, BaseModel) else val}


class PartialValueWrapper(Generic[PARTIAL_TYPE]):
    __partial: ValueWrapper[PARTIAL_TYPE]
    __delta: str

    def __init__(self, partial: ValueWrapper[PARTIAL_TYPE], delta: str) -> None:
        self.__partial = partial
        self.__delta = delta

    @staticmethod
    def from_parseable(
        partial: PARTIAL_TYPE, delta: str
    ) -> "PartialValueWrapper[PARTIAL_TYPE]":
        return PartialValueWrapper[PARTIAL_TYPE](
            ValueWrapper.from_value(partial), delta
        )

    @staticmethod
    def from_parse_failure(delta: str) -> "PartialValueWrapper[PARTIAL_TYPE]":
        return PartialValueWrapper[PARTIAL_TYPE](ValueWrapper.unset(), delta)

    @property
    def delta(self) -> str:
        return self.__delta

    @property
    def is_parseable(self) -> bool:
        return self.__partial.has_value

    @property
    def parsed(self) -> PARTIAL_TYPE:
        assert self.is_parseable, "No parsed value."
        return self.__partial.value

    def json(self) -> Dict[str, Any]:
        return {
            "delta": self.delta,
            "parsed": self.__partial.json(),
        }


class BAMLStreamResponse(Generic[TYPE, PARTIAL_TYPE]):
    __partial_value: ValueWrapper[PartialValueWrapper[PARTIAL_TYPE]]
    __final_value: ValueWrapper[TYPE]

    def __init__(
        self,
        response: ValueWrapper[TYPE],
        partial_value: ValueWrapper[PartialValueWrapper[PARTIAL_TYPE]],
    ) -> None:
        self.__partial_value = partial_value
        self.__final_value = response

    @staticmethod
    def from_parsed_partial(
        partial: PARTIAL_TYPE, delta: str
    ) -> "BAMLStreamResponse[TYPE, PARTIAL_TYPE]":
        return BAMLStreamResponse[TYPE, PARTIAL_TYPE](
            ValueWrapper.unset(),
            ValueWrapper.from_value(PartialValueWrapper.from_parseable(partial, delta)),
        )

    @staticmethod
    def from_failed_partial(delta: str) -> "BAMLStreamResponse[TYPE, PARTIAL_TYPE]":
        return BAMLStreamResponse[TYPE, PARTIAL_TYPE](
            ValueWrapper.unset(),
            ValueWrapper.from_value(PartialValueWrapper.from_parse_failure(delta)),
        )

    @staticmethod
    def from_final_response(response: TYPE) -> "BAMLStreamResponse[TYPE, PARTIAL_TYPE]":
        return BAMLStreamResponse[TYPE, PARTIAL_TYPE](
            ValueWrapper.from_value(response), ValueWrapper.unset()
        )

    @property
    def is_complete(self) -> bool:
        return self.__final_value.has_value

    @property
    def final_response(self) -> TYPE:
        if not self.is_complete:
            raise ValueError("Stream not yet complete")
        return self.__final_value.value

    @property
    def has_partial_value(self) -> bool:
        return self.__partial_value.has_value

    @property
    def partial(self) -> PartialValueWrapper[PARTIAL_TYPE]:
        if not self.has_partial_value:
            raise ValueError("No partial value")
        return self.__partial_value.value

    def dump_json(self, **kwargs) -> str:
        return json.dumps(self.json(), **kwargs)

    def json(self) -> Dict[str, Any]:
        if self.has_partial_value:
            return {
                "partial": self.partial.json(),
                "final_response": self.__final_value.json(),
            }

        return {
            "partial": None,
            "final_response": self.__final_value.json(),
        }


class TextDelta(BaseModel):
    delta: str


class AsyncBAMLStream(Generic[TYPE, PARTIAL_TYPE]):
    __stream: AsyncIterator[LLMResponse]
    __final_response: ValueWrapper[TYPE]

    def __init__(
        self,
        stream: AsyncIterator[LLMResponse],
        partial_deserializer: Deserializer[PARTIAL_TYPE],
        final_deserializer: Deserializer[TYPE],
    ):
        self.__stream = stream
        self.__partial_deserializer = partial_deserializer
        self.__deserializer = final_deserializer
        self.__final_response = ValueWrapper.unset()

    async def __aenter__(self):
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        await self.until_done()

    @property
    async def text_stream(self) -> AsyncIterator[TextDelta]:
        async for response in self.__stream:
            yield TextDelta(delta=response.generated)

    @property
    async def parsed_stream(self) -> AsyncIterator[PartialValueWrapper[PARTIAL_TYPE]]:
        total_text = ""
        if self.__final_response.has_value:
            return
        async for response in self.__stream:
            try:
                total_text += response.generated
                parsed_json = partial_parser.parse(total_text)
                parsed = self.__partial_deserializer.from_string(parsed_json)
                yield PartialValueWrapper.from_parseable(
                    partial=parsed, delta=response.generated
                )
            except Exception as e:
                yield PartialValueWrapper.from_parse_failure(delta=response.generated)
        try:
            self.__final_response = ValueWrapper.from_value(
                self.__deserializer.from_string(total_text)
            )
        except Exception as e:
            self.__final_response = ValueWrapper.unset()

    async def get_final_response(self) -> ValueWrapper[TYPE]:
        await self.until_done()
        return self.__final_response

    async def until_done(self) -> None:
        if self.__final_response.has_value:
            return
        async for r in self.parsed_stream:
            pass

    # async def __aiter__(self) -> AsyncIterator[BAMLStreamResponse[TYPE, PARTIAL_TYPE]]:
    #     total_text = ""
    #     async for response in self.stream:
    #         try:
    #             total_text += response.generated
    #             parsed = self.__partial_deserializer.from_string(total_text)
    #             yield BAMLStreamResponse.from_parsed_partial(
    #                 partial=parsed, delta=response.generated
    #             )
    #         except Exception as e:
    #             yield BAMLStreamResponse.from_failed_partial(delta=response.generated)

    #     final_response = self.__deserializer.from_string(total_text)
    #     yield BAMLStreamResponse.from_final_response(response=final_response)

    # async def __anext__(self) -> BAMLStreamResponse[TYPE, PARTIAL_TYPE]:
    #     return await self.__aiter__().__anext__()

    # async def __await__(self) -> BAMLStreamResponse[TYPE, PARTIAL_TYPE]:
    #     return await self.__anext__()


# class AsyncBAMLStreamManager(Generic[TYPE, PARTIAL_TYPE]):
#     def __init__(
#         self,
#         stream_creation_coroutine: typing.Awaitable[
#             AsyncBAMLStream[TYPE, PARTIAL_TYPE]
#         ],
#     ):
#         self.__stream_creation_coroutine = stream_creation_coroutine
#         self.__stream: AsyncBAMLStream[TYPE, PARTIAL_TYPE] | None = None

#     async def __aenter__(self) -> AsyncBAMLStream[TYPE, PARTIAL_TYPE]:
#         self.__stream = await self.__stream_creation_coroutine
#         return self.__stream

#     async def __aexit__(self, exc_type, exc_val, exc_tb):
#         pass
#         # if self.__stream is not None:
#         #     await self.__stream.close()  # Ensure proper closure of the stream
