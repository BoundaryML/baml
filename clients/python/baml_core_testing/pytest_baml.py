import asyncio
import typing
import pytest

# from gloo_internal.api import API
# from gloo_internal.tracer import trace
# from gloo_internal.api_types import (
#     TestCaseStatus,
# )
# from gloo_internal.env import ENV
# from gloo_internal.logging import logger
from baml_core.otel import trace
from baml_core.services.api import APIWrapper
import os
import re

# TODO import from baml_core logger
import logging
from baml_core.services.logger import logger

# from gloo_internal import api_types
from baml_core.services import api_types
from baml_core import baml_init

logger.setLevel(logging.DEBUG)
baml_test = pytest.mark.baml_test


class GlooTestCaseBase(typing.TypedDict):
    name: str


T = typing.TypeVar("T", bound=GlooTestCaseBase)


def pytest_configure(config: pytest.Config) -> None:
    logger.debug("Registering pytest_gloo plugin.")
    config.addinivalue_line(
        "markers",
        "baml_test: mark test as a BAML test to upload data to the BAML dashboard",
    )
    # BAML INIT here
    # Add optional stage parameter to baml_init
    # baml_init(), returns api wrapper we can use
    baml_conf = baml_init(stage="test")
    if baml_conf is None or baml_conf.api is None:
        logger.warn(
            "BAML plugin disabled due to missing environment variables. Did you set GLOO_APP_ID and GLOO_APP_SECRET?"
        )
        return

    config.pluginmanager.register(BamlPytestPlugin(api=baml_conf.api), "pytest_baml")


class TestCaseMetadata:
    node_id: str
    dataset_name: str
    test_name: str
    case_name: str

    def __init__(self, item: pytest.Item) -> None:
        self.node_id = item.nodeid
        self.dataset_name = item.parent.name if item.parent else "Ungrouped"

        test_name = "test"
        case_name = item.name

        # TODO: Do this better.
        # test_name = item.name
        # try:
        #     match = re.search(r"\[(.*?)\]", test_name)
        #     if match:
        #         case_name = match.group(1)
        #         test_name = re.sub(r"\[.*?\]", "", test_name)
        #     else:
        #         case_name = "__default__"
        # except AttributeError:
        #     case_name = "__error__"

        self.test_name = test_name
        self.case_name = case_name

    def __str__(self) -> str:
        return f"{self.dataset_name}/{self.test_name}/{self.case_name}"

    @property
    def tags(self) -> typing.Dict[str, str]:
        return {
            "dataset_name": self.dataset_name,
            "test_name": self.test_name,
            "case_name": self.case_name,
        }


def sanitize(input_str: str) -> str:
    return re.sub(r"[^\w-]", "_", input_str)


# See https://docs.pytest.org/en/7.1.x/_modules/_pytest/hookspec.html#pytest_runtestloop
class BamlPytestPlugin:
    def __init__(self, api: APIWrapper) -> None:
        self.__gloo_tests: typing.Dict[str, TestCaseMetadata] = {}
        self.__completed_tests: typing.Set[str] = set()
        self.api = api

    # On register, we want to set the STAGE env variable
    # to "test" so that the tracer knows to send the logs
    # def pytest_sessionstart(self, session: pytest.Session) -> None:
    #     os.environ["GLOO_STAGE"] = "test"

    @pytest.hookimpl(tryfirst=True)
    def pytest_collection_finish(self, session: pytest.Session) -> None:
        """Called after collection has been performed and modified.

        :param pytest.Session session: The pytest session object.
        """

        # Check if any of the tests are marked as gloo tests
        # If not, we can skip the rest of the setup

        for item in session.items:
            if any(map(lambda mark: mark.name == "baml_test", item.iter_markers())):
                self.__gloo_tests[item.nodeid] = TestCaseMetadata(item)
                # logger.info(
                #     f"Found baml test: {item.nodeid}: {self.__gloo_tests[item.nodeid]}"
                # )

    def maybe_start_logging(self, session: pytest.Session) -> None:
        logger.debug(
            f"Starting logging: Num Tests: {len(self.__gloo_tests)}, {len(session.items)}"
        )
        if len(self.__gloo_tests) == 0:
            logger.debug("No Baml tests detected")
            return

        logger.debug("Creating test cases")

        dataset_cases: typing.Dict[str, typing.Dict[str, typing.List[str]]] = {}
        for item in self.__gloo_tests.values():
            # Add case_name to the corresponding dataset
            if item.dataset_name not in dataset_cases:
                dataset_cases[item.dataset_name] = {}
            if item.test_name not in dataset_cases[item.dataset_name]:
                dataset_cases[item.dataset_name][item.test_name] = []
            dataset_cases[item.dataset_name][item.test_name].append(item.case_name)

        # Validate that no duplicate test cases are being created
        for dataset_name, test_cases in dataset_cases.items():
            for test_name, case_names in test_cases.items():
                if len(set(case_names)) != len(case_names):
                    duplicate_cases = [
                        case_name
                        for case_name in case_names
                        if case_names.count(case_name) > 1
                    ]
                    raise Exception(
                        f"Duplicate test cases found in dataset {dataset_name} test {test_name}: {duplicate_cases}"
                    )

        # replace w/ new api wrapper created in Baml INit
        # await API.test.create_session()

        for dataset_name, test_cases in dataset_cases.items():
            for test_name, case_names in test_cases.items():
                self.api.test.create_cases(
                    payload=api_types.CreateTestCase(
                        test_dataset_name=dataset_name,
                        test_name=test_name,
                        test_case_args=[{"name": c} for c in case_names],
                    )
                )

    @pytest.hookimpl(tryfirst=True)
    def pytest_runtestloop(
        self, session: pytest.Session
    ) -> typing.Optional[typing.Any]:
        if (
            session.testsfailed
            and not session.config.option.continue_on_collection_errors
        ):
            raise session.Interrupted(
                "%d errors during collection" % session.testsfailed
            )

        if session.config.option.collectonly:
            return True

        # asyncio.run(self.maybe_start_logging(session))
        self.maybe_start_logging(session)
        return None

    @pytest.hookimpl(tryfirst=True)
    def pytest_runtest_logstart(
        self, nodeid: str, location: typing.Tuple[str, typing.Optional[int], str]
    ) -> None:
        """Called at the start of running the runtest protocol for a single item.

        See :hook:`pytest_runtest_protocol` for a description of the runtest protocol.

        :param str nodeid: Full node ID of the item.
        :param location: A tuple of ``(filename, lineno, testname)``.
        """
        if nodeid in self.__gloo_tests:
            item = self.__gloo_tests[nodeid]
            # Log the start of the test
            self.api.test.update_case_sync(
                payload=api_types.UpdateTestCase(
                    test_dataset_name=item.dataset_name,
                    test_case_definition_name=item.test_name,
                    test_case_arg_name=item.case_name,
                    status=api_types.TestCaseStatus.RUNNING,
                    error_data=None,
                )
            )

    # wrapper ensures we can yield to other hooks
    # this one just sets the context but doesnt actually run
    # the test. It lets the "default" hook run the test.
    @pytest.hookimpl(tryfirst=True)
    def pytest_runtest_call(self, item: pytest.Item) -> None:
        if item.nodeid not in self.__gloo_tests:
            return

        # Before running the test, make this a traced function.
        meta = self.__gloo_tests[item.nodeid]
        tags = dict(
            test_case_arg_name=meta.case_name,
            test_case_name=meta.test_name,
            # TODO: do a test var
            test_cycle_id=self.api.test.session_id,
            test_dataset_name=meta.dataset_name,
        )

        item.obj = trace(_tags=tags)(item.obj)  # type: ignore

    @pytest.hookimpl(tryfirst=True)
    def pytest_runtest_makereport(
        self, item: pytest.Item, call: pytest.CallInfo[typing.Any]
    ) -> None:
        if item.nodeid not in self.__gloo_tests:
            return

        if call.when == "call":
            status = (
                api_types.TestCaseStatus.PASSED
                if call.excinfo is None
                else api_types.TestCaseStatus.FAILED
            )

            meta = self.__gloo_tests[item.nodeid]
            self.api.test.update_case_sync(
                payload=api_types.UpdateTestCase(
                    test_dataset_name=meta.dataset_name,
                    test_case_definition_name=meta.test_name,
                    test_case_arg_name=meta.case_name,
                    status=status,
                    error_data={"error": str(call.excinfo.value)}
                    if call.excinfo
                    else None,
                )
            )
            self.__completed_tests.add(item.nodeid)

    @pytest.hookimpl(tryfirst=True)
    def pytest_sessionfinish(
        self,
        session: pytest.Session,
        exitstatus: typing.Union[int, pytest.ExitCode],
    ) -> None:
        if session.config.option.collectonly:
            return

        if (
            session.testsfailed
            and not session.config.option.continue_on_collection_errors
        ):
            return

        try:
            for nodeid, meta in self.__gloo_tests.items():
                if nodeid not in self.__completed_tests:
                    self.api.test.update_case_sync(
                        payload=api_types.UpdateTestCase(
                            test_dataset_name=meta.dataset_name,
                            test_case_definition_name=meta.test_name,
                            test_case_arg_name=meta.case_name,
                            status=api_types.TestCaseStatus.CANCELLED,
                            error_data=None,
                        )
                    )
        except Exception as e:
            # If we don't catch this the user is not able to see any other underlying test errors.
            logger.error(f"Failed to update test case status: {e}")
