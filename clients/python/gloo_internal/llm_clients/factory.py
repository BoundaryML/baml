from __future__ import annotations
import typing
from .base_client import LLMClient


class LLMClientRegistry:
    _registry: typing.Dict[str, typing.Type[LLMClient]] = {}

    @classmethod
    def register(
        cls, provider: str | typing.List[str]
    ) -> typing.Callable[[typing.Type[LLMClient]], typing.Type[LLMClient]]:
        def decorator(sub_cls: typing.Type[LLMClient]) -> typing.Type[LLMClient]:
            if not issubclass(sub_cls, LLMClient):
                raise TypeError("Registered class must inherit from LLMClient")
            if isinstance(provider, str):
                if provider in cls._registry:
                    raise ValueError(
                        f"LLMClient for provider '{provider}' already registered"
                    )
                cls._registry[provider] = sub_cls
            else:
                for p in provider:
                    if p in cls._registry:
                        raise ValueError(
                            f"LLMClient for provider '{p}' already registered"
                        )
                    cls._registry[p] = sub_cls
            return sub_cls

        return decorator

    @classmethod
    def create_instance(
        cls,
        *,
        provider: str,
        __default_fallback__: typing.Union["LLMClient", None] = None,
        __fallback__: typing.Union[typing.Dict[int, "LLMClient"], None] = None,
        **kwargs: typing.Any,
    ) -> LLMClient:
        if provider not in cls._registry:
            raise ValueError(f"No LLMClient registered for provider '{provider}'")
        client_cls = cls._registry[provider]
        return client_cls(
            provider=provider,
            **kwargs,
            __default_fallback__=__default_fallback__,
            __fallback__=__fallback__,
        )


register_llm_client = LLMClientRegistry.register
llm_client_factory = LLMClientRegistry.create_instance
