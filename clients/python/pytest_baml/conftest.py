import pytest
from baml_core.logger import logger
from baml_lib import baml_init
from .pytest_baml import BamlPytestPlugin


def pytest_addoption(parser: pytest.Parser) -> None:
    parser.addoption(
        "--pytest-baml-ipc",
        action="store",
        dest="baml_ipc",
        default=None,
        type=int,
        help="The name of the ipc pipe to communicate on.",
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
    config.pluginmanager.register(
        BamlPytestPlugin(api=baml_conf.api, ipc_channel=config.getoption("baml_ipc")),
        "pytest_baml",
    )
