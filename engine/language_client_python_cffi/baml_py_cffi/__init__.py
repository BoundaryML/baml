"""BAML Python CFFI Client - A new Python client using CFFI for BAML runtime"""

from ._ffi import version
from ._lib import set_shared_library_path

__version__ = "0.205.0"
__all__ = ["version", "set_shared_library_path"]