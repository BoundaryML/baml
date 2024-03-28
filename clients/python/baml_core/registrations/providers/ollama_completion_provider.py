import typing
import ollama  # type: ignore


from baml_core.provider_manager import (
    LLMProvider,
    LLMResponse,
    register_llm_provider,
)


@register_llm_provider("baml-ollama-completion")
@typing.final
class OllamaCompletionProvider(LLMProvider):
    __client_host: str
    __kwargs: typing.Dict[str, typing.Any]

    def __init__(
        self, *, options: typing.Dict[str, typing.Any], **kwargs: typing.Any
    ) -> None:
        super().__init__(**kwargs)

        self.__client_host = options.pop("host", "http://localhost:11434")
        self.__kwargs = {}
        for params in [
            "model",
            "format",
            "options",
            "system",
            "template",
            "context",
            "raw",
            "keep_alive",
        ]:
            if params in options:
                self.__kwargs[params] = options.pop(params)

        # All options should be consumed
        if options:
            raise ValueError(f"Unknown options: {options} for OllamaCompletionProvider")
        self._set_args(**self.__kwargs)

    def _to_error_code(self, error: Exception) -> typing.Optional[int]:
        return None

    async def _stream(self, prompt: str) -> typing.AsyncIterator[LLMResponse]:
        client = ollama.AsyncClient(host=self.__client_host)
        response: typing.AsyncIterator[  # type: ignore
            ollama.GenerateResponse
        ] = await client.generate(
            prompt=prompt,
            **self.__kwargs,
            stream=True,
        )

        async for message in response:
            yield LLMResponse(
                generated=message["response"],
                model_name=message["model"],
                meta=dict(
                    baml_is_complete=message["done"],
                    logprobs=None,
                ),
            )

    def _validate(self) -> None:
        pass

    async def _run(self, prompt: str) -> LLMResponse:
        client = ollama.AsyncClient(host=self.__client_host)
        response: ollama.GenerateResponse = await client.generate(  # type: ignore
            prompt=prompt,
            **self.__kwargs,
        )

        text = response["response"]

        return LLMResponse(
            generated=text,
            model_name=response["model"],
            meta=dict(
                baml_is_complete=response["done"],
                logprobs=None,
            ),
        )
