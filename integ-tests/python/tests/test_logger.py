from ..baml_client import b
from ..baml_client.logging import set_log_level, get_log_level
import pytest
import io
import contextlib


@pytest.fixture(scope="function")
def reset_log_level():
    previous_level = logging.get_log_level()
    yield
    logging.set_log_level(previous_level)

@pytest.mark.asyncio
@pytest.mark.usefixtures("reset_log_level")
async def test_logger():
    """
    Test that the logger works as expected.

    We need to actually run this test manually, as rust
    prints to stdout directly, and we can't capture it.
    """
    set_log_level("INFO")
    assert get_log_level() == "INFO"
    # capture the output
    captured_output = io.StringIO()
    with contextlib.redirect_stdout(captured_output):
        result = await b.TestOllama("banks using the word 'fiscal'")
        assert "fiscal" in result.lower()
        assert captured_output.getvalue() == ""

    set_log_level("WARN")
    assert get_log_level() == "WARN"
    captured_output = io.StringIO()
    with contextlib.redirect_stdout(captured_output):
        result = await b.TestOllama("banks using the word 'fiscal'")
        assert "fiscal" in result
        assert captured_output.getvalue() == ""

    set_log_level("OFF")
    assert get_log_level() == "OFF"
    captured_output = io.StringIO()
    with contextlib.redirect_stdout(captured_output):
        result = await b.TestOllama("banks using the word 'fiscal'")
        assert "fiscal" in result
        assert captured_output.getvalue() == ""

    
    set_log_level("INFO")
    assert get_log_level() == "INFO"
    captured_output = io.StringIO()
    with contextlib.redirect_stdout(captured_output):
        result = await b.TestOllama("banks using the word 'fiscal'")
        assert "fiscal" in result
        assert captured_output.getvalue() == ""
