import typing
import typing_extensions
from enum import Enum
from pydantic import BaseModel, ConfigDict, Field

import baml_py

CheckT = typing_extensions.TypeVar('CheckT')
CheckName = typing_extensions.TypeVar('CheckName', bound=str)

class Check(BaseModel):
    name: str
    expression: str
    status: str

class Checked(BaseModel, typing.Generic[CheckT, CheckName]):
    value: CheckT
    checks: typing.Dict[CheckName, Check]

def get_checks(checks: typing.Dict[CheckName, Check]) -> typing.List[Check]:
    return list(checks.values())

def all_succeeded(checks: typing.Dict[CheckName, Check]) -> bool:
    return all(check.status == "succeeded" for check in get_checks(checks))


class Action(BaseModel):
    type: str
    parameters: typing.Dict[str, typing.Union[str, int, bool]]
    async_: bool

class Asset(BaseModel):
    id: int
    type: str
    metadata: AssetMetadata
    tags: typing.List[str]

class AssetMetadata(BaseModel):
    filename: str
    size: int
    mimeType: str
    uploaded: str
    checksum: str

class ButtonWidget(BaseModel):
    label: str
    action: str
    style: typing.Dict[str, str]

class ComplexData(BaseModel):
    primary: PrimaryData
    secondary: typing.Optional[SecondaryData]
    tertiary: typing.Union[TertiaryData, None]

class Condition(BaseModel):
    type: str
    conditions: typing.List[typing.Union[Condition, SimpleCondition]]

class Configuration(BaseModel):
    version: str
    features: typing.List[Feature]
    environments: typing.Dict[str, Environment]
    rules: typing.List[Rule]

class ContainerWidget(BaseModel):
    layout: str
    children: typing.List[Widget]
    style: typing.Dict[str, str]

class DataObject(BaseModel):
    type: str
    value: typing.Dict[str, str]

class Dimensions(BaseModel):
    width: int
    height: int

class Environment(BaseModel):
    name: str
    url: str
    variables: typing.Dict[str, str]
    secrets: typing.Optional[typing.Dict[str, str]]

class Error(BaseModel):
    type: str
    message: str
    code: int

class ErrorDetail(BaseModel):
    code: str
    message: str
    details: typing.Optional[typing.Dict[str, str]]

class Feature(BaseModel):
    name: str
    enabled: bool
    config: typing.Optional[typing.Dict[str, typing.Union[str, int, bool]]]
    dependencies: typing.List[str]

class ImageWidget(BaseModel):
    alt: str
    dimensions: Dimensions

class Item(BaseModel):
    id: int
    name: str
    variants: typing.List[Variant]
    attributes: typing.Dict[str, typing.Union[str, int, float, bool]]

class KitchenSink(BaseModel):
    id: int
    name: str
    score: float
    active: bool
    nothing: None
    status: str
    priority: int
    tags: typing.List[str]
    numbers: typing.List[int]
    matrix: typing.List[typing.List[int]]
    metadata: typing.Dict[str, str]
    scores: typing.Dict[str, float]
    description: typing.Optional[str]
    notes: typing.Union[str, None]
    data: typing.Union[str, int, DataObject]
    result: typing.Union[Success, Error]
    user: User
    items: typing.List[Item]
    config: Configuration

class Node(BaseModel):
    id: int
    type: str
    value: typing.Union[str, int, typing.List[Node], typing.Dict[str, Node]]
    metadata: typing.Optional[NodeMetadata]

class NodeMetadata(BaseModel):
    created: str
    modified: str
    tags: typing.List[str]
    attributes: typing.Dict[str, typing.Union[str, int, bool, None]]

class PrimaryData(BaseModel):
    values: typing.List[typing.Union[str, int, float]]
    mappings: typing.Dict[str, typing.Dict[str, str]]
    flags: typing.List[bool]

class Record(BaseModel):
    id: int
    data: typing.Dict[str, typing.Union[str, int, bool, None]]
    related: typing.Optional[typing.List[Record]]

class ResponseMetadata(BaseModel):
    timestamp: str
    requestId: str
    duration: int
    retries: int

class Rule(BaseModel):
    id: int
    name: str
    condition: Condition
    actions: typing.List[Action]
    priority: int

class SecondaryData(BaseModel):
    records: typing.List[Record]
    index: typing.Dict[str, Record]

class Setting(BaseModel):
    key: str
    value: typing.Union[str, int, bool]
    metadata: typing.Optional[typing.Dict[str, str]]

class SimpleCondition(BaseModel):
    field: str
    operator: str
    value: typing.Union[str, int, float, bool]

class Success(BaseModel):
    type: str
    data: typing.Dict[str, str]

class TertiaryData(BaseModel):
    raw: str
    parsed: typing.Optional[typing.Dict[str, str]]
    valid: bool

class TextWidget(BaseModel):
    content: str
    format: str
    style: typing.Dict[str, str]

class UltraComplex(BaseModel):
    tree: Node
    widgets: typing.List[Widget]
    data: typing.Optional[ComplexData]
    response: UserResponse
    assets: typing.List[Asset]

class User(BaseModel):
    id: int
    profile: UserProfile
    settings: typing.Dict[str, Setting]

class UserProfile(BaseModel):
    name: str
    email: str
    bio: typing.Optional[str]
    links: typing.List[str]

class UserResponse(BaseModel):
    status: str
    data: typing.Optional[User]
    error: typing.Optional[ErrorDetail]
    metadata: ResponseMetadata

class Variant(BaseModel):
    sku: str
    price: float
    stock: int
    options: typing.Dict[str, str]

class Widget(BaseModel):
    type: str
    button: typing.Optional[ButtonWidget]
    text: typing.Optional[TextWidget]
    img: typing.Optional[ImageWidget]
    container: typing.Optional[ContainerWidget]