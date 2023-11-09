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

from baml_lib._impl.deserializer import register_deserializer
from enum import Enum


@register_deserializer({  })
class Sentiment(str, Enum):
    Positive = "Positive"
    Negative = "Negative"
    Neutral = "Neutral"


__all__ = [
    'register_deserializer'
]
