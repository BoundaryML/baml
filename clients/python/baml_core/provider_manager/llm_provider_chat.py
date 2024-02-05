import abc
import json
import traceback
import typing
import aiohttp
from pydantic import BaseModel, Field
from typeguard import typechecked

from ..configs.retry_policy import WrappedFn

from ..errors.llm_exc import LLMException, ProviderErrorCode

from ..cache_manager import CacheManager
from ..services.api_types import CacheRequest, LLMChat
from ..otel.helper import try_serialize
from ..otel.provider import create_event
from .llm_provider_base import (
    AbstractLLMProvider,
    LLMResponse,
    LLMChatMessage,
    _update_template_with_vars,
)


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
    async def _run_prompt_template_internal(
        self,
        *,
        template: str,
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> LLMResponse:
        return await self._run_chat_template_internal(
            [self.__prompt_to_chat(template)], replacers=replacers, params=params
        )

    @typing.final
    @typechecked
    async def _run_prompt_template_internal_stream(
        self,
        *,
        template: str,
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> typing.AsyncIterator[LLMResponse]:
        async for r in self._run_chat_template_internal_stream(
            [self.__prompt_to_chat(template)],
            replacers=replacers,
            params=params,
        ):
            yield r

    @typing.final
    @typechecked
    async def _run_chat_template_internal(
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

        create_event(
            "llm_prompt_template",
            {
                "chat_prompt": list(map(lambda x: json.dumps(x), chats)),
                "provider": self.provider,
                "template_vars": json.dumps(updates),
            },
        )

        if cached := self._check_cache(
            prompt=chats, prompt_vars={k: v for k, v in updates.items()}
        ):
            return cached

        # Before we run the chat, we need to update the chat messages.
        messages: typing.List[LLMChatMessage] = [
            {
                "role": msg["role"],
                "content": _update_template_with_vars(
                    template=msg["content"], updates=updates
                ),
            }
            for msg in chats
        ]

        try:
            return await self.__run_chat_with_telemetry(messages)
        except Exception as e:
            self._raise_error(e)

    @typing.final
    @typechecked
    async def _run_chat_template_internal_stream(
        self,
        *message_templates: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]],
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> typing.AsyncIterator[LLMResponse]:
        updates = {k: k.format(**params) for k in replacers}
        if len(message_templates) == 1 and isinstance(message_templates[0], list):
            chats = message_templates[0]
        else:
            chats = typing.cast(typing.List[LLMChatMessage], message_templates)

        create_event(
            "llm_prompt_template",
            {
                "chat_prompt": list(map(lambda x: json.dumps(x), chats)),
                "provider": self.provider,
                "template_vars": json.dumps(updates),
            },
        )

        if cached := self._check_cache(
            prompt=chats, prompt_vars={k: v for k, v in updates.items()}
        ):
            yield cached
            return

        # Before we run the chat, we need to update the chat messages.
        messages: typing.List[LLMChatMessage] = [
            {
                "role": msg["role"],
                "content": _update_template_with_vars(
                    template=msg["content"], updates=updates
                ),
            }
            for msg in chats
        ]

        async for response in self._stream_chat(messages):
            yield response

    @typechecked
    async def _run_prompt_internal(self, prompt: str) -> LLMResponse:
        return await self._run_chat_internal([self.__prompt_to_chat(prompt)])

    @typechecked
    async def _run_chat_internal(
        self, *messages: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]]
    ) -> LLMResponse:
        if len(messages) == 1 and isinstance(messages[0], list):
            chat_message = messages[0]
        else:
            chat_message = typing.cast(typing.List[LLMChatMessage], messages)

        if cached := self._check_cache(prompt=chat_message, prompt_vars={}):
            return cached

        try:
            return await self.__run_chat_with_telemetry(chat_message)
        except Exception as e:
            self._raise_error(e)

    @typing.final
    async def __run_chat_with_telemetry(
        self, messages: typing.List[LLMChatMessage]
    ) -> LLMResponse:
        self._start_run(messages)
        response = await self._run_chat(messages)
        self._end_run(response)
        return response

    # Implemented by the actual providers that extend this
    @abc.abstractmethod
    async def _run_chat(self, messages: typing.List[LLMChatMessage]) -> LLMResponse:
        raise NotImplementedError

    @abc.abstractmethod
    async def _stream_chat(
        self, messages: typing.List[LLMChatMessage]
    ) -> typing.AsyncIterator[LLMResponse]:
        raise NotImplementedError
        yield  # To appease typechecker. It thinks it's not a generator function unless it has a yield.
