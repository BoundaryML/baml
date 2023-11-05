import typing
from .abstract_cache_provider import AbstractCacheProvider

T = typing.TypeVar("T", bound=AbstractCacheProvider)


class CacheProviderFactory:
    _registry: typing.Dict[str, typing.Type[AbstractCacheProvider]] = {}

    @classmethod
    def register(
        cls, *provider: str
    ) -> typing.Callable[[typing.Type[T]], typing.Type[T]]:
        def decorator(sub_cls: typing.Type[T]) -> typing.Type[T]:
            if not issubclass(sub_cls, AbstractCacheProvider):
                raise TypeError(
                    "Registered class must inherit from AbstractCacheProvider"
                )
            for p in provider:
                assert (
                    p not in cls._registry
                ), f"CacheProvider already registered: '{p}'"
                cls._registry[p] = sub_cls

            return sub_cls

        return decorator

    @classmethod
    def create_instance(
        cls,
        *,
        provider: str,
        **kwargs: typing.Any,
    ) -> AbstractCacheProvider:
        assert (
            provider in cls._registry
        ), f"CacheProvider not registered: '{provider}'. Use one of: {list(cls._registry.keys())}"
        client_cls = cls._registry[provider]
        return client_cls(**kwargs)


register_cache_provider = CacheProviderFactory.register
cache_provider_factory = CacheProviderFactory.create_instance
