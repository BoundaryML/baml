import pytest
from baml_core.logger import logger
from baml_lib import baml_init
from .pytest_baml import BamlPytestPlugin


baml_test = pytest.mark.baml_test


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
    if baml_conf.api is None:
        logger.warn(
            "BAML plugin disabled due to missing environment variables. Did you set GLOO_APP_ID and GLOO_APP_SECRET?"
        )
        return

    config.pluginmanager.register(BamlPytestPlugin(api=baml_conf.api), "pytest_baml")
