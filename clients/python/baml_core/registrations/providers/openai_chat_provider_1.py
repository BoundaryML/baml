from openai import AsyncOpenAI, AsyncAzureOpenAI, AsyncClient
from openai.types.chat.chat_completion import ChatCompletion
from .openai_helper_1 import to_error_code

import typing


from baml_core.provider_manager import (
    LLMChatMessage,
    LLMChatProvider,
    LLMResponse,
    register_llm_provider,
)


@register_llm_provider("baml-openai-chat", "baml-azure-chat")
@typing.final
class OpenAIChatProvider(LLMChatProvider):
    __kwargs: typing.Dict[str, typing.Any]
    _client: AsyncClient

    def __init__(
        self, *, options: typing.Dict[str, typing.Any], **kwargs: typing.Any
    ) -> None:
        default_chat_role = kwargs.pop("default_chat_role", "user")
        assert isinstance(
            default_chat_role, str
        ), f"default_chat_role must be a string: {type(default_chat_role)}. {default_chat_role}"
        if "temperature" not in options:
            options["temperature"] = 0
        super().__init__(
            prompt_to_chat=lambda prompt: LLMChatMessage(
                role=default_chat_role, content=prompt
            ),
            **kwargs,
        )

        if options.get("api_type") == "azure" or options.get("azure_endpoint"):
            # We still need to map from the 0.x API to the 1.x API. People may use either of these:
            # api_key / api_key
            # api_version / api_version
            # api_base / azure_endpoint

            self._client = AsyncAzureOpenAI(
                api_key=options["api_key"],
                api_version=options["api_version"],
                azure_endpoint=options["api_base"] or options["azure_endpoint"],
            )
        else:
            self._client = AsyncOpenAI(api_key=options["api_key"])
        options.pop("api_key", None)
        options.pop("api_version", None)
        options.pop("api_base", None)
        options.pop("azure_endpoint", None)
        timeout = options.get("timeout") or options.get("request_timeout") or None
        if options.pop("request_timeout", None) is not None:
            self._client.timeout = timeout
        options["model"] = options.get("model", None) or options.get("engine")

        self.__kwargs = options
        self._set_args(**self.__kwargs)

    def _to_error_code(self, e: Exception) -> typing.Optional[int]:
        return to_error_code(e)

    def _validate(self) -> None:
        pass

    # def _to_chat_completion_messages(self, messages: typing.List[LLMChatMessage]) -> ChatComple

    async def _run_chat(self, messages: typing.List[LLMChatMessage]) -> LLMResponse:
        response: ChatCompletion = await self._client.chat.completions.create(
            messages=messages, **self.__kwargs  # type: ignore
        )
        if not isinstance(response, ChatCompletion):
            raise ValueError(
                f"Invalid response type returned from LLM provider: {self.provider}"
            )

        text = response.choices[0].message.content
        usage = response.usage
        model = response.model
        finish_reason = response.choices[0].finish_reason

        if not isinstance(text, str):
            raise ValueError(
                f"No content string returned from LLM provider: {self.provider}"
            )

        prompt_tokens = usage.prompt_tokens if usage else None
        output_tokens = usage.completion_tokens if usage else None
        total_tokens = usage.total_tokens if usage else None

        return LLMResponse(
            generated=text,
            model_name=model,
            meta=dict(
                baml_is_complete=finish_reason == "stop",
                logprobs=None,
                prompt_tokens=prompt_tokens,
                output_tokens=output_tokens,
                total_tokens=total_tokens,
                finish_reason=finish_reason,
            ),
        )

    async def _stream_chat(
        self, messages: typing.List[LLMChatMessage]
    ) -> typing.AsyncIterator[LLMResponse]:

        response = await self._client.chat.completions.create(
            messages=messages,  # type: ignore
            **self.__kwargs,
            stream=True,
        )

        async for r in response:  # type: ignore
            prompt_tokens = None
            output_tokens = None
            total_tokens = None
            # Note, openai currently does not provide usages for streams.
            yield LLMResponse(
                generated=r.choices[0].delta.content or "",
                model_name=r.model if r.model else "unknown-model",
                meta=dict(
                    baml_is_complete=r.choices[0].finish_reason == "stop",
                    logprobs=None,
                    prompt_tokens=prompt_tokens,
                    output_tokens=output_tokens,
                    total_tokens=total_tokens,
                    finish_reason=r.choices[0].finish_reason if r.choices else None,
                    stream=True,
                ),
            )
