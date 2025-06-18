from __future__ import annotations
import os
import warnings

from baml_py import BamlCtxManager, BamlRuntime
from .inlinedbaml import get_baml_files
from typing import Dict

DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME = BamlRuntime.from_files(
  "baml_src",
  get_baml_files(),
  os.environ.copy()
)
DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_CTX = BamlCtxManager(DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME)

def reset_baml_env_vars(env_vars: Dict[str, str]):
    warnings.warn(
        "reset_baml_env_vars is deprecated and should be removed. Environment variables are now lazily loaded on each function call",
        DeprecationWarning,
        stacklevel=2
    )

__all__ = []
