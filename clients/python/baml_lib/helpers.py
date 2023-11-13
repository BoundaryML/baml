import os
import typing
import uuid

from baml_core.provider_manager import LLMManager
from baml_core.cache_manager import CacheManager
from baml_core import otel
from baml_core.services.api import APIWrapper
from baml_core.logger import logger


class __InternalBAMLConfig:
    def __init__(self, *, api: typing.Optional[APIWrapper] = None) -> None:
        self.api = api


__api: typing.Optional[APIWrapper] = None


class Params:
    def __init__(self) -> None:
        self.stage: str = os.environ.get("GLOO_STAGE", "prod")
        self.base_url: str = os.environ.get(
            "GLOO_BASE_URL", "https://app.trygloo.com/api"
        )
        self.project_id: typing.Optional[str] = os.environ.get("GLOO_APP_ID")
        self.secret_key: typing.Optional[str] = os.environ.get("GLOO_APP_SECRET")
        self.cache_enabled: bool = os.environ.get("GLOO_CACHE", "1") == "1"

    def set_base_url(self, base_url: typing.Optional[str]) -> None:
        if base_url is not None:
            if base_url == "reset":
                self.base_url = os.environ.get(
                    "GLOO_BASE_URL", "https://app.trygloo.com/api"
                )
            else:
                self.base_url = base_url

    def set_project_id(self, project_id: typing.Optional[str]) -> None:
        if project_id is not None:
            if project_id == "reset":
                self.project_id = os.environ.get("GLOO_APP_ID")
            else:
                self.project_id = project_id

    def set_secret_key(self, secret_key: typing.Optional[str]) -> None:
        if secret_key is not None:
            if secret_key == "reset":
                self.secret_key = os.environ.get("GLOO_APP_SECRET")
            else:
                self.secret_key = secret_key

    def set_stage(self, stage: typing.Optional[str]) -> None:
        if stage is not None:
            if stage == "reset":
                self.stage = os.environ.get("GLOO_STAGE", "prod")
            else:
                self.stage = stage

    def set_cache_enabled(self, cache_enabled: typing.Optional[bool]) -> None:
        if cache_enabled is not None:
            self.cache_enabled = cache_enabled

    def get_api(self) -> typing.Optional[APIWrapper]:
        if self.project_id is not None and self.secret_key is not None:
            process_id = str(uuid.uuid4())
            return APIWrapper(
                base_url=self.base_url,
                stage=self.stage,
                api_key=self.secret_key,
                project_id=self.project_id,
                session_id=process_id,
            )
        else:
            return None


__CachedParams = Params()


def baml_init(
    *,
    project_id: typing.Optional[str] = None,
    secret_key: typing.Optional[str] = None,
    base_url: typing.Optional[str] = None,
    enable_cache: typing.Optional[bool] = None,
    stage: typing.Optional[str] = None,
    idempotent: bool = False,
) -> __InternalBAMLConfig:
    global __api
    if idempotent and __api is not None:
        return __InternalBAMLConfig(api=__api)

    global __CachedParams
    __CachedParams.set_base_url(base_url)
    __CachedParams.set_project_id(project_id)
    __CachedParams.set_secret_key(secret_key)
    __CachedParams.set_stage(stage)
    __CachedParams.set_cache_enabled(enable_cache)

    __api = __CachedParams.get_api()
    otel.use_tracing(__api)

    if enable_cache:
        if __api:
            logger.info("Using GlooCache")
            CacheManager.add_cache("gloo", api=__api)
        else:
            logger.warn(
                "Wanted to use GlooCache but no API key was provided. Did you set GLOO_APP_ID and GLOO_APP_SECRET?"
            )

    LLMManager.validate()
    return __InternalBAMLConfig(api=__api)
