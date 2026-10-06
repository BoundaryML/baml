"""An error of a spawned task that no code awaits ends the host process.

That is the default of the Python bridge. A host replaces it with
`baml_bridge.set_unhandled_spawn_error_handler` (see
`bridge_tests/test_process_lifecycle.py`). The default ends the process, so
the call runs in a child process.
"""

import os
import signal
import subprocess
import sys

UNHANDLED_SPAWN_SCRIPT = """\
import time

import baml_sdk
from baml_bridge import shutdown_runtime

assert baml_sdk.spawn_unhandled_error() == 1
shutdown_runtime()
time.sleep(1)
raise SystemExit(42)
"""


# SDK_PARITY_LINT(skip): requires subprocess-level SDK harness support
def test_unhandled_spawn_error_uses_host_default():
    result = subprocess.run(
        [sys.executable, "-c", UNHANDLED_SPAWN_SCRIPT],
        capture_output=True,
        text=True,
        check=False,
        timeout=60,
    )

    expected_returncode = signal.SIGTERM if os.name == "nt" else 1
    assert result.returncode == expected_returncode, result.stderr
    assert "boom" in result.stderr
