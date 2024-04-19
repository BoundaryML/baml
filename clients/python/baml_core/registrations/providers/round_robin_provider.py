from random import randint
import asyncio
import typing
from typing import Any, AsyncIterator, Dict, List, Optional, Union

from baml_core_ffi import TemplateStringMacro

from baml_core.provider_manager import (
    register_llm_provider,
    LLMResponse,
    LLMManager,
    LLMChatMessage,
)
from baml_core.provider_manager.llm_provider_base import AbstractLLMProvider


def _parse_strategy_item(item: typing.Any) -> str:
    if isinstance(item, str):
        return item
    if isinstance(item, dict):
        if "client" not in item:
            raise ValueError(
                f"{RoundRobinProvider.__name__} strategy entries must specify 'client'"
            )
        client = item["client"]
        if isinstance(client, str):
            return client
        raise ValueError(
            f"{RoundRobinProvider.__name__} strategy entries must specify 'client' as a string"
        )

    raise ValueError(
        f"{RoundRobinProvider.__name__} strategy entries must be string or dicts. Got: {type(item)}"
    )


def _parse_strategy(strategy: Optional[Any]) -> List[str]:
    if strategy is None:
        raise ValueError(f"{RoundRobinProvider.__name__} requires a strategy option")
    if not isinstance(strategy, list):
        raise ValueError(
            f"{RoundRobinProvider.__name__} strategy must be a list. Got: {type(strategy)}"
        )

    return [_parse_strategy_item(item) for item in strategy]


@register_llm_provider("baml-round-robin")
@typing.final
class RoundRobinProvider(AbstractLLMProvider):
    __kwargs: Dict[str, Any]
    __providers: List[str]
    __provider_index: int
    __lock: asyncio.Lock

    def _to_error_code(self, e: Exception) -> typing.Optional[int]:
        return None

    def __init__(self, *, options: Dict[str, Any], **kwargs: typing.Any) -> None:
        super().__init__(**kwargs)
        self.__kwargs = options
        self.__lock = asyncio.Lock()

        self.__providers = _parse_strategy(options.pop("strategy", None))
        start = options.pop("start", None)
        if start == "random" or start is None:
            self.__provider_index = randint(0, len(self.__providers) - 1)
        else:
            assert isinstance(start, int) and 0 <= start < len(
                self.__providers
            ), f"{self.__class__.__name__} specifies an unknown start index: {start}"
            self.__provider_index = start

    def _validate(self) -> None:
        """
        Run any validation checks on the provider. This is called via
        baml_init() and should raise an exception if the provider is
        not configured correctly.
        """
        assert (
            len(self.__kwargs) == 0
        ), f"{self.__class__.__name__} has unexpected options: {self.__kwargs}"
        assert (
            len(self.__providers) > 0
        ), f"{self.__class__.__name__} requires a strategy of at least 1"

        del self.__kwargs

    async def _choose_provider(self) -> AbstractLLMProvider:
        assert (
            len(self.__providers) > 0
        ), f"0 providers but {len(self.__providers)} strategy items"
        # The lock is needed for the get-and-increment to be atomic. Strictly
        # speaking, it's not necessary (this race condition would not be a
        # mission-critical bug), so if anyone complains about it because of
        # performance or some compatibility issue, we can remove it.
        async with self.__lock:
            provider_index = self.__provider_index
            self.__provider_index = (self.__provider_index + 1) % len(self.__providers)
        client_name = self.__providers[provider_index % len(self.__providers)]
        return LLMManager.get_llm(client_name)

    async def _run_jinja_template_internal(
        self,
        *,
        jinja_template: str,
        args: Dict[str, Any],
        template_macros: List[TemplateStringMacro],
        output_format: str,
    ) -> LLMResponse:
        return await (await self._choose_provider()).run_jinja_template(
            jinja_template=jinja_template,
            args=args,
            template_macros=template_macros,
            output_format=output_format,
        )

    async def _run_prompt_internal(self, prompt: str) -> LLMResponse:
        return await (await self._choose_provider()).run_prompt(prompt)

    async def _run_prompt_template_internal(
        self,
        *,
        template: str,
        replacers: typing.Iterable[str],
        params: Dict[str, Any],
    ) -> LLMResponse:
        return await (await self._choose_provider()).run_prompt_template(
            template=template, replacers=replacers, params=params
        )

    async def _run_chat_internal(
        self, *messages: Union[LLMChatMessage, List[LLMChatMessage]]
    ) -> LLMResponse:
        return await (await self._choose_provider()).run_chat(*messages)

    async def _run_chat_template_internal(
        self,
        *message_templates: Union[LLMChatMessage, List[LLMChatMessage]],
        replacers: typing.Iterable[str],
        params: Dict[str, Any],
    ) -> LLMResponse:
        return await (await self._choose_provider()).run_chat_template(
            *message_templates,
            replacers=replacers,
            params=params,
        )

    async def _run_prompt_internal_stream(
        self, prompt: str
    ) -> AsyncIterator[LLMResponse]:
        # We _should_ use '(await self._choose_provider()).run_prompt_stream(prompt)'
        # here, but we have to use getattr because the inheritance hierarchy for
        # python providers is f'd up
        async for r in (await self._choose_provider()).run_prompt_stream(prompt):
            yield r

    async def _run_prompt_template_internal_stream(
        self,
        *,
        template: str,
        replacers: typing.Iterable[str],
        params: Dict[str, Any],
    ) -> AsyncIterator[LLMResponse]:
        async for r in (await self._choose_provider()).run_prompt_template_stream(
            template=template,
            replacers=replacers,
            params=params,
        ):
            yield r

    async def _run_chat_internal_stream(
        self, *messages: Union[LLMChatMessage, List[LLMChatMessage]]
    ) -> AsyncIterator[LLMResponse]:
        # We _should_ use '(await self._choose_provider()).run_chat_stream(prompt)'
        # here, but we have to use getattr because the inheritance hierarchy for
        # python providers is f'd up
        async for r in (await self._choose_provider()).run_chat_stream(*messages):
            yield r

    async def _run_chat_template_internal_stream(
        self,
        *message_templates: Union[LLMChatMessage, List[LLMChatMessage]],
        replacers: typing.Iterable[str],
        params: Dict[str, Any],
    ) -> AsyncIterator[LLMResponse]:
        async for r in (await self._choose_provider()).run_chat_template_stream(
            *message_templates,
            replacers=replacers,
            params=params,
        ):
            yield r

    async def _run_jinja_template_internal_stream(
        self,
        *,
        jinja_template: str,
        args: Dict[str, Any],
        output_format: str,
        template_macros: List[TemplateStringMacro],
    ) -> AsyncIterator[LLMResponse]:
        async for r in (await self._choose_provider()).run_jinja_template_stream(
            jinja_template=jinja_template,
            args=args,
            output_format=output_format,
            template_macros=template_macros,
        ):
            yield r
