import asyncio
import typing
import typing_extensions
import colorama
from pydantic import BaseModel
import pytest
from baml_core.logger import logger

from baml_core.otel.tracer import _trace_internal
from baml_core.services.api import APIWrapper
from baml_core.services import api_types
import re

from baml_core.otel import flush_trace_logs, add_message_transformer_hook
from .ipc_channel import BaseIPCChannel, IPCChannel, NoopIPCChannel


class GlooTestCaseBase(typing_extensions.TypedDict):
    name: str


T = typing.TypeVar("T", bound=GlooTestCaseBase)


class TestRunMeta(BaseModel):
    dashboard_url: str


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


def _to_filters(f: str) -> str:
    # Filters are of the form <FunctionName>:<ImplName>:<TestName>
    # We want to convert this to a regex that matches the test name

    if ":" in f:
        parts = f.split(":")
        if len(parts) != 3:
            raise Exception(
                f"Invalid filter: {f}. Filters must be of the form <FunctionName>:<ImplName>:<TestName>."
            )
        function_name, impl_name, test_name = parts
        return f"test_{test_name}\\[{function_name}-{impl_name}\\]"
    else:
        # Any of the parts can match
        return f"*{f}*"


def _to_regex_filter(f: str) -> str:
    parsed = _to_filters(f)
    # Replace * with .*
    parsed = parsed.replace("*", ".*")
    return f"(^{parsed}$)"


# See https://docs.pytest.org/en/7.1.x/_modules/_pytest/hookspec.html#pytest_runtestloop
class BamlPytestPlugin:
    def __init__(
        self,
        api: typing.Optional[APIWrapper],
        ipc_channel: typing.Optional[int],
        include_filters: typing.List[str] = [],
        exclude_filters: typing.List[str] = [],
    ) -> None:
        self.__gloo_tests: typing.Dict[str, TestCaseMetadata] = {}
        self.__completed_tests: typing.Set[str] = set()
        self.__api = api
        self.__dashboard_url: typing.Optional[str] = None
        self.__ipc = (
            NoopIPCChannel()
            if ipc_channel is None
            else IPCChannel(host="127.0.0.1", port=ipc_channel)
        )
        self.__include_filters = (
            "|".join(map(_to_regex_filter, include_filters)) or None
        )
        self.__exclude_filters = (
            "|".join(map(_to_regex_filter, exclude_filters)) or None
        )

        if ipc_channel is not None:
            add_message_transformer_hook(lambda log: self.__ipc.send("log", log))

    def __test_matches_filter(self, test_name: str) -> bool:
        # Check exclude filters first
        if self.__exclude_filters is not None:
            if re.match(self.__exclude_filters, test_name):
                return False
        if self.__include_filters is not None:
            if re.match(self.__include_filters, test_name):
                return True
            else:
                return False
        # If we get here, we have no include filters, so we should return true
        return True

    @pytest.hookimpl(tryfirst=True)
    def pytest_generate_tests(self, metafunc: pytest.Metafunc) -> None:
        for marker in metafunc.definition.iter_markers("baml_function_test"):
            owner = marker.kwargs["owner"]
            if marker.kwargs["impls"]:
                metafunc.parametrize(
                    f"{owner.name}Impl",
                    list(map(lambda x: owner.get_impl(x).run, marker.kwargs["impls"])),
                    ids=map(lambda x: f"{owner.name}-{x}", marker.kwargs["impls"]),
                )
            else:
                # Remove the test if no impls are specified
                metafunc.parametrize(
                    f"{owner.name}Impl",
                    [None],
                    ids=[f"{owner.name}-SKIPPED"],
                )
        for marker in metafunc.definition.iter_markers("baml_function_stream_test"):
            owner = marker.kwargs["owner"]
            if marker.kwargs["impls"]:
                metafunc.parametrize(
                    f"{owner.name}Impl",
                    list(
                        map(lambda x: owner.get_impl(x).stream, marker.kwargs["impls"])
                    ),
                    ids=map(lambda x: f"{owner.name}-{x}", marker.kwargs["impls"]),
                )
            else:
                # Remove the test if no impls are specified
                metafunc.parametrize(
                    f"{owner.name}Impl",
                    [None],
                    ids=[f"{owner.name}-SKIPPED"],
                )

    @pytest.fixture
    def baml_ipc_channel(self) -> BaseIPCChannel:
        return self.__ipc

    @pytest.hookimpl(trylast=True)
    def pytest_collection_modifyitems(
        self, config: pytest.Config, items: typing.List[pytest.Item]
    ) -> None:
        filtered_items = [
            item for item in items if self.__test_matches_filter(item.name)
        ]

        for item in filtered_items:
            for k in ["baml_function_test", "baml_function_stream_test"]:
                if k in item.keywords:
                    item.add_marker("baml_test")
                    # Add more keywords here:
                    kwargs = item.keywords[k].kwargs
                    impls = kwargs.get("impls", [])
                    if not impls:
                        item.add_marker(pytest.mark.skip(reason="No impls specified"))

            if "baml_test" in item.keywords:
                if hasattr(item, "function") and asyncio.iscoroutinefunction(
                    item.function
                ):
                    item.add_marker(pytest.mark.asyncio)

            # if not self.__test_matches_filter(item.name):
            #     item.add_marker(pytest.mark.skip(reason="BAML Filter does not match"))
        items[:] = filtered_items

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

    def maybe_start_logging(self, session: pytest.Session) -> None:
        if self.__api is None:
            return

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

        self.__dashboard_url = self.__api.test.create_session()

        for dataset_name, test_cases in dataset_cases.items():
            for test_name, case_names in test_cases.items():
                self.__api.test.create_cases(
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

        self.maybe_start_logging(session)
        if self.__dashboard_url:
            self.__ipc.send("test_url", TestRunMeta(dashboard_url=self.__dashboard_url))
            print(
                f"View test results at {colorama.Fore.CYAN}{self.__dashboard_url}{colorama.Fore.RESET}"
            )

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
            payload = api_types.UpdateTestCase(
                test_dataset_name=item.dataset_name,
                test_case_definition_name=item.test_name,
                test_case_arg_name=item.case_name,
                status=api_types.TestCaseStatus.RUNNING,
                error_data=None,
            )

            # Log the start of the test
            self.__ipc.send("update_test_case", payload)
            if self.__api is not None:
                self.__api.test.update_case_sync(payload=payload)

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
            test_cycle_id=self.__api.test.session_id if self.__api else "local-run",
            test_dataset_name=meta.dataset_name,
        )

        item.obj = _trace_internal(item.obj, __tags__=tags)  # type: ignore

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
            payload = api_types.UpdateTestCase(
                test_dataset_name=meta.dataset_name,
                test_case_definition_name=meta.test_name,
                test_case_arg_name=meta.case_name,
                status=status,
                error_data={"error": str(call.excinfo.value)} if call.excinfo else None,
            )

            self.__ipc.send("update_test_case", payload)
            if self.__api is not None:
                self.__api.test.update_case_sync(payload=payload)
            self.__completed_tests.add(item.nodeid)

    @pytest.hookimpl(tryfirst=True)
    def pytest_sessionfinish(
        self,
        session: pytest.Session,
        exitstatus: typing.Union[int, pytest.ExitCode],
    ) -> None:
        if session.config.option.collectonly:
            return

        logger.info("Flushing trace logs..")
        flush_trace_logs()

        if (
            session.testsfailed
            and not session.config.option.continue_on_collection_errors
        ):
            if self.__dashboard_url:
                print(
                    f"View test results at {colorama.Fore.CYAN}{self.__dashboard_url}{colorama.Fore.RESET}"
                )
            return

        try:
            for nodeid, meta in self.__gloo_tests.items():
                if nodeid not in self.__completed_tests:
                    payload = api_types.UpdateTestCase(
                        test_dataset_name=meta.dataset_name,
                        test_case_definition_name=meta.test_name,
                        test_case_arg_name=meta.case_name,
                        status=api_types.TestCaseStatus.CANCELLED,
                        error_data=None,
                    )
                    self.__ipc.send("update_test_case", payload)
                    if self.__api is not None:
                        self.__api.test.update_case_sync(payload=payload)
        except Exception as e:
            # If we don't catch this the user is not able to see any other underlying test errors.
            logger.error(f"Failed to update test case status: {e}")

        if self.__dashboard_url:
            print(
                f"View test results at {colorama.Fore.CYAN}{self.__dashboard_url}{colorama.Fore.RESET}"
            )
