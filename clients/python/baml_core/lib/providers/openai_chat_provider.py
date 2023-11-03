import openai
import typing

from ..._impl.provider import (
    LLMChatMessage,
    LLMChatProvider,
    LLMResponse,
    register_llm_provider,
)


@register_llm_provider("openai-chat", "azure-chat")
@typing.final
class OpenAIChatProvider(LLMChatProvider):
    __kwargs: typing.Dict[str, typing.Any]

    def __init__(
        self, *, options: typing.Dict[str, typing.Any], **kwargs: typing.Any
    ) -> None:
        default_chat_role = kwargs.pop("default_chat_role", "user")
        assert (
            type(default_chat_role) is str
        ), f"default_chat_role must be a string: {type(default_chat_role)}. {default_chat_role}"

        super().__init__(
            prompt_to_chat=lambda prompt: LLMChatMessage(
                role=default_chat_role, content=prompt
            ),
            **kwargs,
        )
        self.__kwargs = options
        self._set_args(**self.__kwargs)

    async def _run_chat(self, messages: typing.List[LLMChatMessage]) -> LLMResponse:
        response = await openai.ChatCompletion.acreate(messages=messages, **self.__kwargs)  # type: ignore
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
