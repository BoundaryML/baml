from __future__ import annotations
import abc
import json
import os
import typing
from baml_core_ffi import RenderData, TemplateStringMacro, render_prompt
from typeguard import typechecked
from ..otel.provider import create_event
from .llm_response import LLMResponse
from .llm_provider_base import (
    AbstractLLMProvider,
    LLMChatMessage,
    update_template_with_vars,
)
from ..errors.llm_exc import LLMException, ProviderErrorCode


def default_chat_to_prompt(messages: typing.List[LLMChatMessage]) -> str:
    return "\n".join([msg["content"] for msg in messages if "content" in msg])


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
    async def _run_jinja_template_internal(
        self,
        *,
        jinja_template: str,
        args: typing.Dict[str, typing.Any],
        output_format: str,
        template_macros: typing.List[TemplateStringMacro],
    ) -> LLMResponse:
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
            return await self._run_chat_internal(
                list(
                    map(
                        lambda x: LLMChatMessage(role=x.role, content=x.message),
                        prompt[1],
                    )
                )
            )
        else:
            return await self._run_prompt_internal(prompt[1])

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
        prompt = update_template_with_vars(template=template, updates=updates)
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
        prompt = update_template_with_vars(template=template, updates=updates)
        try:
            async for r in self.__stream_with_telemetry(prompt):
                yield r
        except Exception as e:
            self._raise_error(e)

    @typing.final
    @typechecked
    async def _run_prompt_internal_stream(
        self,
        *,
        prompt: str,
    ) -> typing.AsyncIterator[LLMResponse]:
        try:
            async for r in self.__stream_with_telemetry(prompt):
                yield r
        except Exception as e:
            self._raise_error(e)

    @typing.final
    @typechecked
    async def _run_chat_internal_stream(
        self, *messages: LLMChatMessage | typing.List[LLMChatMessage]
    ) -> typing.AsyncIterator[LLMResponse]:
        if len(messages) == 1 and isinstance(messages[0], list):
            chats = messages[0]
        else:
            chats = typing.cast(typing.List[LLMChatMessage], messages)

        async for msg in self._run_prompt_internal_stream(
            prompt=self.__chat_to_prompt(chats)
        ):
            yield msg

    async def __stream_with_telemetry(
        self, prompt: str
    ) -> typing.AsyncIterator[LLMResponse]:
        self._start_run(prompt)
        last_response: typing.Optional[LLMResponse] = None
        total_text = ""

        async for response in self._stream(prompt):
            if isinstance(response, NotImplementedError):
                print(
                    "Streaming not implemented for {}. Falling back to non-streaming API".format(
                        self.provider
                    )
                )
                # if this is also not implemented we will error out
                response = await self._run(prompt)
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
    async def _run_chat_template_internal_stream(
        self,
        *message_templates: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]],
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> typing.AsyncIterator[LLMResponse]:
        if len(message_templates) == 1 and isinstance(message_templates[0], list):
            chats = message_templates[0]
        else:
            chats = typing.cast(typing.List[LLMChatMessage], message_templates)
        try:
            async for x in self._run_prompt_template_internal_stream(
                template=self.__chat_to_prompt(chats),
                replacers=replacers,
                params=params,
            ):
                yield x
        except Exception as e:
            self._raise_error(e)

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
        raise NotImplementedError()

    @abc.abstractmethod
    async def _stream(self, prompt: str) -> typing.AsyncIterator[LLMResponse]:
        raise NotImplementedError()
        yield  # appease the linter that doesn't understand that this is an async generator unless theres a yield
