from __future__ import annotations

import typing
import pydantic

if typing.TYPE_CHECKING:
    from ... import lorem


class Address(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    street: typing.Union[str, None]
    city: typing.Union[str, None]
    zip: typing.Union[str, None]


class PhoneNumber(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    country_code: typing.Union[int, None]
    digits: typing.Union[str, None]


class Resume(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    name: typing.Union[str, None]
    email: typing.Union[str, None]
    addresses: typing.List[Address]
    scores: typing.Dict[str, int]
    sentiment: typing.Union[lorem.Sentiment, None]
    contact: typing.Union[PhoneNumber, None]


__all__ = [
    "Address",
    "PhoneNumber",
    "Resume",
]
