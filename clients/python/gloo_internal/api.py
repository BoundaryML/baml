from __future__ import annotations
import atexit
import datetime

import http
import typing

import aiohttp
import pydantic
import requests

from . import api_types
from .env import ENV
from .logging import logger

T = typing.TypeVar("T", bound=pydantic.BaseModel)
U = typing.TypeVar("U", bound=pydantic.BaseModel)


class _APIWrapper:
    def __init__(self) -> None:
        self.__base_url: None | str = None
        self.__project_id: None | str = None
        self.__headers: None | typing.Dict[str, str] = None

    @property
    def base_url(self) -> str:
        if self.__base_url is None:
            try:
                self.__base_url = ENV.GLOO_BASE_URL
            except Exception:
                self.__base_url = "https://app.trygloo.com/api"
        return self.__base_url

    @property
    def project_id(self) -> str:
        if self.__project_id is None:
            try:
                self.__project_id = ENV.GLOO_APP_ID
            except Exception:
                self.__project_id = ""
        return self.__project_id

    @property
    def key(self) -> str | None:
        try:
            return ENV.GLOO_APP_SECRET
        except Exception:
            return None

    @property
    def headers(self) -> typing.Dict[str, str]:
        if self.__headers is None:
            self.__headers = {
                "Content-Type": "application/json",
            }
            if self.key:
                self.__headers["Authorization"] = f"Bearer {self.key}"
        return self.__headers

    def _call_api_sync(
        self, endpoint: str, payload: T, parser: typing.Type[U] | None = None
    ) -> U | None:
        data = payload.model_dump(by_alias=True)
        response = requests.post(
            f"{self.base_url}/{endpoint}", json=data, headers=self.headers
        )
        if response.status_code != http.HTTPStatus.OK:
            text = response.text
            raise Exception(f"Failed with status code {response.status_code}: {text}")
        if parser:
            return parser.model_validate_json(response.text)
        else:
            return None

    async def _call_api(
        self, endpoint: str, payload: T, parser: typing.Type[U] | None = None
    ) -> U | None:
        async with aiohttp.ClientSession() as session:
            data = payload.model_dump(by_alias=True)
            async with session.post(
                f"{self.base_url}/{endpoint}", headers=self.headers, json=data
            ) as response:
                if response.status != 200:
                    text = await response.text()
                    raise Exception(
                        f"Failed with status code {response.status}: {text}"
                    )
                if parser:
                    return parser.model_validate_json(await response.text())
                else:
                    return None


class __APIBase:
    def __init__(self, *, base: _APIWrapper) -> None:
        self.__base = base

    @property
    def project_id(self) -> str:
        return self.__base.project_id

    def _call_api_sync(
        self, endpoint: str, payload: T, parser: typing.Type[U] | None = None
    ) -> U | None:
        return self.__base._call_api_sync(endpoint, payload, parser)

    async def _call_api(
        self, endpoint: str, payload: T, parser: typing.Type[U] | None = None
    ) -> U | None:
        return await self.__base._call_api(endpoint, payload, parser)


class TestingAPIWrapper(__APIBase):
    def __init__(self, base: _APIWrapper) -> None:
        super().__init__(base=base)

    async def create_session(self) -> None:
        if not self.project_id:
            logger.warning("GLOO_APP_ID not set, dropping log.")
            return

        response = await self._call_api(
            "tests/create-cycle",
            api_types.CreateCycleRequest(
                project_id=self.project_id, session_id=ENV.GLOO_PROCESS_ID
            ),
            api_types.CreateCycleResponse,
        )
        if response:
            logger.info(f"\033[94mSee test results at: {response.dashboard_url}\033[0m")

    async def create_cases(self, *, payload: api_types.CreateTestCase) -> None:
        if not self.project_id:
            logger.warning("GLOO_APP_ID not set, dropping log.")
            return

        payload.project_id = self.project_id
        payload.test_cycle_id = ENV.GLOO_PROCESS_ID
        await self._call_api("tests/create-case", payload=payload)

    async def update_case(self, *, payload: api_types.UpdateTestCase) -> None:
        if not self.project_id:
            logger.warning("GLOO_APP_ID not set, dropping log.")
            return

        payload.project_id = self.project_id
        payload.test_cycle_id = ENV.GLOO_PROCESS_ID
        await self._call_api("tests/update", payload=payload)

    def update_case_sync(self, *, payload: api_types.UpdateTestCase) -> None:
        if not self.project_id:
            logger.warning("GLOO_APP_ID not set, dropping log.")
            return

        payload.project_id = self.project_id
        payload.test_cycle_id = ENV.GLOO_PROCESS_ID
        self._call_api_sync("tests/update", payload=payload)


class ProcessAPIWrapper(__APIBase):
    def __init__(self, base: _APIWrapper) -> None:
        super().__init__(base=base)

    def start(self) -> None:
        if not self.project_id:
            logger.warning("GLOO_APP_ID not set, dropping log.")
            return

        response = self._call_api_sync(
            "process/start",
            api_types.StartProcessRequest(
                project_id=self.project_id,
                session_id=ENV.GLOO_PROCESS_ID,
                stage=ENV.GLOO_STAGE,
                hostname=ENV.GLOO_HOSTNAME,
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
            logger.warning("GLOO_APP_ID not set, dropping log.")
            return

        self._call_api_sync(
            "process/end",
            api_types.EndProcessRequest(
                project_id=self.project_id,
                session_id=ENV.GLOO_PROCESS_ID,
                end_time=datetime.datetime.utcnow().isoformat() + "Z",
            ),
        )


class APIWrapper(__APIBase):
    def __init__(self) -> None:
        wrapper = _APIWrapper()
        super().__init__(base=wrapper)
        self.test = TestingAPIWrapper(base=wrapper)
        self.process = ProcessAPIWrapper(base=wrapper)

    async def check_cache(
        self, *, payload: api_types.CacheRequest
    ) -> api_types.CacheResponse | None:
        if not (ENV.GLOO_STAGE == "test" or ENV.GLOO_CACHE == "1"):
            # logger.warning("Caching not enabled. SET GLOO_CACHE=1 to enable.")
            return None

        if not self.project_id:
            return None

        payload.project_id = self.project_id
        try:
            return await self._call_api("cache", payload, api_types.CacheResponse)
        except Exception:
            return None

    async def log(
        self,
        *,
        payload: api_types.LogSchema,
    ) -> None:
        if not self.project_id:
            logger.warning("GLOO_APP_ID not set, dropping log.")
            return

        try:
            payload.project_id = self.project_id
            await self._call_api("log/v2", payload)
        except Exception as e:
            event_name = payload.context.event_chain[-1].function_name
            if payload.context.event_chain[-1].variant_name:
                event_name = (
                    f"{event_name}::{payload.context.event_chain[-1].variant_name}"
                )
            logger.warning(f"Log failure on {event_name}: {e}")
            logger.debug(f"Dropped Payload: {payload}")


API = APIWrapper()
