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

from  ..configs.retry_policy import DefaultRetryPolicy
from baml_core.provider_manager import LLMManager
from os import environ


AZURE_YES_NO = LLMManager.add_llm(
    name="AZURE_YES_NO",
    provider="baml-openai-chat",
    retry_policy=DefaultRetryPolicy,
    options=dict(
        model="gpt-3.5-turbo",
        api_key=environ['OPENAI_API_KEY'],
        request_timeout=45,
        max_tokens=400,
    ),
)


__all__ = [
    'LLMManager'
]
