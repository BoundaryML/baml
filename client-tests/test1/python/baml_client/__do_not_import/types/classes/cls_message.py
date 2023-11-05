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
# fmt: off

from ..enums.enm_messagesender import MessageSender
from baml_lib._impl.deserializer import register_deserializer
from pydantic import BaseModel


@register_deserializer({ "sender1": "sender","body1": "body", })
class Message(BaseModel):
    sender: MessageSender
    body: str
