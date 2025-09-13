from ..baml_client.config import set_log_level, get_log_level
import os
import sys

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
async def test_logger(capfd: pytest.CaptureFixture[str]):
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
async def test_logger_initializes_correctly(capfd: pytest.CaptureFixture[str]):
    # default if not set should be INFO
    # make sure BAML_LOG is not set in infisical when running this test.
    starter = os.environ.get("BAML_LOG")
    assert starter is None or starter == "INFO", (
        "BAML_LOG should be INFO but was " + starter
    )
    assert get_log_level() == "INFO"
    result = await b.TestOpenAIShorthand("use the word 'fiscal'")
    assert get_log_level() == "INFO"
    assert "fiscal" in result.lower()

    captured = capfd.readouterr()
    # assert captured.out == "hello\n"
    assert "PROMPT" in captured.out

    # Test with environment variable from dotenv, which sets BAML_LOG to warn
    # a caveat here is, log level is only set after a function call.
    loaded = load_dotenv(dotenv_path="./test-dotenv", override=True)
    assert loaded, "Failed to load dotenv file"
    assert os.environ.get("BAML_LOG") == "warn"
    result = await b.TestOpenAIShorthand("use the word 'fiscal'")
    assert get_log_level() == "WARN"
    assert "fiscal" in result.lower()

    # Check captured output with capfd
    captured = capfd.readouterr()
    # At WARN level, we shouldn't see PROMPT logs
    assert "PROMPT" not in captured.out


@pytest.mark.asyncio
@pytest.mark.usefixtures("reset_log_level")
async def test_fallback_errors():
    try:
        await b.TestFallbackAlwaysFails("lorem ipsum")
    except Exception as e:
        assert "openai/gpt-0-noexist" in str(e)
        assert "openai/gpt-1-noexist" in str(e)
        assert "openai/gpt-2-noexist" in str(e)
    assert False


@pytest.mark.asyncio
@pytest.mark.usefixtures("reset_log_level")
async def test_capture_stdout_fallback_always_fails():
    # 1️⃣ Create a pipe (r = read end, w = write end)
    r_fd, w_fd = os.pipe()

    # 2️⃣ Save the original stdout fd so we can restore it later
    saved_stdout_fd = os.dup(sys.stdout.fileno())

    # 3️⃣ Replace stdout (fd 1) with the write end of the pipe
    os.dup2(w_fd, sys.stdout.fileno())
    os.close(w_fd)  # the duplicated fd (1) is now the write end

    # ---- from here on, everything that goes to fd 1 ends up in the pipe ----
    print("Hello from print()")
    os.write(1, b"Hello from os.write()\n")  # <-- also captured
    await b.TestFallbackAlwaysFails("use the word 'fiscal'")

    # 4️⃣ Flush Python's buffered stdout so the pipe receives everything
    sys.stdout.flush()

    # 5️⃣ Restore the original stdout
    os.dup2(saved_stdout_fd, sys.stdout.fileno())
    os.close(saved_stdout_fd)

    # 6️⃣ Read the data from the pipe
    captured_bytes = b""
    while True:
        chunk = os.read(r_fd, 4096)
        if not chunk:
            break
        captured_bytes += chunk
    os.close(r_fd)

    assert "" == captured_bytes.decode()  # assume UTF‑8 text
