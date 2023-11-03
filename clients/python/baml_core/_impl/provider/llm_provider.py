import abc
from enum import Enum
import json
import traceback
import typing
import aiohttp
from pydantic import BaseModel, Field
from typeguard import typechecked

from ...otel.provider import try_serialize
from ...otel import create_event


class LLMResponse(BaseModel):
    generated: str
    mdl_name: str = Field(alias="model_name")
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
        formatted_traceback = "".join(
            traceback.format_exception(type(e), e, e.__traceback__)
        )
        create_event(
            "llm_request_error",
            {
                "traceback": formatted_traceback,
                "message": f"{type(e).__name__}: {e}",
                "code": self._to_error_code(e) or -1,
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

    @abc.abstractmethod
    async def run_prompt_template(
        self,
        *,
        template: str,
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> LLMResponse:
        pass

    @abc.abstractmethod
    async def run_chat_template(
        self,
        *message_templates: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]],
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> LLMResponse:
        pass

    @typing.final
    def _start_run(
        self, prompt: typing.Union[str, typing.List[LLMChatMessage]]
    ) -> None:
        if isinstance(prompt, str):
            create_event(
                "llm_request_start",
                {"prompt": prompt, "provider": self.provider},
            )
        else:
            create_event(
                "llm_request_start",
                {
                    "chat_prompt": list(map(lambda x: json.dumps(x), prompt)),
                    "provider": self.provider,
                },
            )

    @typing.final
    def _end_run(self, response: LLMResponse) -> None:
        create_event(
            "llm_request_end",
            {
                "generated": response.generated,
                "model_name": response.mdl_name,
                "meta": json.dumps(response.meta),
            },
        )

    @typing.final
    def _log_args(self, **kwargs: typing.Any) -> None:
        create_event(
            "llm_request_args", {k: try_serialize(v)[0] for k, v in kwargs.items()}
        )

    @typing.final
    async def _run_prompt_template(
        self,
        *,
        template: str,
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> LLMResponse:
        updates = {k: k.format(**params) for k in replacers}
        prompt = self.__prompt_from_template(template=template, updates=updates)
        create_event(
            "llm_prompt_template",
            {
                "prompt": template,
                "provider": self.provider,
                "template_vars": json.dumps(updates),
            },
        )
        return await self.run_prompt(prompt)

    @typing.final
    def __prompt_from_template(
        self, *, template: str, updates: typing.Mapping[str, str]
    ) -> str:
        prompt = str(template)
        for k, v in updates.items():
            prompt = prompt.replace(k, v)
        return prompt

    @typing.final
    async def _run_chat_template(
        self,
        *message_templates: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]],
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> LLMResponse:
        updates = {k: k.format(**params) for k in replacers}
        if len(message_templates) == 1 and isinstance(message_templates[0], list):
            chats = message_templates[0]
        else:
            chats = typing.cast(typing.List[LLMChatMessage], message_templates)
        messages: typing.List[LLMChatMessage] = [
            {
                "role": msg["role"],
                "content": self.__prompt_from_template(
                    template=msg["content"], updates=updates
                ),
            }
            for msg in chats
        ]
        create_event(
            "llm_prompt_template",
            {
                "chat_prompt": list(map(lambda x: json.dumps(x), chats)),
                "provider": self.provider,
                "template_vars": json.dumps(updates),
            },
        )
        return await self.run_chat(messages)


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
    async def run_prompt_template(
        self,
        *,
        template: str,
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> LLMResponse:
        return await self._run_prompt_template(
            template=template, replacers=replacers, params=params
        )

    @typing.final
    @typechecked
    async def run_chat_template(
        self,
        *message_templates: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]],
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> LLMResponse:
        if len(message_templates) == 1 and isinstance(message_templates[0], list):
            chats = message_templates[0]
        else:
            chats = typing.cast(typing.List[LLMChatMessage], message_templates)
        return await self._run_prompt_template(
            template=self.__chat_to_prompt(chats),
            replacers=replacers,
            params=params,
        )

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

    @typing.final
    @typechecked
    async def run_prompt_template(
        self,
        *,
        template: str,
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> LLMResponse:
        return await self._run_chat_template(
            [self.__prompt_to_chat(template)], replacers=replacers, params=params
        )

    @typing.final
    @typechecked
    async def run_chat_template(
        self,
        *message_templates: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]],
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> LLMResponse:
        return await self._run_chat_template(
            *message_templates, replacers=replacers, params=params
        )

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
