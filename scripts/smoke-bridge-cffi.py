#!/usr/bin/env python3
"""Language-neutral smoke test for the `bridge_cffi` engine cdylib.

Usage: smoke-bridge-cffi.py <lib_path> <expected_version>

dlopens the shared library via stdlib ctypes, calls the C ABI `version()`
(which returns a `{ptr, len}` buffer owned by the engine), and asserts it
equals the canonical version this build was stamped to — the same handshake
every dylib-loader SDK performs after loading. Depends on no SDK crate, so the
Rust and (future) Go bridges can share it.
"""

from __future__ import annotations

import ctypes
import sys


class Buffer(ctypes.Structure):
    # Layout-identical to `bridge_cffi::Buffer` (a pointer + length returned
    # by value). `ptr` is c_void_p so ctypes does not copy/free it for us.
    _fields_ = [("ptr", ctypes.c_void_p), ("len", ctypes.c_size_t)]


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit("usage: smoke-bridge-cffi.py <lib_path> <expected_version>")
    lib_path, expected = sys.argv[1], sys.argv[2]

    lib = ctypes.CDLL(lib_path)
    lib.version.restype = Buffer
    lib.version.argtypes = []
    lib.free_buffer.restype = None
    lib.free_buffer.argtypes = [Buffer]

    buf = lib.version()
    version = ctypes.string_at(buf.ptr, buf.len).decode("utf-8")
    lib.free_buffer(buf)

    print(f"loaded library reports version: {version!r}")
    if version != expected:
        sys.exit(f"version mismatch: library {version!r} != expected {expected!r}")
    print("OK: dlopen + version() handshake passed")


if __name__ == "__main__":
    main()
