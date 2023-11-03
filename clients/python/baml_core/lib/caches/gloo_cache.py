import typing

from ...services.api import APIWrapper
from ...services.api_types import CacheRequest, CacheResponse
from ..._impl.cache import register_cache_provider, AbstractCacheProvider


@register_cache_provider("gloo")
@typing.final
class GlooCache(AbstractCacheProvider):
    def __init__(self, api: APIWrapper) -> None:
        super().__init__()
        self.__api = api

    def get_llm_request(
        self, cache_request: CacheRequest
    ) -> typing.Optional[CacheResponse]:
        return self.__api.check_cache(payload=cache_request)

    def save_llm_request(
        self, cache_request: CacheRequest, response: CacheResponse
    ) -> None:
        # Gloo handles saving the cache directly in log.
        pass
