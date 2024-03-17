# type: ignore
import openai
import typing

from .openai_helper import to_error_code

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
        self.__kwargs = options
        self._set_args(**self.__kwargs)

    def _to_error_code(self, e: Exception) -> typing.Optional[int]:
        return to_error_code(e)

    def _validate(self) -> None:
        pass

    async def _run_chat(self, messages: typing.List[LLMChatMessage]) -> LLMResponse:
        response = await openai.ChatCompletion.acreate(
            # type: ignore
            messages=messages,
            **self.__kwargs,
        )

        text = response["choices"][0]["message"]["content"]
        usage = response["usage"]
        model = response["model"]
        finish_reason = response["choices"][0]["finish_reason"]

        return LLMResponse(
            generated=text,
            model_name=model,
            meta=dict(
                baml_is_complete=finish_reason == "stop",
                logprobs=None,
                prompt_tokens=usage.get("prompt_tokens", None),
                output_tokens=usage.get("completion_tokens", None),
                total_tokens=usage.get("total_tokens", None),
                finish_reason=finish_reason,
            ),
        )

    # async def _stream_chat(
    #     self, messages: typing.List[LLMChatMessage]
    # ) -> typing.AsyncIterator[LLMResponse]:
    #     yield await self._run_chat(messages)
