"""Stored dataclass fields and optional HTTP summaries for host capture."""

import dataclasses
import types
from urllib.parse import urlsplit, urlunsplit

try:
    import requests
except ImportError:
    requests = None

try:
    import httpx
except ImportError:
    httpx = None

NO_PROJECTION = object()


def dataclass_values(value, fields, mro):
    stored = {}
    for base in mro:
        descriptor = type.__dict__["__dict__"].__get__(base).get("__dict__")
        if type(descriptor) is types.GetSetDescriptorType:
            stored = descriptor.__get__(value)
            break
    result = {}
    for name, field in dict.items(fields):
        if (
            type(field) is not dataclasses.Field
            or field._field_type is not dataclasses._FIELD
        ):
            continue
        if name in stored:
            result[name] = dict.get(stored, name)
            continue
        for base in mro:
            descriptor = type.__dict__["__dict__"].__get__(base).get(name)
            if type(descriptor) is types.MemberDescriptorType:
                result[name] = descriptor.__get__(value)
                break
    return result


def _url(value):
    if type(value) is not str:
        return None
    if len(value) > 65536:
        return {"$truncated": "bytes"}
    parts = urlsplit(value)
    # HTTP summaries omit credentials, query parameters and fragments.
    return urlunsplit(
        (parts.scheme, parts.netloc.rsplit("@", 1)[-1], parts.path, "", "")
    )


def http_summary(value):
    kind = type(value)
    if requests is not None:
        if kind is requests.Request or kind is requests.PreparedRequest:
            stored = object.__getattribute__(value, "__dict__")
            return {"method": stored.get("method"), "url": _url(stored.get("url"))}
        if kind is requests.Response:
            stored = object.__getattribute__(value, "__dict__")
            request = http_summary(stored.get("request"))
            return {
                "status_code": stored.get("status_code"),
                "url": _url(stored.get("url")),
                "request": request if request is not NO_PROJECTION else None,
            }
    if httpx is not None:
        if kind is httpx.Request:
            stored = object.__getattribute__(value, "__dict__")
            url = stored.get("url")
            return {
                "method": stored.get("method"),
                "url": _url(httpx.URL.__str__(url)) if type(url) is httpx.URL else None,
            }
        if kind is httpx.Response:
            stored = object.__getattribute__(value, "__dict__")
            request = http_summary(stored.get("_request"))
            return {
                "status_code": stored.get("status_code"),
                "request": request if request is not NO_PROJECTION else None,
            }
    return NO_PROJECTION
