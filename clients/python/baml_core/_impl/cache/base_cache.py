import abc
import typing

from ...services.api_types import CacheRequest, CacheResponse, LogSchema


class ICache:
    def __init__(self) -> None:
        self.__name = self.__class__.__name__

    @property
    def name(self) -> str:
        return self.__name

    @abc.abstractmethod
    def get_llm_request(
        self, cache_request: CacheRequest
    ) -> typing.Optional[CacheResponse]:
        pass

    @abc.abstractmethod
    def save_llm_request(
        self, cache_request: CacheRequest, response: CacheResponse
    ) -> None:
        pass


class _CacheManager:
    __caches: typing.List[ICache]

    def __init__(self) -> None:
        self.__caches = []

    def add_cache(self, cache: ICache) -> None:
        self.__caches.append(cache)

    def get_llm_request(
        self, cache_request: CacheRequest
    ) -> typing.Optional[CacheResponse]:
        for cache in self.__caches:
            try:
                response = cache.get_llm_request(cache_request)
                if response:
                    return response
            except BaseException:
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
            except BaseException:
                # Silently fail.
                pass


CacheManager = _CacheManager()
