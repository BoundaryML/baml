from __future__ import annotations

import pydantic


class Address(pydantic.BaseModel): ...


class PhoneNumber(pydantic.BaseModel): ...


class Resume(pydantic.BaseModel): ...


__all__ = [
    "Address",
    "PhoneNumber",
    "Resume",
]
