"""BAML_LOG-gated delivery of BAML structured logs to the host's stderr.

The bridge reads BAML_LOG at the start of each function call, so these tests
toggle it in-process. ``capfd`` captures at the file-descriptor level, which
is where the native bridge writes.
"""

import sys

import pytest

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk import emit_logs

# os.environ changes made after process start are not visible to the native
# bridge on Windows (putenv only updates the CRT copy of the environment).
pytestmark = pytest.mark.skipif(
    sys.platform == "win32",
    reason="in-process os.environ changes are invisible to the native bridge on Windows",
)


# SDK_PARITY_LINT(skip): requires subprocess-level SDK harness support
def test_baml_log_env_var_streams_logs_to_stderr(capfd, monkeypatch):
    monkeypatch.setenv("BAML_LOG", "info")
    assert emit_logs("py-log-marker") == "py-log-marker"
    err = capfd.readouterr().err
    assert "[INFO] info py-log-marker" in err
    assert "[WARN] warn py-log-marker" in err
    assert "[ERROR] error py-log-marker" in err
    # debug is below the requested info threshold.
    assert "debug py-log-marker" not in err


# SDK_PARITY_LINT(skip): requires subprocess-level SDK harness support
def test_baml_logs_stay_off_without_baml_log(capfd, monkeypatch):
    monkeypatch.delenv("BAML_LOG", raising=False)
    assert emit_logs("py-quiet-marker") == "py-quiet-marker"
    assert "py-quiet-marker" not in capfd.readouterr().err
