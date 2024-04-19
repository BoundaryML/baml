from __future__ import annotations
import abc
import json
import os
import typing
from baml_core_ffi import TemplateStringMacro
from typeguard import typechecked

from baml_core.jinja.render_prompt import render_prompt, RenderData


from ..errors.llm_exc import LLMException, ProviderErrorCode

from ..otel.provider import create_event
from .llm_response import LLMResponse
from .llm_provider_base import (
    AbstractLLMProvider,
    LLMChatMessage,
    update_template_with_vars,
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
                "content": update_template_with_vars(
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
    async def _run_jinja_template_internal(
        self,
        *,
        jinja_template: str,
        args: typing.Dict[str, typing.Any],
        output_format: str,
        template_macros: typing.List[TemplateStringMacro],
    ) -> LLMResponse:
        rendered = render_prompt(
            jinja_template,
            RenderData(
                args=args,
                ctx=RenderData.ctx(
                    client=self.client,
                    output_format=output_format,
                    env=os.environ.copy(),
                ),
                template_string_macros=template_macros,
            ),
        )

        if rendered[0] == "chat":
            return await self._run_chat_internal(
                [
                    LLMChatMessage(role=chat.role, content=chat.message)
                    for chat in rendered[1]
                ]
            )
        else:
            return await self._run_prompt_internal(rendered[1])

    @typing.final
    async def _run_jinja_template_internal_stream(
        self,
        *,
        jinja_template: str,
        args: typing.Dict[str, typing.Any],
        output_format: str,
        template_macros: typing.List[TemplateStringMacro],
    ) -> typing.AsyncIterator[LLMResponse]:
        prompt = render_prompt(
            jinja_template,
            RenderData(
                args=args,
                ctx=RenderData.ctx(
                    client=self.client,
                    output_format=output_format,
                    env=os.environ.copy(),
                ),
                template_string_macros=template_macros,
            ),
        )

        if prompt[0] == "chat":
            async for r in self._run_chat_internal_stream(
                list(
                    map(
                        lambda x: LLMChatMessage(role=x.role, content=x.message),
                        prompt[1],
                    )
                )
            ):
                yield r
        else:
            async for r in self._run_prompt_internal_stream(prompt=prompt[1]):
                yield r

    @typing.final
    async def _run_chat_internal_stream(
        self, *messages: LLMChatMessage | typing.List[LLMChatMessage]
    ) -> typing.AsyncIterator[LLMResponse]:
        if len(messages) == 1 and isinstance(messages[0], list):
            chat_message = messages[0]
        else:
            chat_message = typing.cast(typing.List[LLMChatMessage], messages)
        async for response in self.__run_chat_stream_with_telemetry(chat_message):
            yield response

    @typing.final
    async def _run_prompt_internal_stream(
        self, *, prompt: str
    ) -> typing.AsyncIterator[LLMResponse]:
        async for response in self._run_chat_internal_stream(
            self.__prompt_to_chat(prompt)
        ):
            yield response

    @typing.final
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
                "content": update_template_with_vars(
                    template=msg["content"], updates=updates
                ),
            }
            for msg in chats
        ]

        async for response in self.__run_chat_stream_with_telemetry(messages):
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

    # Accumulates all messages and sends final payload
    # to the backend for tracing purposes
    async def __run_chat_stream_with_telemetry(
        self, messages: typing.List[LLMChatMessage]
    ) -> typing.AsyncIterator[LLMResponse]:
        self._start_run(messages)
        last_response: typing.Optional[LLMResponse] = None
        total_text = ""

        async for response in self._stream_chat(messages):
            if isinstance(response, NotImplementedError):
                print(
                    "Streaming not implemented for {}. Falling back to non-streaming API".format(
                        self.provider
                    )
                )
                # if this is also not implemented we will error out
                response = await self._run_chat(messages)
            yield response
            total_text += response.generated
            last_response = response
        if last_response is not None:
            self._end_run(
                LLMResponse(
                    generated=total_text,
                    model_name=last_response.mdl_name,
                    meta=last_response.meta,
                )
            )
        else:
            raise LLMException(
                code=ProviderErrorCode.INTERNAL_ERROR,
                message="No response from provider stream",
            )

    # Implemented by the actual providers that extend this
    @abc.abstractmethod
    async def _run_chat(self, messages: typing.List[LLMChatMessage]) -> LLMResponse:
        raise NotImplementedError()

    async def _stream_chat(
        self, messages: typing.List[LLMChatMessage]
    ) -> typing.AsyncIterator[LLMResponse]:
        yield await self._run_chat(messages)
        # yield  # To appease typechecker. It thinks it's not a generator function unless it has a yield.
