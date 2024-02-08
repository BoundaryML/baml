import typing
from .llm_provider_base import AbstractLLMProvider

T = typing.TypeVar("T", bound=AbstractLLMProvider)


class LLMProviderFactory:
    _registry: typing.Dict[str, typing.Type[AbstractLLMProvider]] = {}

    @classmethod
    def register(
        cls, *provider: str
    ) -> typing.Callable[[typing.Type[T]], typing.Type[T]]:
        def decorator(sub_cls: typing.Type[T]) -> typing.Type[T]:
            if not issubclass(sub_cls, AbstractLLMProvider):
                raise TypeError(
                    "Registered class must inherit from AbstractLLMProvider"
                )
            for p in provider:
                assert p not in cls._registry, f"LLMProvider already registered: '{p}'"
                cls._registry[p] = sub_cls

            return sub_cls

        return decorator

    @classmethod
    def create_instance(
        cls,
        *,
        provider: str,
        **kwargs: typing.Any,
    ) -> AbstractLLMProvider:
        assert (
            provider in cls._registry
        ), f"LLMProvider not registered: '{provider}'. Use one of: {list(cls._registry.keys())}"
        client_cls = cls._registry[provider]
        return client_cls(provider=provider, **kwargs)


register_llm_provider = LLMProviderFactory.register
llm_provider_factory = LLMProviderFactory.create_instance
