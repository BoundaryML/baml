# type: ignore
import openai
import typing

from baml_core.provider_manager import LLMProvider, LLMResponse, register_llm_provider
from .openai_helper import to_error_code


@register_llm_provider("baml-openai-completion", "baml-azure-completion")
@typing.final
class OpenAICompletionProvider(LLMProvider):
    __kwargs: typing.Dict[str, typing.Any]

    def __init__(
        self, *, options: typing.Dict[str, typing.Any], **kwargs: typing.Any
    ) -> None:
        super().__init__(**kwargs)
        if "temperature" not in options:
            options["temperature"] = 0
        self.__kwargs = options
        self._set_args(**self.__kwargs)

    def _to_error_code(self, e: Exception) -> typing.Optional[int]:
        return to_error_code(e)

    def _validate(self) -> None:
        pass

    async def _run(self, prompt: str) -> LLMResponse:
        response = await openai.Completion.acreate(prompt=prompt, **self.__kwargs)
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

    async def _stream(self, prompt: str) -> typing.AsyncIterator[LLMResponse]:
        yield await self._run(prompt)
