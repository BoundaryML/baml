from __future__ import annotations

from baml.baml_core import BamlRuntime
from .baml import _inlinedbaml

BamlRuntime.initialize_runtime(
    "baml_src", _inlinedbaml.FILES, sdk_root=__name__
)

from . import baml

import typing
import pydantic

from baml.baml_core import define_function as __define_function


class KitchenSink(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    id: int
    name: str
    score: float
    active: bool
    status: str
    priority: int
    tags: typing.List[str]
    numbers: typing.List[int]
    matrix: typing.List[typing.List[int]]
    metadata: typing.Dict[str, str]
    scores: typing.Dict[str, float]
    description: typing.Optional[str]
    data: typing.Union[str, int, DataObject]
    result: typing.Union[Success, Error]
    user: User
    items: typing.List[Item]
    config: Configuration


class DataObject(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    type_: str
    value: typing.Dict[str, str]


class Success(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    type_: str
    data: typing.Dict[str, str]


class Error(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    type_: str
    message: str
    code: int


class User(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    id: int
    profile: UserProfile
    settings: typing.Dict[str, Setting]


class UserProfile(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    name: str
    email: str
    bio: typing.Optional[str]
    links: typing.List[str]


class Setting(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    key: str
    value: typing.Union[str, int, bool]
    metadata: typing.Optional[typing.Dict[str, str]]


class Item(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    id: int
    name: str
    variants: typing.List[Variant]
    attributes: typing.Dict[str, typing.Union[str, int, float, bool]]


class Variant(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    sku: str
    price: float
    stock: int
    options: typing.Dict[str, str]


class Configuration(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    version: str
    features: typing.List[Feature]
    environments: typing.Dict[str, Environment]
    rules: typing.List[Rule]


class Feature(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    name: str
    enabled: bool
    config: typing.Optional[typing.Dict[str, typing.Union[str, int, bool]]]
    dependencies: typing.List[str]


class Environment(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    name: str
    url: str
    variables: typing.Dict[str, str]
    secrets: typing.Optional[typing.Dict[str, str]]


class Rule(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    id: int
    name: str
    condition: Condition
    actions: typing.List[Action]
    priority: int


class Condition(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    type_: str
    conditions: typing.List[typing.Union[Condition, SimpleCondition]]


class SimpleCondition(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    field: str
    operator: str
    value: typing.Union[str, int, float, bool]


class Action(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    type_: str
    parameters: typing.Dict[str, typing.Union[str, int, bool]]
    async_: bool


class UltraComplex(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    tree: Node
    widgets: typing.List[Widget]
    data: typing.Optional[ComplexData]
    response: UserResponse
    assets: typing.List[Asset]


class Node(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    id: int
    type_: str
    value: typing.Union[str, int, typing.List[Node], typing.Dict[str, Node]]
    metadata: typing.Optional[NodeMetadata]


class NodeMetadata(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    created: str
    modified: str
    tags: typing.List[str]
    attributes: typing.Dict[str, typing.Union[str, int, bool]]


class Widget(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    type_: str
    button: typing.Optional[ButtonWidget]
    text: typing.Optional[TextWidget]
    img: typing.Optional[ImageWidget]
    container: typing.Optional[ContainerWidget]


class ButtonWidget(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    label: str
    action: str
    style: typing.Dict[str, str]


class TextWidget(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    content: str
    format: str
    style: typing.Dict[str, str]


class ImageWidget(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    alt: str
    dimensions: Dimensions


class Dimensions(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    width: int
    height: int


class ContainerWidget(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    layout: str
    children: typing.List[Widget]
    style: typing.Dict[str, str]


class ComplexData(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    primary: PrimaryData
    secondary: typing.Optional[SecondaryData]
    tertiary: typing.Optional[TertiaryData]


class PrimaryData(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    values: typing.List[typing.Union[str, int, float]]
    mappings: typing.Dict[str, typing.Dict[str, str]]
    flags: typing.List[bool]


class SecondaryData(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    records: typing.List[Record]
    index: typing.Dict[str, Record]


class Record(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    id: int
    data: typing.Dict[str, typing.Union[str, int, bool]]
    related: typing.Optional[typing.List[Record]]


class TertiaryData(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    raw: str
    parsed: typing.Optional[typing.Dict[str, str]]
    valid: bool


class UserResponse(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    status: str
    data: typing.Optional[User]
    error: typing.Optional[ErrorDetail]
    metadata: ResponseMetadata


class ErrorDetail(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    code: str
    message: str
    details: typing.Optional[typing.Dict[str, str]]


class ResponseMetadata(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    timestamp: str
    requestId: str
    duration: int
    retries: int


class Asset(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    id: int
    type_: str
    metadata: AssetMetadata
    tags: typing.List[str]


class AssetMetadata(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    filename: str
    size: int
    mimeType: str
    uploaded: str
    checksum: str


TestKitchenSink       = __define_function("root.TestKitchenSink", "sync",  ["input"])
TestKitchenSink_async = __define_function("root.TestKitchenSink", "async", ["input"])


TestUltraComplex       = __define_function("root.TestUltraComplex", "sync",  ["input"])
TestUltraComplex_async = __define_function("root.TestUltraComplex", "async", ["input"])


TestRecursiveComplexity       = __define_function("root.TestRecursiveComplexity", "sync",  ["input"])
TestRecursiveComplexity_async = __define_function("root.TestRecursiveComplexity", "async", ["input"])


__all__ = [
    "KitchenSink",
    "DataObject",
    "Success",
    "Error",
    "User",
    "UserProfile",
    "Setting",
    "Item",
    "Variant",
    "Configuration",
    "Feature",
    "Environment",
    "Rule",
    "Condition",
    "SimpleCondition",
    "Action",
    "UltraComplex",
    "Node",
    "NodeMetadata",
    "Widget",
    "ButtonWidget",
    "TextWidget",
    "ImageWidget",
    "Dimensions",
    "ContainerWidget",
    "ComplexData",
    "PrimaryData",
    "SecondaryData",
    "Record",
    "TertiaryData",
    "UserResponse",
    "ErrorDetail",
    "ResponseMetadata",
    "Asset",
    "AssetMetadata",
    "TestKitchenSink",
    "TestKitchenSink_async",
    "TestUltraComplex",
    "TestUltraComplex_async",
    "TestRecursiveComplexity",
    "TestRecursiveComplexity_async",
]
