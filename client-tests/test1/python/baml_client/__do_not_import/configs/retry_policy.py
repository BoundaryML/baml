# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.
#
# BAML version: 0.1.1-canary.7
# Generated Date: __DATE__
# Generated by: __USER__

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from baml_core.configs.retry_policy import create_retry_policy_constant_delay, create_retry_policy_exponential_backoff


DefaultRetryPolicy = create_retry_policy_constant_delay(
  max_retries=2,
  delay_ms=1
)
DelayRetryPolicy = create_retry_policy_exponential_backoff(
  max_retries=2,
  delay_ms=1,
  max_delay_ms=10000,
  multiplier=1.5
)


__all__ = [
    'create_retry_policy_constant_delay',
    'create_retry_policy_exponential_backoff'
]
