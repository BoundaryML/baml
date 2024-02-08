# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from ..types.classes.cls_conversation import Conversation
from ..types.classes.cls_message import Message
from ..types.classes.cls_proposedmessage import ProposedMessage
from ..types.enums.enm_messagesender import MessageSender
from ..types.partial.classes.cls_conversation import PartialConversation
from ..types.partial.classes.cls_message import PartialMessage
from ..types.partial.classes.cls_proposedmessage import PartialProposedMessage
from baml_core.stream import AsyncStream
from baml_lib._impl.functions import BaseBAMLFunction
from typing import AsyncIterator, Callable, Protocol, runtime_checkable


IMaybePolishTextOutput = str

@runtime_checkable
class IMaybePolishText(Protocol):
    """
    This is the interface for a function.

    Args:
        arg: ProposedMessage

    Returns:
        str
    """

    async def __call__(self, arg: ProposedMessage, /) -> str:
        ...

   

@runtime_checkable
class IMaybePolishTextStream(Protocol):
    """
    This is the interface for a stream function.

    Args:
        arg: ProposedMessage

    Returns:
        AsyncStream[str, str]
    """

    def __call__(self, arg: ProposedMessage, /) -> AsyncStream[str, str]:
        ...
class IBAMLMaybePolishText(BaseBAMLFunction[str, str]):
    def __init__(self) -> None:
        super().__init__(
            "MaybePolishText",
            IMaybePolishText,
            ["v1", "v2"],
        )

    async def __call__(self, *args, **kwargs) -> str:
        return await self.get_impl("v1").run(*args, **kwargs)
    
    def stream(self, *args, **kwargs) -> AsyncStream[str, str]:
        res = self.get_impl("v1").stream(*args, **kwargs)
        return res

BAMLMaybePolishText = IBAMLMaybePolishText()

__all__ = [ "BAMLMaybePolishText" ]