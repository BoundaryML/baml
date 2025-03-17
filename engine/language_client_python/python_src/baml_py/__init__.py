# BAML Python API: provides the Python API for the BAML runtime.

if __name__ == "baml_py":
    import os

    if "BAML_LOG" not in os.environ:
        os.environ["BAML_LOG"] = "info"

# Re-export the pyo3 API
from .baml_py import (
    BamlRuntime,
    FunctionResult,
    FunctionResultStream,
    BamlImagePy as Image,
    BamlAudioPy as Audio,
    invoke_runtime_cli,
    ClientRegistry,
    # Collector utilities
    Collector,
    FunctionLog,
    LLMCall,
    Timing,
    Usage,
    HTTPRequest,
)
from .stream import BamlStream, BamlSyncStream
from .ctx_manager import CtxManager as BamlCtxManager

__all__ = [
    "BamlRuntime",
    "ClientRegistry",
    "BamlStream",
    "BamlSyncStream",
    "BamlCtxManager",
    "FunctionResult",
    "FunctionResultStream",
    "Image",
    "Audio",
    "invoke_runtime_cli",
    # Collector types
    "Collector",
    "FunctionLog",
    "LLMCall",
    "Timing",
    "Usage",
    "HTTPRequest",
]
