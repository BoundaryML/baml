from openai import AsyncOpenAI, AsyncAzureOpenAI, AsyncClient
from openai.types.chat.chat_completion import ChatCompletion
from openai.types.chat.chat_completion_message_param import ChatCompletionMessageParam
from .openai_helper_1 import to_error_code

import typing

from baml_core.provider_manager import (
    LLMChatMessage,
    LLMChatProvider,
    LLMResponse,
    register_llm_provider,
)


def _to_chat_completion_messages(msg: LLMChatMessage) -> ChatCompletionMessageParam:
    if msg["role"] == "user":
        return {"role": "user", "content": msg["content"]}
    if msg["role"] == "assistant":
        return {"role": "assistant", "content": msg["content"]}
    # Default to system messages
    return {"role": "system", "content": msg["content"]}


@register_llm_provider("baml-openai-chat", "baml-azure-chat")
@typing.final
class OpenAIChatProvider(LLMChatProvider):
    __request_args: typing.Dict[str, typing.Any]
    __client_args: typing.Dict[str, typing.Any]

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
        self.__client_args = {
            "timeout": options.pop("request_timeout", options.pop("timeout", 60 * 3)),
            "api_key": options["api_key"],
        }

        if options.pop("max_retries", None) is not None:
            raise ValueError("Use a BAML RetryPolicy instead of passing in max_retries")

        if options.get("api_type") == "azure" or options.get("azure_endpoint"):
            self.__client_args.update(
                api_version=options["api_version"],
                azure_endpoint=options.get("api_base") or options["azure_endpoint"],
            )

        # Build up the actual request args, which don't include any of these.
        options.pop("api_key", None)
        options.pop("api_version", None)
        options.pop("api_base", None)
        options.pop("api_type", None)
        options.pop("azure_endpoint", None)

        options["model"] = (
            options.get("model", None)
            or options.pop("engine", None)
            or options.pop("deployment_name", None)
        )

        self.__request_args = options
        self._set_args(**self.__request_args)

    def __create_client(self) -> AsyncClient:
        options = self.__client_args.copy()
        timeout = options.get("timeout", 60 * 3)
        if options.get("api_type") == "azure" or options.get("azure_endpoint"):
            # We still need to map from the 0.x API to the 1.x API. People may use either of these:
            # api_key / api_key
            # api_version / api_version
            # api_base / azure_endpoint
            return AsyncAzureOpenAI(
                api_key=options["api_key"],
                api_version=options.get("api_version"),
                azure_endpoint=options.get("api_base") or options["azure_endpoint"],
                timeout=timeout,
            )
        else:
            return AsyncOpenAI(api_key=options["api_key"], timeout=timeout)

    def _to_error_code(self, e: Exception) -> typing.Optional[int]:
        return to_error_code(e)

    def _validate(self) -> None:
        pass

    async def _run_chat(self, messages: typing.List[LLMChatMessage]) -> LLMResponse:
        # Instantiate the client everytime to prevent event-loop errors
        client = self.__create_client()
        response: ChatCompletion = await client.chat.completions.create(
            messages=list(map(_to_chat_completion_messages, messages)),
            **self.__request_args,
        )
        assert isinstance(
            response, ChatCompletion
        ), f"Invalid response type returned from LLM provider: {self.provider}"

        if not response.choices:
            raise ValueError(
                f"No completion choice returned from LLM provider {self.provider}"
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
        client = self.__create_client()
        response = await client.chat.completions.create(
            messages=list(map(_to_chat_completion_messages, messages)),
            **self.__request_args,
            stream=True,
        )

        async for r in response:
            prompt_tokens = None
            output_tokens = None
            total_tokens = None
            if not r.choices:
                continue
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
                ),
            )
