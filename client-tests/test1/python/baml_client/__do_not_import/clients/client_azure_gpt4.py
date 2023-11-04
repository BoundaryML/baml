# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.
#
# BAML version: 0.0.1
# Generated Date: __DATE__
# Generated by: __USER__

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long

from baml_core._impl.provider import LLMManager
from os import environ


AZURE_GPT4 = LLMManager.add_llm(
    name="AZURE_GPT4",
    provider="baml-openai-chat",
    retry_policy=None,
    options=dict(
        model="gpt-3.5-turbo",
        api_key=environ['OPENAI_API_KEY'],
        request_timeout=45,
        max_tokens=400,
    ),
)
