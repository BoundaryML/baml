# ruff: noqa: E402

import dotenv

dotenv.load_dotenv(dotenv_path=dotenv.find_dotenv(usecwd=True))


from baml_version import __version__
from .helpers import baml_init
from ._impl.deserializer import DeserializerException


__all__ = ["__version__", "baml_init", "DeserializerException"]

import typing
from pydantic import BaseModel
from enum import Enum


class MyEnum(str, Enum):
    FOO = "foo"
    BAR = "bar"


class Foo(BaseModel):
    bar: str
    em2: MyEnum = MyEnum.FOO


foo = Foo(bar="baz")


class Bar(BaseModel):
    foo: typing.List[Foo]
    en: MyEnum = MyEnum.BAR


bar = Bar(foo=[foo])


bar.model_fields.keys()

bar.model_dump_json()
