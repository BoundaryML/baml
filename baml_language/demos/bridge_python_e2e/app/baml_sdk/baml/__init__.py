from __future__ import annotations

from . import errors, fs, http, llm, media, net, panics, stream

import typing
import pydantic

from baml.baml_core import (
    define_instance_method as _define_instance_method,
    define_static_method as _define_static_method,
)


class String(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")

    length       = _define_instance_method("baml.String.length", "sync",  ["self"])
    length_async = _define_instance_method("baml.String.length", "async", ["self"])

    toLowerCase       = _define_instance_method("baml.String.toLowerCase", "sync",  ["self"])
    toLowerCase_async = _define_instance_method("baml.String.toLowerCase", "async", ["self"])

    toUpperCase       = _define_instance_method("baml.String.toUpperCase", "sync",  ["self"])
    toUpperCase_async = _define_instance_method("baml.String.toUpperCase", "async", ["self"])

    trim       = _define_instance_method("baml.String.trim", "sync",  ["self"])
    trim_async = _define_instance_method("baml.String.trim", "async", ["self"])

    includes       = _define_instance_method("baml.String.includes", "sync",  ["self", "search"])
    includes_async = _define_instance_method("baml.String.includes", "async", ["self", "search"])

    startsWith       = _define_instance_method("baml.String.startsWith", "sync",  ["self", "prefix"])
    startsWith_async = _define_instance_method("baml.String.startsWith", "async", ["self", "prefix"])

    endsWith       = _define_instance_method("baml.String.endsWith", "sync",  ["self", "suffix"])
    endsWith_async = _define_instance_method("baml.String.endsWith", "async", ["self", "suffix"])

    split       = _define_instance_method("baml.String.split", "sync",  ["self", "delimiter"])
    split_async = _define_instance_method("baml.String.split", "async", ["self", "delimiter"])

    substring       = _define_instance_method("baml.String.substring", "sync",  ["self", "start", "end"])
    substring_async = _define_instance_method("baml.String.substring", "async", ["self", "start", "end"])

    replace       = _define_instance_method("baml.String.replace", "sync",  ["self", "search", "replacement"])
    replace_async = _define_instance_method("baml.String.replace", "async", ["self", "search", "replacement"])

    indexOf       = _define_instance_method("baml.String.indexOf", "sync",  ["self", "search"])
    indexOf_async = _define_instance_method("baml.String.indexOf", "async", ["self", "search"])

    charAt       = _define_instance_method("baml.String.charAt", "sync",  ["self", "index"])
    charAt_async = _define_instance_method("baml.String.charAt", "async", ["self", "index"])

    matches       = _define_instance_method("baml.String.matches", "sync",  ["self", "pattern"])
    matches_async = _define_instance_method("baml.String.matches", "async", ["self", "pattern"])

    replaceAll       = _define_instance_method("baml.String.replaceAll", "sync",  ["self", "search", "replacement"])
    replaceAll_async = _define_instance_method("baml.String.replaceAll", "async", ["self", "search", "replacement"])


class Uint8Array(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")

    zeroes       = staticmethod(_define_static_method("baml.Uint8Array.zeroes", "sync",  ["size"]))
    zeroes_async = staticmethod(_define_static_method("baml.Uint8Array.zeroes", "async", ["size"]))

    from_array       = staticmethod(_define_static_method("baml.Uint8Array.from_array", "sync",  ["array"]))
    from_array_async = staticmethod(_define_static_method("baml.Uint8Array.from_array", "async", ["array"]))

    from_hex       = staticmethod(_define_static_method("baml.Uint8Array.from_hex", "sync",  ["hex"]))
    from_hex_async = staticmethod(_define_static_method("baml.Uint8Array.from_hex", "async", ["hex"]))

    from_base64       = staticmethod(_define_static_method("baml.Uint8Array.from_base64", "sync",  ["base64"]))
    from_base64_async = staticmethod(_define_static_method("baml.Uint8Array.from_base64", "async", ["base64"]))

    length       = _define_instance_method("baml.Uint8Array.length", "sync",  ["self"])
    length_async = _define_instance_method("baml.Uint8Array.length", "async", ["self"])

    at       = _define_instance_method("baml.Uint8Array.at", "sync",  ["self", "index"])
    at_async = _define_instance_method("baml.Uint8Array.at", "async", ["self", "index"])

    push       = _define_instance_method("baml.Uint8Array.push", "sync",  ["self", "item"])
    push_async = _define_instance_method("baml.Uint8Array.push", "async", ["self", "item"])

    pop       = _define_instance_method("baml.Uint8Array.pop", "sync",  ["self"])
    pop_async = _define_instance_method("baml.Uint8Array.pop", "async", ["self"])

    concat       = _define_instance_method("baml.Uint8Array.concat", "sync",  ["self", "other"])
    concat_async = _define_instance_method("baml.Uint8Array.concat", "async", ["self", "other"])

    includes       = _define_instance_method("baml.Uint8Array.includes", "sync",  ["self", "item"])
    includes_async = _define_instance_method("baml.Uint8Array.includes", "async", ["self", "item"])

    reverse       = _define_instance_method("baml.Uint8Array.reverse", "sync",  ["self"])
    reverse_async = _define_instance_method("baml.Uint8Array.reverse", "async", ["self"])

    slice       = _define_instance_method("baml.Uint8Array.slice", "sync",  ["self", "start", "end"])
    slice_async = _define_instance_method("baml.Uint8Array.slice", "async", ["self", "start", "end"])

    to_array       = _define_instance_method("baml.Uint8Array.to_array", "sync",  ["self"])
    to_array_async = _define_instance_method("baml.Uint8Array.to_array", "async", ["self"])

    to_hex       = _define_instance_method("baml.Uint8Array.to_hex", "sync",  ["self"])
    to_hex_async = _define_instance_method("baml.Uint8Array.to_hex", "async", ["self"])

    to_base64       = _define_instance_method("baml.Uint8Array.to_base64", "sync",  ["self"])
    to_base64_async = _define_instance_method("baml.Uint8Array.to_base64", "async", ["self"])

    sort       = _define_instance_method("baml.Uint8Array.sort", "sync",  ["self"])
    sort_async = _define_instance_method("baml.Uint8Array.sort", "async", ["self"])


__all__ = [
    "String",
    "Uint8Array",
]
