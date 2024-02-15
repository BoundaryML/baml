from openai import AsyncOpenAI, AsyncAzureOpenAI
from openai.types.completion import Completion
from openai import AsyncClient
from .openai_helper_1 import to_error_code
import typing
from baml_core.provider_manager import (
    LLMProvider,
    LLMResponse,
    register_llm_provider,
)


@register_llm_provider("baml-openai-completion", "baml-azure-completion")
@typing.final
class OpenAICompletionProvider(LLMProvider):
    __kwargs: typing.Dict[str, typing.Any]
    _client: AsyncClient

    def __init__(
        self, *, options: typing.Dict[str, typing.Any], **kwargs: typing.Any
    ) -> None:
        super().__init__(**kwargs)
        if "temperature" not in options:
            options["temperature"] = 0

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

    async def _run(self, prompt: str) -> LLMResponse:
        response: Completion = await self._client.completions.create(
            prompt=prompt, **self.__kwargs
        )

        assert isinstance(
            response, Completion
        ), f"No completion response returned from LLM provider {self.provider}"

        if not response.choices:
            raise ValueError(
                f"No completion choice returned from LLM provider {self.provider}"
            )

        choice = response.choices[0]
        text = choice.text

        assert isinstance(
            text, str
        ), f"No content string returned from LLM provider: {self.provider}"

        usage = response.usage
        model = response.model
        finish_reason = choice.finish_reason
        logprobs = choice.logprobs
        prompt_tokens = usage.prompt_tokens if usage else None
        output_tokens = usage.completion_tokens if usage else None
        total_tokens = usage.total_tokens if usage else None

        return LLMResponse(
            generated=text,
            model_name=model,
            meta=dict(
                baml_is_complete=finish_reason == "stop",
                logprobs=logprobs,
                prompt_tokens=prompt_tokens,
                output_tokens=output_tokens,
                total_tokens=total_tokens,
                finish_reason=finish_reason,
            ),
        )

    async def _stream(self, prompt: str) -> typing.AsyncIterator[LLMResponse]:
        yield await self._run(prompt)
