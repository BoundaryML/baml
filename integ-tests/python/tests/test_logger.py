from ..baml_client import b
from ..baml_client.config import set_log_level, get_log_level
import pytest
import io
import contextlib


@pytest.fixture(scope="function")
def reset_log_level():
    previous_level = get_log_level()
    yield
    set_log_level(previous_level)

@pytest.mark.asyncio
@pytest.mark.usefixtures("reset_log_level")
async def test_logger():
    """
    Test that the logger works as expected.

    We need to actually run this test manually, as rust
    prints to stdout directly, and we can't capture it.
    """
    async def test_log_level(level: str):
        set_log_level(level)
        assert get_log_level() == level
        # capture the output
        captured_output = io.StringIO()
        with contextlib.redirect_stdout(captured_output):
            result = await b.TestOllamaHaiku("banks using the word 'fiscal'")
            assert "fiscal" in result.model_dump_json().lower()
            assert captured_output.getvalue() == ""

    await test_log_level("INFO")
    await test_log_level("WARN")
    await test_log_level("INFO")
    await test_log_level("OFF")
    await test_log_level("INFO")
