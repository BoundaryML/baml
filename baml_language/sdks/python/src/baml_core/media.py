"""Python implementations of BAML stdlib media wrapper types."""

from __future__ import annotations

from typing import Any, ClassVar, Optional, TypeVar

from . import baml_py
from .baml_py import BamlPyHandle
from .cffi.v1 import baml_inbound_pb2

SelfMedia = TypeVar("SelfMedia", bound="_BamlMedia")


class _BamlMedia:
    __slots__ = ("_handle",)

    _handle_type: ClassVar[int]

    def __init__(self, handle: BamlPyHandle) -> None:
        self._handle = handle

    @classmethod
    def from_url(
        cls: type[SelfMedia],
        url: str,
        mime_type: Optional[str] = None,
    ) -> SelfMedia:
        return cls._from_pyhandle(baml_py._media_from_url(cls._handle_type, url, mime_type))

    @classmethod
    def from_file(
        cls: type[SelfMedia],
        file: str,
        mime_type: Optional[str] = None,
    ) -> SelfMedia:
        return cls._from_pyhandle(baml_py._media_from_file(cls._handle_type, file, mime_type))

    @classmethod
    def from_base64(
        cls: type[SelfMedia],
        base64: str,
        mime_type: Optional[str] = None,
    ) -> SelfMedia:
        return cls._from_pyhandle(
            baml_py._media_from_base64(cls._handle_type, base64, mime_type)
        )

    def url(self) -> Optional[str]:
        return baml_py._media_url(self._handle, self._handle_type)

    def file(self) -> Optional[str]:
        return baml_py._media_file(self._handle, self._handle_type)

    def base64(self) -> str:
        return baml_py._media_base64(self._handle, self._handle_type)

    def mime_type(self) -> Optional[str]:
        return baml_py._media_mime_type(self._handle, self._handle_type)

    @classmethod
    def _from_pyhandle(cls: type[SelfMedia], pyhandle: BamlPyHandle) -> SelfMedia:
        baml_py._media_validate(pyhandle, cls._handle_type)
        return cls(pyhandle)

    def _to_pyhandle(self) -> BamlPyHandle:
        return self._handle

    @classmethod
    def __get_pydantic_core_schema__(
        cls,
        _source_type: Any,
        _handler: Any,
    ) -> Any:
        from pydantic_core import core_schema

        return core_schema.is_instance_schema(cls)


class BamlImage(_BamlMedia):
    _handle_type = baml_inbound_pb2.BamlHandleType.ADT_MEDIA_IMAGE


class BamlAudio(_BamlMedia):
    _handle_type = baml_inbound_pb2.BamlHandleType.ADT_MEDIA_AUDIO


class BamlVideo(_BamlMedia):
    _handle_type = baml_inbound_pb2.BamlHandleType.ADT_MEDIA_VIDEO


class BamlPdf(_BamlMedia):
    _handle_type = baml_inbound_pb2.BamlHandleType.ADT_MEDIA_PDF


__all__ = ["BamlImage", "BamlAudio", "BamlVideo", "BamlPdf"]
