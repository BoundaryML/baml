"""BAML Python CFFI Client - A new Python client using CFFI for BAML runtime"""

from typing import Dict
from ._ffi import version
from ._lib import set_shared_library_path
from .runtime import BamlRuntime

__version__ = "0.205.0"
__all__ = ["BamlRuntime", "version", "create_runtime", "set_shared_library_path"]


# Simple creation function matching Go API
def create_runtime(
    root_path: str, src_files: Dict[str, str], env_vars: Dict[str, str]
) -> BamlRuntime:
    """Create a new BAML runtime"""
    return BamlRuntime(root_path, src_files, env_vars)
