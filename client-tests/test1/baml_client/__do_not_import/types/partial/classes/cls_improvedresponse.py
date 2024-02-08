# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from ...enums.enm_sentiment import Sentiment
from baml_lib._impl.deserializer import register_deserializer
from pydantic import BaseModel
from typing import Optional


@register_deserializer({  })
class PartialImprovedResponse(BaseModel):
    should_improve: Optional[bool] = None
    improved_response: Optional[str] = None
    field: Optional[Sentiment] = None
