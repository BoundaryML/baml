from __future__ import annotations
import typing
import anthropic

from .base_client import LLMClient
from .factory import register_llm_client
from .. import api_types


@register_llm_client("anthropic")
class AnthropicLLMClient(LLMClient):
    def __init__(self, provider: str, **kwargs: typing.Any) -> None:
        if "max_tokens_to_sample" not in kwargs:
            kwargs["max_tokens_to_sample"] = 300
        if "model" not in kwargs:
            assert False, "AnthropicLLMClient requires a model"

        if "max_retries" in kwargs and "__retry" in kwargs:
            assert False, "Cannot specify both max_retries and __retry"

        __retry = kwargs.pop("__retry", kwargs.pop("max_retries", 0))
        super().__init__(provider=provider, **kwargs, __retry=__retry)

        client_kwargs = {}
        if "api_key" in kwargs:
            client_kwargs["api_key"] = kwargs.pop("api_key")
        if "auth_token" in kwargs:
            client_kwargs["auth_token"] = kwargs.pop("auth_token")
        if "base_url" in kwargs:
            client_kwargs["base_url"] = kwargs.pop("base_url")
        if "timeout" in kwargs:
            client_kwargs["timeout"] = kwargs.pop("timeout")
        if "default_headers" in kwargs:
            client_kwargs["default_headers"] = kwargs.pop("default_headers")
        if "default_query" in kwargs:
            client_kwargs["default_query"] = kwargs.pop("default_query")
        if "transport" in kwargs:
            client_kwargs["transport"] = kwargs.pop("transport")
        if "proxies" in kwargs:
            client_kwargs["proxies"] = kwargs.pop("proxies")
        if "connection_pool_limits" in kwargs:
            client_kwargs["connection_pool_limits"] = kwargs.pop(
                "connection_pool_limits"
            )
        if "_strict_response_validation" in kwargs:
            client_kwargs["_strict_response_validation"] = kwargs.pop(
                "_strict_response_validation"
            )
        self.__call_args = kwargs

        self.__client = anthropic.AsyncAnthropic(**client_kwargs, max_retries=0)

    def get_model_name(self) -> str:
        # Try some well known keys
        return typing.cast(str, self.kwargs["model"])

    def _exception_to_code(self, e: BaseException) -> int | None:
        if isinstance(e, anthropic.APIStatusError):
            return e.status_code
        return None

    async def _run_chat(
        self, chats: typing.List[api_types.LLMChat]
    ) -> typing.Tuple[str, api_types.LLMOutputModel]:
        messages = "".join(
            [
                (anthropic.HUMAN_PROMPT if c["role"] == "user" else anthropic.AI_PROMPT)
                + " "
                + c["content"]
                for c in chats
            ]
        )
        aprompt_tokens = self.__client.count_tokens(messages)
        response: anthropic.types.Completion = await self.__client.completions.create(
            prompt=f"{messages}{anthropic.AI_PROMPT}",
            **self.kwargs,
        )

        model = response.model
        text = response.completion

        output_tokens = await self.__client.count_tokens(text)
        prompt_tokens = await aprompt_tokens

        return model, api_types.LLMOutputModel(
            raw_text=text,
            metadata=api_types.LLMOutputModelMetadata(
                logprobs=None,
                prompt_tokens=prompt_tokens,
                output_tokens=output_tokens,
                total_tokens=output_tokens + prompt_tokens,
            ),
        )

    async def _run_completion(
        self, prompt: str
    ) -> typing.Tuple[str, api_types.LLMOutputModel]:
        messages = f"{anthropic.HUMAN_PROMPT} {prompt}{anthropic.AI_PROMPT}"
        aprompt_tokens = self.__client.count_tokens(messages)
        response: anthropic.types.Completion = await self.__client.completions.create(
            prompt=messages,
            **self.__call_args,
        )

        model = response.model
        text = response.completion

        output_tokens = await self.__client.count_tokens(text)
        prompt_tokens = await aprompt_tokens

        return model, api_types.LLMOutputModel(
            raw_text=text,
            metadata=api_types.LLMOutputModelMetadata(
                logprobs=None,
                prompt_tokens=prompt_tokens,
                output_tokens=output_tokens,
                total_tokens=output_tokens + prompt_tokens,
            ),
        )
