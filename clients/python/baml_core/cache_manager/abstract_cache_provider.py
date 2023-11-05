import abc
from ..services.api_types import CacheRequest, CacheResponse
import typing


class AbstractCacheProvider:
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
