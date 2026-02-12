# BAML Python API: new system powered by bex_engine.

import atexit

from .baml_py import (
    BamlRuntime,
    FunctionResult,
    HostSpanManager,
    flush_events,
    get_version,
)
from .ctx_manager import CtxManager as BamlCtxManager

# Flush buffered trace events on process exit so nothing is lost.
atexit.register(flush_events)

__all__ = [
    "BamlRuntime",
    "FunctionResult",
    "HostSpanManager",
    "BamlCtxManager",
    "flush_events",
    "get_version",
]
