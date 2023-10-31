import abc
from enum import Enum
import json
import typing
import aiohttp
from pydantic import BaseModel
from typeguard import typechecked

from baml_client.otel.provider import try_serialize
from ...otel import create_event


class LLMResponse(BaseModel):
    generated: str
    model_name: str
    meta: typing.Any


class ProviderErrorCode(int):
    INTERNAL_ERROR = 500
    BAD_REQUEST = 400
    UNAUTHORIZED = 401
    FORBIDDEN = 403
    NOT_FOUND = 404
    RATE_LIMITED = 429


class LLMException(BaseException):
    code: typing.Optional[int]
    message: str

    def __init__(self, *, code: typing.Optional[int] = None, message: str) -> None:
        self.code = code
        self.message = message
        super().__init__(message)

    def __str__(self) -> str:
        return f"LLM Failed: Code {self.code}: {self.message}"

    def __repr__(self) -> str:
        return f"LLMException(code={self.code!r}, message={self.message!r})"


class BaseProvider(abc.ABC):
    def _to_error_code(
        self, e: BaseException
    ) -> typing.Optional[typing.Union[ProviderErrorCode, int]]:
        if isinstance(e, aiohttp.ClientError):
            return ProviderErrorCode.INTERNAL_ERROR
        if isinstance(e, aiohttp.ClientResponseError):
            return e.status
        return None

    def _raise_error(self, e: BaseException) -> typing.NoReturn:
        create_event(
            "llm_request_error",
            {
                "error": str(e),
                "error_type": type(e).__name__,
                "error_code": self._to_error_code(e) or -1,
            },
        )
        if isinstance(e, LLMException):
            raise e
        code = self._to_error_code(e)
        if code is not None:
            raise LLMException(code=code, message=str(e))
        raise e


class LLMChatMessage(typing.TypedDict):
    role: str
    content: str


class AbstractLLMProvider(BaseProvider, abc.ABC):
    """
    Abstract base class to ensure both LLMProvider and LLMChatProvider
    have run_prompt and run_chat methods.
    """

    def __init__(self, provider: str, **kwargs: typing.Any) -> None:
        self.__provider = provider
        assert not kwargs, f"Unhandled provider settings: {', '.join(kwargs.keys())}"

    @property
    def provider(self) -> str:
        return self.__provider

    @abc.abstractmethod
    async def run_prompt(self, prompt: str) -> LLMResponse:
        pass

    @abc.abstractmethod
    async def run_chat(
        self, *messages: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]]
    ) -> LLMResponse:
        pass

    @typing.final
    def _start_run(
        self, prompt: typing.Union[str, typing.List[LLMChatMessage]]
    ) -> None:
        if isinstance(prompt, str):
            create_event(
                "llm_request_start",
                {"prompt": prompt},
            )
        else:
            create_event(
                "llm_request_start",
                {"chat_prompt": list(map(lambda x: json.dumps(x), prompt))},
            )

    @typing.final
    def _end_run(self, response: LLMResponse) -> None:
        create_event(
            "llm_request_end",
            {
                "generated": response.generated,
                "model_name": response.model_name,
                "meta": json.dumps(response.meta),
            },
        )

    @typing.final
    def _log_args(self, **kwargs: typing.Any) -> None:
        create_event(
            "llm_request_args", {k: try_serialize(v)[0] for k, v in kwargs.items()}
        )


def default_chat_to_prompt(messages: typing.List[LLMChatMessage]) -> str:
    return "\n".join(
        [
            msg["content"]
            for msg in messages
            if isinstance(msg, dict) and "content" in msg
        ]
    )


class LLMProvider(AbstractLLMProvider):
    @typechecked
    def __init__(
        self,
        *,
        chat_to_prompt: typing.Callable[
            [typing.List[LLMChatMessage]], str
        ] = default_chat_to_prompt,
        **kwargs: typing.Any,
    ) -> None:
        super().__init__(**kwargs)
        self.__chat_to_prompt = chat_to_prompt

    @typing.final
    @typechecked
    async def run_prompt(self, prompt: str) -> LLMResponse:
        try:
            return await self.__run(prompt)
        except BaseException as e:
            self._raise_error(e)

    @typing.final
    @typechecked
    async def run_chat(
        self, *messages: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]]
    ) -> LLMResponse:
        if len(messages) == 1 and isinstance(messages[0], list):
            chats = messages[0]
        else:
            chats = typing.cast(typing.List[LLMChatMessage], messages)
        return await self.run_prompt(self.__chat_to_prompt(chats))

    @typing.final
    async def __run(self, prompt: str) -> LLMResponse:
        self._start_run(prompt)
        response = await self._run(prompt)
        self._end_run(response)
        return response

    @abc.abstractmethod
    async def _run(self, prompt: str) -> LLMResponse:
        raise NotImplementedError


class LLMChatProvider(AbstractLLMProvider):
    @typechecked
    def __init__(
        self,
        *,
        prompt_to_chat: typing.Callable[[str], LLMChatMessage],
        **kwargs: typing.Any,
    ) -> None:
        super().__init__(**kwargs)
        self.__prompt_to_chat = prompt_to_chat

    @typechecked
    async def run_prompt(self, prompt: str) -> LLMResponse:
        return await self.run_chat([self.__prompt_to_chat(prompt)])

    @typechecked
    async def run_chat(
        self, *messages: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]]
    ) -> LLMResponse:
        try:
            if len(messages) == 1 and isinstance(messages[0], list):
                chat_message = messages[0]
                return await self.__run_chat(chat_message)
            else:
                return await self.__run_chat(
                    typing.cast(typing.List[LLMChatMessage], messages)
                )
        except BaseException as e:
            self._raise_error(e)

    @typing.final
    async def __run_chat(self, messages: typing.List[LLMChatMessage]) -> LLMResponse:
        self._start_run(messages)
        response = await self._run_chat(messages)
        self._end_run(response)
        return response

    @abc.abstractmethod
    async def _run_chat(self, messages: typing.List[LLMChatMessage]) -> LLMResponse:
        raise NotImplementedError
