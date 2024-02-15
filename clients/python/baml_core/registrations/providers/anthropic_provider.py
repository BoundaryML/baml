import anthropic
import typing
from anthropic.types import (
    MessageStartEvent,
    MessageStreamEvent,
    MessageDeltaEvent,
    ContentBlockStartEvent,
    ContentBlockDeltaEvent,
    MessageParam,
)

from baml_core.provider_manager import (
    LLMChatProvider,
    register_llm_provider,
    LLMResponse,
    LLMChatMessage,
)


def _hydrate_anthropic_tokenizer() -> None:
    # Anthropic's tokenizer is a bit slow to load, so we do it here
    # to avoid the first call to run_prompt being slow.
    # Calling this multiple times is fine, as it's a no-op if the
    # tokenizer is already loaded.
    anthropic.Client().get_tokenizer()


@register_llm_provider("baml-anthropic")
@typing.final
class AnthropicProvider(LLMChatProvider):
    __kwargs: typing.Dict[str, typing.Any]

    def _to_error_code(self, e: Exception) -> typing.Optional[int]:
        if isinstance(e, anthropic.APIStatusError):
            return e.status_code
        if isinstance(e, anthropic.APIResponseValidationError):
            return e.status_code
        if isinstance(e, anthropic.APIConnectionError):
            return 500
        if isinstance(e, anthropic.APITimeoutError):
            return 503
        if isinstance(e, anthropic.APIError):
            return 1
        # This is a catch-all for any other exception types that
        return None

    def __init__(
        self, *, options: typing.Dict[str, typing.Any], **kwargs: typing.Any
    ) -> None:
        _hydrate_anthropic_tokenizer()

        if "max_retries" in options and "retry" in kwargs:
            assert False, "Either use max_retries with Anthropic via options or retry via BAML, not both"

        super().__init__(
            prompt_to_chat=lambda chat: {"role": "user", "content": chat},
            **kwargs,
        )

        if "max_tokens_to_sample" not in options:
            # Anthropic requires a max_tokens_to_sample arg
            # We
            options["max_tokens_to_sample"] = 1000

        client_arg_names = [
            "api_key",
            "auth_token",
            "base_url",
            "timeout",
            "default_headers",
            "default_query",
            "transport",
            "proxies",
            "connection_pool_limits",
            "_strict_response_validation",
            "max_retries",
        ]
        # add temperature 0 to options if not present
        if "temperature" not in options:
            options["temperature"] = 0
        client_kwargs = {k: options.pop(k) for k in client_arg_names if k in options}

        # Default to 0 retries if not specified.
        if "max_retries" not in client_kwargs:
            client_kwargs["max_retries"] = 0

        self.__client = anthropic.AsyncAnthropic(**client_kwargs)
        self.__client_kwargs = client_kwargs
        self.__caller_kwargs = options
        self._set_args(**self.__caller_kwargs, **self.__client_kwargs)

    def _validate(self) -> None:
        pass

    async def _run_chat(self, messages: typing.List[LLMChatMessage]) -> LLMResponse:
        prompt = (
            "".join(
                map(
                    lambda c: f'{anthropic.HUMAN_PROMPT if  c["role"] != "system" else anthropic.AI_PROMPT} {c["content"]}',
                    messages,
                )
            )
            + anthropic.AI_PROMPT
        )
        prompt_tokens = await self.__client.count_tokens(prompt)
        response = typing.cast(
            anthropic.types.Completion,
            await self.__client.completions.create(
                prompt=prompt, **self.__caller_kwargs
            ),
        )
        output_tokens = await self.__client.count_tokens(response.completion)

        return LLMResponse(
            generated=response.completion,
            model_name=response.model,
            meta=dict(
                baml_is_complete=response.stop_reason == "stop_sequence"
                or response.stop_reason == "stop_sequences",
                prompt_tokens=prompt_tokens,
                output_tokens=output_tokens,
                total_tokens=prompt_tokens + output_tokens,
                finish_reason=response.stop_reason,
            ),
        )

    async def _stream_chat(
        self, messages: typing.List[LLMChatMessage]
    ) -> typing.AsyncIterator[LLMResponse]:
        # messages client has diff params
        caller_kwargs_copy = self.__caller_kwargs.copy()
        if "max_tokens" not in caller_kwargs_copy:
            caller_kwargs_copy["max_tokens"] = caller_kwargs_copy.pop(
                "max_tokens_to_sample", None
            )
        else:
            caller_kwargs_copy.pop("max_tokens_to_sample", None)

        def to_anthropic_message(msg: LLMChatMessage) -> MessageParam:
            return {
                "role": "user" if msg["role"] == "user" else "assistant",
                "content": msg["content"],
            }

        total_input_tokens = 0
        # cumulative token count
        total_output_tokens = 0
        model = None
        finish_reason = None
        async with self.__client.messages.stream(
            messages=list(map(to_anthropic_message, messages)), **caller_kwargs_copy
        ) as stream:
            last_response: typing.Optional[MessageStreamEvent] = None
            async for response in stream:
                last_response = response
                if isinstance(response, MessageStartEvent):
                    total_input_tokens = response.message.usage.input_tokens
                    model = response.message.model
                elif isinstance(response, MessageDeltaEvent):
                    total_output_tokens = response.usage.output_tokens
                    finish_reason = response.delta.stop_reason
                elif isinstance(response, ContentBlockStartEvent):
                    yield LLMResponse(
                        generated=response.content_block.text,
                        model_name=model or "<unknown-stream-model>",
                        meta={},
                    )
                elif isinstance(response, ContentBlockDeltaEvent):
                    yield LLMResponse(
                        generated=response.delta.text,
                        model_name=model or "<unknown-stream-model>",
                        meta={},
                    )

            # Send final delta with cumulative token counts
            if last_response is not None:
                yield LLMResponse(
                    generated="",
                    model_name="",
                    meta=dict(
                        baml_is_complete=finish_reason is not None
                        and finish_reason != "max_tokens",
                        prompt_tokens=total_input_tokens,
                        output_tokens=total_output_tokens,
                        total_tokens=total_input_tokens + total_output_tokens,
                        finish_reason=finish_reason,
                        stream=True,
                    ),
                )
