import abc
import typing
import aiohttp
from pydantic import BaseModel
from typeguard import typechecked


class LLMResponse(BaseModel):
    generated: str
    model_name: str
    meta: typing.Any


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
    def _to_error_code(self, e: BaseException) -> typing.Optional[int]:
        if isinstance(e, aiohttp.ClientError):
            return 500
        if isinstance(e, aiohttp.ClientResponseError):
            return e.status
        return None

    def _raise_error(self, e: BaseException) -> typing.NoReturn:
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
            return await self._run(prompt)
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
                return await self._run_chat(messages[0])
            else:
                return await self._run_chat(
                    typing.cast(typing.List[LLMChatMessage], messages)
                )
        except BaseException as e:
            self._raise_error(e)

    @abc.abstractmethod
    async def _run_chat(self, messages: typing.List[LLMChatMessage]) -> LLMResponse:
        raise NotImplementedError
