import anthropic
import typing
from packaging.version import parse as parse_version

if parse_version(version=anthropic.__version__) < parse_version("0.16.0"):
    from anthropic.types.beta import (  # type: ignore
        MessageStartEvent,
        MessageStreamEvent,
        MessageDeltaEvent,
        ContentBlockStartEvent,
        ContentBlockDeltaEvent,
        MessageParam,
    )
else:
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


@register_llm_provider("baml-anthropic", "baml-anthropic-completion")
@typing.final
class AnthropicProvider(LLMChatProvider):
    __caller_kwargs: typing.Dict[str, typing.Any]
    __client_kwargs: typing.Dict[str, typing.Any]

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
        assert not (
            "max_retries" in options and "retry" in kwargs
        ), "Either use max_retries with Anthropic via options or retry via BAML, not both"

        super().__init__(
            prompt_to_chat=lambda chat: {"role": "user", "content": chat},
            **kwargs,
        )

        self._ensure_option_defaults(options, "max_tokens_to_sample", 1000)
        self._ensure_option_defaults(options, "temperature", 0)

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
        self._ensure_option_defaults(options, "max_retries", 0)
        client_kwargs = {k: options.pop(k) for k in client_arg_names if k in options}

        self.__client_kwargs = client_kwargs
        self.__caller_kwargs = options
        self._set_args(**self.__caller_kwargs, **self.__client_kwargs)

    def _ensure_option_defaults(
        self, options: typing.Dict[str, typing.Any], key: str, default_value: typing.Any
    ) -> None:
        if key not in options:
            options[key] = default_value

    def __create_client(self) -> anthropic.AsyncAnthropic:
        return anthropic.AsyncAnthropic(**self.__client_kwargs)

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
        client = self.__create_client()
        prompt_tokens = await client.count_tokens(prompt)
        response = typing.cast(
            anthropic.types.Completion,
            await client.completions.create(prompt=prompt, **self.__caller_kwargs),
        )
        output_tokens = await client.count_tokens(response.completion)

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
        # Grab the message that has "system" in it and pop it from the list
        system_message: typing.Optional[str] = None
        system_messages = [m for m in messages if m.get("role") == "system"]
        if len(system_messages) > 1:
            raise ValueError("Only one system message is allowed")
        elif len(system_messages) == 1:
            system_chat_msg = system_messages[0]
            messages.remove(system_chat_msg)
            system_message = system_chat_msg.get("content")

        # messages client has diff params
        caller_kwargs_copy = self.__caller_kwargs.copy()
        if "max_tokens" not in caller_kwargs_copy:
            caller_kwargs_copy["max_tokens"] = caller_kwargs_copy.pop(
                "max_tokens_to_sample", None
            )
        else:
            caller_kwargs_copy.pop("max_tokens_to_sample", None)

        def to_anthropic_message(msg: LLMChatMessage) -> MessageParam:  # type: ignore
            return {
                "role": "user" if msg["role"] == "user" else "assistant",
                "content": msg["content"],
            }

        total_input_tokens = 0
        # cumulative token count
        total_output_tokens = 0
        model = None
        finish_reason = None
        messages_api = None

        messages_kwargs: typing.Dict[str, typing.Any] = {
            "messages": list(map(to_anthropic_message, messages)),
            **caller_kwargs_copy,
        }

        if system_message:
            messages_kwargs["system"] = system_message

        client = self.__create_client()

        if parse_version(version=anthropic.__version__) < parse_version("0.16.0"):
            messages_api = client.beta.messages  # type: ignore
        else:
            messages_api = client.messages
        async with messages_api.stream(
            **messages_kwargs,
        ) as stream:
            last_response: typing.Optional[MessageStreamEvent] = None  # type: ignore
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
