import openai
import typing

from ..._impl.provider import LLMProvider, LLMResponse, register_llm_provider


@register_llm_provider("openai", "azure")
@typing.final
class OpenAIProvider(LLMProvider):
    __kwargs: typing.Dict[str, typing.Any]

    def __init__(
        self, *, options: typing.Dict[str, typing.Any], **kwargs: typing.Any
    ) -> None:
        super().__init__(**kwargs)
        self.__kwargs = options
        self._set_args(**self.__kwargs)

    async def _run(self, prompt: str) -> LLMResponse:
        response = await openai.Completion.acreate(prompt=prompt, **self.__kwargs)  # type: ignore
        text = response["choices"][0]["text"]
        usage = response["usage"]
        model = response["model"]
        finish_reason = response["choices"][0]["finish_reason"]

        return LLMResponse(
            generated=text,
            model_name=model,
            meta=dict(
                baml_is_complete=finish_reason == "stop",
                logprobs=response["choices"][0]["logprobs"],
                prompt_tokens=usage.get("prompt_tokens", None),
                output_tokens=usage.get("completion_tokens", None),
                total_tokens=usage.get("total_tokens", None),
                finish_reason=finish_reason,
            ),
        )
