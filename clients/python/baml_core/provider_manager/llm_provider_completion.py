import abc
import json
import typing
from typeguard import typechecked
from ..otel.provider import create_event
from .llm_response import LLMResponse
from .llm_provider_base import (
    AbstractLLMProvider,
    LLMChatMessage,
    _update_template_with_vars,
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
    async def _run_prompt_template_internal(
        self,
        *,
        template: str,
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> LLMResponse:
        updates = {k: k.format(**params) for k in replacers}
        create_event(
            "llm_prompt_template",
            {
                "prompt": template,
                "provider": self.provider,
                "template_vars": json.dumps(updates),
            },
        )
        if cached := self._check_cache(prompt=template, prompt_vars=updates):
            return cached
        prompt = _update_template_with_vars(template=template, updates=updates)
        try:
            return await self.__run_with_telemetry(prompt)
        except Exception as e:
            self._raise_error(e)

    @typing.final
    @typechecked
    async def _run_prompt_template_internal_stream(
        self,
        *,
        template: str,
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> typing.AsyncIterator[LLMResponse]:
        updates = {k: k.format(**params) for k in replacers}
        create_event(
            "llm_prompt_template",
            {
                "prompt": template,
                "provider": self.provider,
                "template_vars": json.dumps(updates),
            },
        )
        if cached := self._check_cache(prompt=template, prompt_vars=updates):
            yield cached
        prompt = _update_template_with_vars(template=template, updates=updates)
        try:
            async for r in self._run_stream(prompt):
                yield r
        except Exception as e:
            self._raise_error(e)

    @typing.final
    @typechecked
    async def _run_chat_template_internal(
        self,
        *message_templates: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]],
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> LLMResponse:
        if len(message_templates) == 1 and isinstance(message_templates[0], list):
            chats = message_templates[0]
        else:
            chats = typing.cast(typing.List[LLMChatMessage], message_templates)
        return await self._run_prompt_template_internal(
            template=self.__chat_to_prompt(chats),
            replacers=replacers,
            params=params,
        )

    @typing.final
    @typechecked
    async def _run_prompt_internal(self, prompt: str) -> LLMResponse:
        if cached := self._check_cache(prompt=prompt, prompt_vars={}):
            return cached

        try:
            return await self.__run_with_telemetry(prompt)
        except Exception as e:
            self._raise_error(e)

    @typing.final
    @typechecked
    async def _run_chat_internal(
        self, *messages: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]]
    ) -> LLMResponse:
        if len(messages) == 1 and isinstance(messages[0], list):
            chats = messages[0]
        else:
            chats = typing.cast(typing.List[LLMChatMessage], messages)
        return await self._run_prompt_internal(self.__chat_to_prompt(chats))

    @typing.final
    async def __run_with_telemetry(self, prompt: str) -> LLMResponse:
        self._start_run(prompt)
        response = await self._run(prompt)
        self._end_run(response)
        return response

    # Implemented by the actual providers that extend this
    @abc.abstractmethod
    async def _run(self, prompt: str) -> LLMResponse:
        raise NotImplementedError

    @abc.abstractmethod
    async def _run_stream(self, prompt: str) -> typing.AsyncIterator[LLMResponse]:
        raise NotImplementedError
        yield  # appease the linter that doesn't understand that this is an async generator unless theres a yield
