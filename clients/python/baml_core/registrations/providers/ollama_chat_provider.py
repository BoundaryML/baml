import typing
import ollama


from baml_core.provider_manager import (
    LLMChatMessage,
    LLMChatProvider,
    LLMResponse,
    register_llm_provider,
)


def _to_chat_completion_messages(msg: LLMChatMessage) -> ollama.Message:
    role = typing.cast(typing.Literal["user", "assistant", "system"], msg['role']) if msg['role'] in ["user", "assistant", "system"] else "system"
    return {
        "content": msg["content"],
        "role": role,
        "images": None,
    }

@register_llm_provider("baml-ollama-chat")
@typing.final
class OllamaChatProvider(LLMChatProvider):
    __client: ollama.AsyncClient
    __kwargs: typing.Dict[str, typing.Any]

    def __init__(
        self, *, options: typing.Dict[str, typing.Any], **kwargs: typing.Any
    ) -> None:
        default_chat_role = kwargs.pop("default_chat_role", "system")
        assert isinstance(
            default_chat_role, str
        ), f"default_chat_role must be a string: {type(default_chat_role)}. {default_chat_role}"
        super().__init__(
            prompt_to_chat=lambda prompt: LLMChatMessage(
                role=default_chat_role, content=prompt
            ),
            **kwargs,
        )

        client_kwargs: typing.Dict[str, typing.Any] = {}
        if "host" in options:
            client_kwargs["host"] = options.pop("host")
        if "follow_redirects" in options:
            client_kwargs["follow_redirects"] = options.pop("follow_redirects")
        self.__client = ollama.AsyncClient(**client_kwargs)


        self.__kwargs = {}
        for params in ["model", "format", "options"]:
            if params in options:
                self.__kwargs[params] = options.pop(params)
        
        # All options should be consumed
        if options:
            raise ValueError(f"Unknown options: {options} for OllamaChatProvider")
        self._set_args(**self.__kwargs)

    def _to_error_code(self, error: Exception) -> typing.Optional[int]:
        return None
    
    async def _stream_chat(self, messages: typing.List[LLMChatMessage]) -> typing.AsyncIterator[LLMResponse]:
        stream: typing.AsyncIterator[ollama.ChatResponse] = await self.__client.chat(
            messages=list(map(_to_chat_completion_messages, messages)),
            **self.__kwargs,
            stream=True,
        )

        async for response in stream:
            yield LLMResponse(
                generated=response['message']["content"],
                model_name=response['model'],
                meta=dict(
                    baml_is_complete=response['done'],
                    logprobs=None,
                ),
            )

    def _validate(self) -> None:
        pass

    async def _run_chat(self, messages: typing.List[LLMChatMessage]) -> LLMResponse:
        response: ollama.ChatResponse = await self.__client.chat(
            messages=list(map(_to_chat_completion_messages, messages)),
            **self.__kwargs,
        )
        

        text = response['message']
        finish_reason = response["done"]

        return LLMResponse(
            generated=text["content"],
            model_name=response['model'],
            meta=dict(
                baml_is_complete=response['done'],
                logprobs=None,
            ),
        )
