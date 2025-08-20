"""BAML Python CFFI Client - A new Python client using CFFI for BAML runtime"""

from ._ffi import version
from ._lib import set_shared_library_path

# Phase 2 imports (for testing, will be part of runtime in Phase 3)
from ._callbacks import register_callbacks, CallbackState
from ._async_utils import make_async_call

__version__ = "0.205.0"
__all__ = ["version", "set_shared_library_path"]