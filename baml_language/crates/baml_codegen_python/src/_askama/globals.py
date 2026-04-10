from __future__ import annotations
import os
import warnings

try:
    from baml import BamlCtxManager, BamlRuntime
    from .inlinedbaml import get_baml_files
    DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME = BamlRuntime.from_files(
      "baml_src",
      get_baml_files(),
    )
    DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_CTX = BamlCtxManager(DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME)
except ImportError:
    DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME = None  # type: ignore[assignment]
    DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_CTX = None  # type: ignore[assignment]

from typing import Dict

def reset_baml_env_vars(env_vars: Dict[str, str]):
    warnings.warn(
        "reset_baml_env_vars is deprecated and should be removed. Environment variables are now lazily loaded on each function call",
        DeprecationWarning,
        stacklevel=2
    )

__all__ = []
