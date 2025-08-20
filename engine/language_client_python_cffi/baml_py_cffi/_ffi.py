import ctypes
from typing import Optional
from ._lib import load_library

# Load the library
_lib = load_library()

# Define function signatures
_lib.version.argtypes = []
_lib.version.restype = ctypes.c_char_p

def version() -> str:
    """Get BAML library version"""
    result = _lib.version()
    return result.decode('utf-8')