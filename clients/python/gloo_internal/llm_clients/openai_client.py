import typing
import openai

from .base_client import LLMClient
from .factory import register_llm_client
from .. import api_types


@register_llm_client(["openai", "azure"])
class OpenAILLMClient(LLMClient):
    def __init__(self, provider: str, **kwargs: typing.Any) -> None:
        super().__init__(provider=provider, **kwargs)

    def get_model_name(self) -> str:
        # Try some well known keys
        for key in ["model_name", "model", "engine"]:
            if key in self.kwargs:
                val = self.kwargs[key]
                if isinstance(val, str):
                    return val.lower()
        return "unknown"

    async def _run_chat(
        self, chats: typing.List[api_types.LLMChat]
    ) -> typing.Tuple[str, api_types.LLMOutputModel]:
        assert self.is_chat(), "This method is only for chat models"

        response = await openai.ChatCompletion.acreate(messages=chats, **self.kwargs)  # type: ignore
        text = response["choices"][0]["message"]["content"]
        usage = response["usage"]
        model = response["model"]
        return model, api_types.LLMOutputModel(
            raw_text=text,
            metadata=api_types.LLMOutputModelMetadata(
                logprobs=None,
                prompt_tokens=usage.get("prompt_tokens", None),
                output_tokens=usage.get("completion_tokens", None),
                total_tokens=usage.get("total_tokens", None),
            ),
        )

    async def _run_completion(
        self, prompt: str
    ) -> typing.Tuple[str, api_types.LLMOutputModel]:
        assert not self.is_chat(), "This method is only for completion models"

        response = await openai.Completion.acreate(prompt=prompt, **self.kwargs)  # type: ignore
        text = response["choices"][0]["text"]
        usage = response["usage"]
        model = response["model"]
        return model, api_types.LLMOutputModel(
            raw_text=text,
            metadata=api_types.LLMOutputModelMetadata(
                logprobs=response["choices"][0]["logprobs"],
                prompt_tokens=usage.get("prompt_tokens", None),
                output_tokens=usage.get("completion_tokens", None),
                total_tokens=usage.get("total_tokens", None),
            ),
        )
