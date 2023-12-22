# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from .cls_zenfetchbotdocumentbase import ZenfetchBotDocumentBase
from baml_lib._impl.deserializer import register_deserializer
from pydantic import BaseModel
from typing import List


@register_deserializer({  })
class ZenfetchBotDocumentBaseList(BaseModel):
    list_of_documents: List[ZenfetchBotDocumentBase]
    @property
    def display(self) -> str:
        ret = []
        if self.list_of_documents:
            for doc in list_of_documents:
                ret.append(doc.display)
        return "\n".join(ret)
