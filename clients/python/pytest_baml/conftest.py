import pytest
from baml_core.logger import logger
from baml_lib import baml_init
from .pytest_baml import BamlPytestPlugin


def pytest_addoption(parser: pytest.Parser) -> None:
    # To be deprecated in favor of baml-ipc
    parser.addoption(
        "--pytest-baml-ipc",
        action="store",
        dest="baml_ipc",
        default=None,
        type=int,
        help="The name of the ipc pipe to communicate on.",
    )
    # Making it more generic.
    parser.addoption(
        "--baml-ipc",
        action="store",
        dest="baml_ipc",
        default=None,
        type=int,
        help="The name of the ipc pipe to communicate on.",
    )

    # Add --pytest-baml-include and --pytest-baml-exclude options here
    # Which can be used multiple times
    parser.addoption(
        "--pytest-baml-include",
        action="append",
        dest="baml_include",
        default=[],
        help="Filter for excluding and including tests.",
    )
    parser.addoption(
        "--pytest-baml-exclude",
        action="append",
        dest="baml_exclude",
        default=[],
        help="Filter for excluding and including tests.",
    )


def pytest_configure(config: pytest.Config) -> None:
    logger.debug("Registering pytest_gloo plugin.")
    config.addinivalue_line(
        "markers",
        "baml_test: mark test as a BAML test to upload data to the BAML dashboard",
    )
    # BAML INIT here
    # Add optional stage parameter to baml_init
    # baml_init(), returns api wrapper we can use
    baml_conf = baml_init(stage="test", enable_cache=True)

    # Get which tests to include/exclude

    config.pluginmanager.register(
        BamlPytestPlugin(
            api=baml_conf.api,
            # We can for now assume that the options are only passed by baml test, which ensures correct types
            ipc_channel=config.getoption("baml_ipc"),
            include_filters=config.getoption("baml_include"),
            exclude_filters=config.getoption("baml_exclude"),
        ),
        "pytest_baml",
    )
