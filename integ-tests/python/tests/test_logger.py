from ..baml_client.config import set_log_level, get_log_level
import os

import pytest

from ..baml_client import b

from dotenv import load_dotenv


@pytest.fixture(scope="function")
def reset_log_level():
    previous_level = get_log_level()
    yield
    set_log_level(previous_level)


@pytest.mark.asyncio
@pytest.mark.usefixtures("reset_log_level")
async def test_logger(capfd):
    """
    Test that the logger works as expected.

    We need to actually run this test manually, as rust
    prints to stdout directly, and we can't capture it.
    """

    async def test_log_level(level: str):
        set_log_level(level)
        assert get_log_level() == level

        result = await b.TestOpenAIShorthand("banks using the word 'fiscal'")
        assert "fiscal" in result.lower()

        captured = capfd.readouterr()
        if level == "INFO":
            assert "PROMPT" in captured.out
        else:
            assert "PROMPT" not in captured.out

    await test_log_level("INFO")
    await test_log_level("WARN")
    await test_log_level("INFO")
    await test_log_level("OFF")
    await test_log_level("INFO")


@pytest.mark.asyncio
@pytest.mark.usefixtures("reset_log_level")
async def test_logger_initializes_correctly(capfd):
    # default if not set should be INFO
    # make sure BAML_LOG is not set in infisical when running this test.
    assert os.environ.get("BAML_LOG") is None
    assert get_log_level() == "INFO"
    print("logger test")
    result = await b.TestOpenAIShorthand("use the word 'fiscal'")
    assert "fiscal" in result.lower()

    captured = capfd.readouterr()
    # assert captured.out == "hello\n"
    assert "PROMPT" in captured.out

    # Test with environment variable from dotenv, which sets BAML_LOG to warn
    load_dotenv(dotenv_path="./test-dotenv")
    assert os.environ.get("BAML_LOG") == "warn"
    # load_dotenv should trigger a re-read of the env var
    # so baml_client now knows about it
    assert get_log_level() == "WARN"

    result = await b.TestOpenAIShorthand("use the word 'fiscal'")
    assert "fiscal" in result.lower()

    # Check captured output with capfd
    captured = capfd.readouterr()
    # At WARN level, we shouldn't see PROMPT logs
    assert "PROMPT" not in captured.out
