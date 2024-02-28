import os
import typing
import uuid

from baml_core.cache_manager import CacheManager
from baml_core import otel
from baml_core.services.api import APIWrapper
from baml_core.logger import logger
from baml_core.otel.provider import set_print_log_level
import logging


class __InternalBAMLConfig:
    def __init__(self, *, api: typing.Optional[APIWrapper] = None) -> None:
        self.api = api


class Params:
    def __init__(self) -> None:
        self.__stage: str = os.environ.get("GLOO_STAGE", "prod")
        self.__base_url: str = os.environ.get(
            "GLOO_BASE_URL", "https://app.boundaryml.com/api"
        )
        self.__project_id: typing.Optional[str] = os.environ.get(
            "BOUNDARY_PROJECT_ID"
        ) or os.environ.get("GLOO_APP_ID")
        self.__secret_key: typing.Optional[str] = os.environ.get(
            "BOUNDARY_SECRET"
        ) or os.environ.get("GLOO_APP_SECRET")
        self.cache_enabled: bool = os.environ.get("GLOO_CACHE", "1") == "1"
        self.__process_id = str(uuid.uuid4())
        self.api = self.__create_api(self.__process_id)
        otel.use_tracing(self.api)

    @property
    def process_id(self) -> str:
        return self.__process_id

    @process_id.setter
    def process_id(self, process_id: str) -> None:
        if process_id != self.__process_id:
            self.__process_id = process_id
            self.api = self.__create_api(process_id)
            otel.use_tracing(self.api)

    @property
    def stage(self) -> str:
        return self.__stage

    @stage.setter
    def stage(self, stage: str) -> None:
        if stage == "reset":
            stage = os.environ.get("GLOO_STAGE", "prod")
        if stage != self.__stage:
            self.__stage = stage
            self.process_id = str(uuid.uuid4())

    @property
    def base_url(self) -> str:
        return self.__base_url

    @base_url.setter
    def base_url(self, base_url: str) -> None:
        if base_url == "reset":
            base_url = os.environ.get("GLOO_BASE_URL", "https://app.boundaryml.com/api")
        if base_url != self.__base_url:
            self.__base_url = base_url
            self.process_id = str(uuid.uuid4())

    @property
    def project_id(self) -> typing.Optional[str]:
        return self.__project_id

    @project_id.setter
    def project_id(self, project_id: typing.Optional[str]) -> None:
        if project_id == "reset":
            project_id = os.environ.get("BOUNDARY_PROJECT_ID") or os.environ.get(
                "GLOO_APP_ID"
            )
        if project_id != self.__project_id:
            self.__project_id = project_id
            self.process_id = str(uuid.uuid4())

    @property
    def secret_key(self) -> typing.Optional[str]:
        return self.__secret_key

    @secret_key.setter
    def secret_key(self, secret_key: typing.Optional[str]) -> None:
        if secret_key == "reset":
            secret_key = os.environ.get("BOUNDARY_SECRET") or os.environ.get(
                "GLOO_APP_SECRET"
            )
        if secret_key != self.__secret_key:
            self.__secret_key = secret_key
            self.process_id = str(uuid.uuid4())

    def set_base_url(self, base_url: typing.Optional[str]) -> None:
        if base_url is not None:
            self.base_url = base_url

    def set_project_id(self, project_id: typing.Optional[str]) -> None:
        if project_id is not None:
            self.project_id = project_id

    def set_secret_key(self, secret_key: typing.Optional[str]) -> None:
        if secret_key is not None:
            self.secret_key = secret_key

    def set_stage(self, stage: typing.Optional[str]) -> None:
        if stage is not None:
            self.stage = stage

    def set_cache_enabled(self, cache_enabled: typing.Optional[bool]) -> None:
        if cache_enabled is not None:
            self.cache_enabled = cache_enabled

    def __create_api(self, process_id: str) -> typing.Optional[APIWrapper]:
        if self.project_id is not None and self.secret_key is not None:
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
    **kwargs: typing.Any,
) -> __InternalBAMLConfig:
    if log_level := os.environ.get("BAML_LOG_LEVEL", None):
        int_level = typing.cast(int, logging._checkLevel(log_level))  # type: ignore
        set_print_log_level(int_level)

    if kwargs.pop("idempotent", None) is not None:
        logger.warning("idempotent is deprecated. Please use enable_cache instead.")

    if on_message_hook := kwargs.pop("message_transformer_hook", None):
        logger.warning(
            "message_transformer_hook is deprecated. Please use baml_client.add_before_send_message_hook."
        )
        otel.add_message_transformer_hook(on_message_hook)

    global __CachedParams
    __CachedParams.set_base_url(base_url)
    __CachedParams.set_project_id(project_id)
    __CachedParams.set_secret_key(secret_key)
    __CachedParams.set_stage(stage)
    __CachedParams.set_cache_enabled(enable_cache)

    # TODO: doesnt actually work when we want idempotency
    if enable_cache:
        if __CachedParams.api is not None:
            logger.info("Using GlooCache")
            CacheManager.add_cache("gloo", api=__CachedParams.api)
        else:
            logger.warning(
                "Wanted to use GlooCache but no API key was provided. Did you set BOUNDARY_PROJECT_ID and BOUNDARY_SECRET?"
            )

    return __InternalBAMLConfig(api=__CachedParams.api)
