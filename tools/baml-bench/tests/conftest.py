"""Pytest configuration: dummy env for app imports + the integration marker.

Service modules read config from ``os.environ`` at import time (the api needs
``CONVEX_URL``), so set harmless placeholders here for the fast unit tests. The
end-to-end integration harness (the ``bench_stack`` fixture that boots the stack)
lands with the e2e tests in a later push; for now only the marker is registered.
"""

from __future__ import annotations

import os

os.environ.setdefault("CONVEX_URL", "http://localhost:3210")
os.environ.setdefault("SERVICE_URL", "http://localhost:8080")
os.environ.setdefault("SERVICE_TOKEN", "devservicetoken")
os.environ.setdefault("CLAUDE_PROXY_TOKEN", "devproxytoken")

import pytest


def pytest_configure(config: pytest.Config) -> None:
    config.addinivalue_line(
        "markers", "integration: end-to-end test that boots the stack (needs Docker)"
    )
