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
            # Anthropic requires a max_tokens_to_sample
            kwargs["max_tokens_to_sample"] = 300

        if "max_retries" in kwargs and "__retry" in kwargs:
            assert False, "Cannot specify both max_retries and __retry"

        __retry = kwargs.pop("__retry", kwargs.pop("max_retries", 0))
        super().__init__(provider=provider, __retry=__retry, **kwargs)

        client_kwargs = {}
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
        ]
        for arg_name in client_arg_names:
            if arg_name in self.kwargs:
                client_kwargs[arg_name] = self.kwargs.pop(arg_name)

        # Let gloo handle retries
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
