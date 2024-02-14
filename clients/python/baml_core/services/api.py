from __future__ import annotations

import http
import typing

import pydantic
import requests
import datetime
from . import api_types
from ..logger import logger
from .api_types import LogSchema
import platform

T = typing.TypeVar("T", bound=pydantic.BaseModel)
U = typing.TypeVar("U", bound=pydantic.BaseModel)


class _APIWrapper:
    def __init__(
        self,
        *,
        base_url: str,
        api_key: str,
        project_id: str,
        session_id: str,
        stage: str,
    ) -> None:
        self.__project_id = project_id
        self.__session_id = session_id
        self.__base_url = base_url
        self.__headers: typing.Dict[str, str] = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {api_key}",
        }
        self.stage = stage

    @property
    def project_id(self) -> str:
        return self.__project_id

    @property
    def session_id(self) -> str:
        return self.__session_id

    def call_api_sync(
        self,
        endpoint: str,
        payload: pydantic.BaseModel,
        parser: typing.Type[U] | None = None,
    ) -> U | None:
        data = payload.model_dump(by_alias=True)
        response = requests.post(
            f"{self.__base_url}/{endpoint}", json=data, headers=self.__headers
        )
        if response.status_code != http.HTTPStatus.OK:
            text = response.text
            raise Exception(f"Failed with status code {response.status_code}: {text}")
        if parser:
            return parser.model_validate_json(response.text)
        else:
            return None

    # async def _call_api(
    #     self, endpoint: str, payload: T, parser: typing.Type[U] | None = None
    # ) -> U | None:
    #     async with aiohttp.ClientSession() as session:
    #         data = payload.model_dump(by_alias=True)
    #         async with session.post(
    #             f"{self.base_url}/{endpoint}", headers=self.headers, json=data
    #         ) as response:
    #             if response.status != 200:
    #                 text = await response.text()
    #                 raise Exception(
    #                     f"Failed with status code {response.status}: {text}"
    #                 )
    #             if parser:
    #                 return parser.model_validate_json(await response.text())
    #             else:
    #                 return None


class __APIBase:
    def __init__(self, *, base: _APIWrapper) -> None:
        self.__base = base

    @property
    def project_id(self) -> str:
        return self.__base.project_id

    @property
    def session_id(self) -> str:
        return self.__base.session_id

    @property
    def stage(self) -> str:
        return self.__base.stage

    def _call_api_sync(
        self,
        endpoint: str,
        payload: pydantic.BaseModel,
        parser: typing.Type[U] | None = None,
    ) -> U | None:
        return self.__base.call_api_sync(endpoint, payload, parser)

    # async def _call_api(
    #     self, endpoint: str, payload: T, parser: typing.Type[U] | None = None
    # ) -> U | None:
    #     return await self.__base._call_api(endpoint, payload, parser)


class TestingAPIWrapper(__APIBase):
    def __init__(self, base: _APIWrapper) -> None:
        super().__init__(base=base)

    def create_session(self) -> str:
        response = self._call_api_sync(
            "tests/create-cycle",
            api_types.CreateCycleRequest(
                project_id=self.project_id, session_id=self.session_id
            ),
            api_types.CreateCycleResponse,
        )
        if not response:
            raise Exception("Failed to create Gloo session")
        return response.dashboard_url

    def create_cases(self, *, payload: api_types.CreateTestCase) -> None:
        payload.project_id = self.project_id
        payload.test_cycle_id = self.session_id
        self._call_api_sync("tests/create-case", payload=payload)

    def update_case(self, *, payload: api_types.UpdateTestCase) -> None:
        if not self.project_id:
            logger.warning("project_id not set, dropping log.")
            return

        payload.project_id = self.project_id
        payload.test_cycle_id = self.session_id
        self._call_api_sync("tests/update", payload=payload)

    def update_case_sync(self, *, payload: api_types.UpdateTestCase) -> None:
        payload.project_id = self.project_id
        payload.test_cycle_id = self.session_id
        self._call_api_sync("tests/update", payload=payload)


class ProcessAPIWrapper(__APIBase):
    def __init__(self, base: _APIWrapper) -> None:
        super().__init__(base=base)

    def start(self) -> None:
        if not self.project_id:
            logger.warning("BOUNDARY_PROJECT_ID not set, dropping log.")
            return

        response = self._call_api_sync(
            "process/start",
            api_types.StartProcessRequest(
                project_id=self.project_id,
                session_id=self.session_id,
                stage=self.stage,
                hostname=platform.node(),
                start_time=datetime.datetime.utcnow().isoformat() + "Z",
                tags={
                    # TODO: Get git information (e.g. what branch we're on)
                },
            ),
            api_types.CreateCycleResponse,
        )
        if response:
            logger.info(f"\033[94mSee test results at: {response.dashboard_url}\033[0m")

    def end(self) -> None:
        if not self.project_id:
            logger.warning("BOUNDARY_PROJECT_ID not set, dropping log.")
            return

        self._call_api_sync(
            "process/end",
            api_types.EndProcessRequest(
                project_id=self.project_id,
                session_id=self.session_id,
                end_time=datetime.datetime.utcnow().isoformat() + "Z",
            ),
        )


class CacheRequestWithProjectId(api_types.CacheRequest):
    project_id: str


class APIWrapper(__APIBase):
    def __init__(
        self,
        *,
        base_url: str,
        api_key: str,
        project_id: str,
        session_id: str,
        stage: str,
    ) -> None:
        wrapper = _APIWrapper(
            base_url=base_url,
            api_key=api_key,
            project_id=project_id,
            session_id=session_id,
            stage=stage,
        )
        super().__init__(base=wrapper)
        self.test = TestingAPIWrapper(base=wrapper)
        self.process = ProcessAPIWrapper(base=wrapper)

    def check_cache(
        self, *, payload: api_types.CacheRequest
    ) -> api_types.CacheResponse | None:
        try:
            request = CacheRequestWithProjectId(
                project_id=self.project_id,
                **payload.model_dump(by_alias=True),
            )
            return self._call_api_sync("cache", request, api_types.CacheResponse)
        except Exception as _:
            # Swallow exceptions as we don't want to fail if cache is down.
            return None

    def log_sync(
        self,
        *,
        payload: LogSchema,
    ) -> None:
        try:
            self._call_api_sync("log/v2", payload)
        except Exception as e:
            event_name = payload.context.event_chain[-1].function_name
            if payload.context.event_chain[-1].variant_name:
                event_name = (
                    f"{event_name}::{payload.context.event_chain[-1].variant_name}"
                )
            logger.warning(f"Log failure on {event_name}: {e}")
            logger.debug(f"Dropped Payload: {payload}")
