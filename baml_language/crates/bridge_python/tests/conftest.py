"""
Pytest configuration for bridge_python tests.

Before running these tests, build and install the native module:

    cd baml_language/crates/bridge_python
    uv run maturin develop --uv
    uv run pytest tests/ -v
"""

import pytest


# ============================================================================
# Cleanup fixture
# ============================================================================
@pytest.fixture(scope="session", autouse=True)
def flush_traces():
    """Ensure traces are flushed when pytest exits."""
    yield
    from baml_py import flush_events

    flush_events()
