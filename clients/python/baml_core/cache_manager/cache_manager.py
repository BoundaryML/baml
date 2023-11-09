import typing

from ..services.api_types import CacheRequest, CacheResponse, LogSchema
from .cache_factory import cache_provider_factory
from .abstract_cache_provider import AbstractCacheProvider


class _CacheManager:
    __caches: typing.List[AbstractCacheProvider]

    def __init__(self) -> None:
        self.__caches = []

    def add_cache(self, provider: str, **kwargs: typing.Any) -> None:
        cache = cache_provider_factory(provider=provider, **kwargs)
        self.__caches = [c for c in self.__caches if c.name != cache.name]
        self.__caches.append(cache)

    def get_llm_request(
        self, cache_request: CacheRequest
    ) -> typing.Optional[CacheResponse]:
        for cache in self.__caches:
            try:
                response = cache.get_llm_request(cache_request)
                if response:
                    return response
            except Exception:
                # Silently fail.
                pass
        return None

    def save_llm_request(self, log: LogSchema) -> None:
        if not log.metadata or log.event_type != "func_llm" or not log.metadata.output:
            return
        request = CacheRequest(
            provider=log.metadata.provider,
            prompt=log.metadata.input.prompt.template,
            prompt_vars=log.metadata.input.prompt.template_args,
            invocation_params=log.metadata.input.invocation_params,
        )
        response = CacheResponse(
            latency_ms=log.context.latency_ms,
            llm_output=log.metadata.output,
            model_name=log.metadata.mdl_name,
        )
        # We'll save it in all caches.
        for cache in self.__caches:
            try:
                cache.save_llm_request(request, response)
            except Exception:
                # Silently fail.
                pass


CacheManager = _CacheManager()
