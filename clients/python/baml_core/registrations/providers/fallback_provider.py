import typing

from baml_core.provider_manager import (
    register_llm_provider,
    LLMResponse,
    LLMManager,
    LLMChatMessage,
)
from baml_core.provider_manager.llm_provider import AbstractLLMProvider


class ChainItem(typing.TypedDict):
    client_name: str
    on_status_code: typing.Optional[typing.List[int]]
    retry_policy: typing.Optional[str]


def _parse_strategy_item(item: typing.Any) -> ChainItem:
    if isinstance(item, str):
        return ChainItem(client_name=item, on_status_code=None, retry_policy=None)
    if isinstance(item, dict):
        if "client" not in item:
            raise ValueError("FallbackProvider requires a client option")
        strategy_item = ChainItem(
            client_name=item.pop("client"),
            on_status_code=item.pop("on_status_code", None),
            retry_policy=item.pop("retry_policy", None),
        )

        assert len(item) == 0, f"Unexpected options in strategy item: {item}"
        return strategy_item

    raise ValueError(
        f"FallbackProvider requires a strategy option as a list of strings or dicts. Got: {type(item)}"
    )


def _parse_strategy(strategy: typing.Optional[typing.Any]) -> typing.List[ChainItem]:
    if strategy is None:
        raise ValueError("FallbackProvider requires a strategy option")
    if not isinstance(strategy, list):
        raise ValueError(
            f"FallbackProvider requires a strategy option as a list. Got: {type(strategy)}"
        )

    return [_parse_strategy_item(item) for item in strategy]


@register_llm_provider("baml-fallback")
@typing.final
class FallbackProvider(AbstractLLMProvider):
    __kwargs: typing.Dict[str, typing.Any]
    __strategy: typing.Union[
        typing.List[
            typing.Tuple[AbstractLLMProvider, typing.Optional[typing.List[int]]]
        ],
        None,
    ]

    def _to_error_code(self, e: BaseException) -> typing.Optional[int]:
        return None

    def __init__(
        self, *, options: typing.Dict[str, typing.Any], **kwargs: typing.Any
    ) -> None:
        super().__init__(**kwargs)
        self.__strategy_raw = _parse_strategy(options.pop("strategy", None))
        self.__kwargs = options
        self.__strategy = None

    @property
    def _strategy(
        self,
    ) -> typing.List[
        typing.Tuple[AbstractLLMProvider, typing.Optional[typing.List[int]]]
    ]:
        if self.__strategy is None:
            raise ValueError(
                "FallbackProvider not initialized. Did you call baml_init()?"
            )
        return self.__strategy

    def _validate(self) -> None:
        """
        Run any validation checks on the provider. This is called via
        baml_init() and should raise an exception if the provider is
        not configured correctly.
        """
        if self.__strategy is not None:
            # Already validated nothing to change.
            return

        assert (
            len(self.__kwargs) == 0
        ), f"FallbackProvider has unexpected options: {self.__kwargs}"
        assert (
            len(self.__strategy_raw) > 0
        ), "FallbackProvider requires a strategy of at least 1"

        del self.__kwargs

        self.__strategy = [
            (LLMManager.get_llm(item["client_name"]), item["on_status_code"])
            for item in self.__strategy_raw
        ]
        del self.__strategy_raw

    async def _run_strategy(
        self,
        method_name: typing.Literal[
            "run_prompt", "run_prompt_template", "run_chat", "run_chat_template"
        ],
        *args: typing.Any,
        **kwargs: typing.Any,
    ) -> LLMResponse:
        error_code = None
        last_exception = None
        for idx, (llm, if_code) in enumerate(self._strategy):
            try:
                if idx > 0 and if_code is not None:
                    if error_code not in if_code:
                        continue
                return typing.cast(
                    LLMResponse, await getattr(llm, method_name)(*args, **kwargs)
                )
            except Exception as e:
                error_code = self._to_error_code(e)
                last_exception = e
        assert last_exception is not None, "Should have caught an exception"
        raise last_exception

    async def run_prompt(self, prompt: str) -> LLMResponse:
        return await self._run_strategy("run_prompt", prompt)

    async def run_prompt_template(
        self,
        *,
        template: str,
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> LLMResponse:
        return await self._run_strategy(
            "run_prompt_template", template, replacers=replacers, params=params
        )

    async def run_chat(
        self, *messages: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]]
    ) -> LLMResponse:
        return await self._run_strategy("run_chat", *messages)

    async def run_chat_template(
        self,
        *message_templates: typing.Union[LLMChatMessage, typing.List[LLMChatMessage]],
        replacers: typing.Iterable[str],
        params: typing.Dict[str, typing.Any],
    ) -> LLMResponse:
        return await self._run_strategy(
            "run_chat_template",
            *message_templates,
            replacers=replacers,
            params=params,
        )
